//! What an import will do, in the import's own words.
//!
//! The review screen used to read statement rows and render its own
//! interpretation of them, while [`super::store::import_statement`] resolved
//! accounts, collapsed transfer pairs, suppressed duplicates and converted
//! currency on its own terms. The two could disagree — "34 accounts will be
//! created" against the one the import would actually make was just the
//! visible case.
//!
//! So this is not an estimator. It runs the *real* import inside a transaction
//! and rolls it back, then describes what happened. Promise equals outcome by
//! construction: a second implementation of "what import will do" is exactly
//! the drift that made this necessary.
//!
//! Everything here is derived from what the rolled-back run actually did —
//! which accounts appeared, which matches became linked, which lines ended up
//! in error — rather than from rules restated alongside the import's own.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;

use super::store::{
    ImportOutcome, ImportRequest, default_account_type, import_statement, lookup_external_account,
};

/// One statement of the batch, with the decisions the user has made so far.
#[derive(Debug)]
pub struct PreviewStatement {
    pub id: String,
    pub request: ImportRequest,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    /// The real per-statement outcomes, exactly as `/statements/import` would
    /// have returned them.
    pub statements: Vec<ImportOutcome>,
    /// Target accounts, grouped by identity across every file in the batch.
    pub accounts: Vec<PreviewAccount>,
    /// Pairs that collapse into one transaction naming both accounts.
    pub transfers: Vec<PreviewTransfer>,
    /// Lines suppressed because another file already carries them.
    pub duplicates: Vec<PreviewDuplicate>,
    /// Trades and the like: shown, never imported.
    pub informational: Vec<PreviewLine>,
    pub errors: Vec<PreviewError>,
    pub totals: PreviewTotals,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTotals {
    pub transactions: usize,
    pub skipped: usize,
    pub errors: usize,
    pub accounts_to_create: usize,
    pub accounts_reused: usize,
    pub transfer_pairs: usize,
    pub duplicates: usize,
    pub informational: usize,
    pub files: usize,
}

/// What this import would do about one target account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountResolution {
    /// An account already holds this identity (or the user picked one).
    Existing,
    /// The import would create it. Reported once per identity, however many
    /// files and statements feed it.
    WillCreate,
    /// Nothing was selected from this identity's statements, so the import
    /// would not touch an account at all.
    Untouched,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAccount {
    /// `None` until the row exists. A to-be-created account is *described*,
    /// not promised an id: the id is minted when the row is created, so naming
    /// one here would be a lie the next import could contradict.
    pub account_id: Option<String>,
    pub name: String,
    pub account_type: String,
    pub source: String,
    pub external_id: Option<String>,
    pub resolution: AccountResolution,
    pub statement_ids: Vec<String>,
    /// The dropped files feeding this account, so "created once" can show its
    /// sources.
    pub files: Vec<String>,
    /// Lines this import would actually book onto the account.
    pub transactions: usize,
    /// Existing accounts the user might mean. Only ever a proposal: the server
    /// never silently decides that an account named "Firstrade" is the
    /// Firstrade export.
    pub suggestions: Vec<AccountSuggestion>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSuggestion {
    pub id: String,
    pub name: String,
    pub account_type: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTransfer {
    pub match_id: String,
    pub posted_at: i64,
    pub description: String,
    pub amount: f64,
    pub currency_code: String,
    pub from_account: String,
    pub to_account: String,
    pub debit_line_id: String,
    pub credit_line_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDuplicate {
    pub line_id: String,
    pub posted_at: i64,
    pub description: String,
    pub amount: f64,
    pub currency_code: String,
    /// Which file's copy survives, so the suppression is explainable.
    pub kept_from: Option<String>,
    pub kept_line_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLine {
    pub line_id: String,
    pub posted_at: i64,
    pub description: String,
    pub amount: f64,
    pub currency_code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewError {
    pub line_id: String,
    pub posted_at: i64,
    pub description: String,
    pub amount: f64,
    pub currency_code: String,
    /// The import's own message, e.g. "no USD/EUR FX rate on or before
    /// 2026-02-04" — the hazard the user needs to see *before* clicking.
    pub error: String,
}

/// Run the import, describe it, roll it back.
///
/// Takes the connection rather than a transaction on purpose: the transaction
/// is opened and rolled back here, so no caller can commit a dry run by
/// mistake. It is also the only connection used — a second one would escape
/// the rollback, and against the pooled test connection would simply deadlock.
pub fn preview_import(
    conn: &mut Connection,
    requests: Vec<PreviewStatement>,
    base_currency: &str,
) -> anyhow::Result<ImportPreview> {
    let tx = conn.transaction()?;

    let accounts_before = id_set(&tx, "SELECT id FROM accounts")?;
    let linked_before = id_set(
        &tx,
        "SELECT id FROM transfer_matches WHERE status='linked'",
    )?;

    let statement_ids: Vec<String> = requests.iter().map(|r| r.id.clone()).collect();
    let mut outcomes = Vec::with_capacity(requests.len());
    for request in requests {
        outcomes.push(import_statement(
            &tx,
            &request.id,
            request.request,
            base_currency,
        )?);
    }

    let accounts = describe_accounts(&tx, &statement_ids, &outcomes, &accounts_before)?;
    let transfers = describe_transfers(&tx, &linked_before)?;
    let duplicates = describe_duplicates(&tx, &statement_ids)?;
    let informational = describe_informational(&tx, &statement_ids)?;
    let errors = describe_errors(&tx, &outcomes)?;

    let mut files: HashSet<String> = HashSet::new();
    for account in &accounts {
        files.extend(account.files.iter().cloned());
    }
    let totals = PreviewTotals {
        transactions: outcomes.iter().map(|o| o.created).sum(),
        skipped: outcomes.iter().map(|o| o.skipped).sum(),
        errors: outcomes.iter().map(|o| o.errors).sum(),
        accounts_to_create: accounts
            .iter()
            .filter(|a| a.resolution == AccountResolution::WillCreate)
            .count(),
        accounts_reused: accounts
            .iter()
            .filter(|a| a.resolution == AccountResolution::Existing)
            .count(),
        transfer_pairs: transfers.len(),
        duplicates: duplicates.len(),
        informational: informational.len(),
        files: files.len(),
    };

    // Explicit, not a drop: a rolled-back dry run is the whole contract here.
    tx.rollback()?;

    Ok(ImportPreview {
        statements: outcomes,
        accounts,
        transfers,
        duplicates,
        informational,
        errors,
        totals,
    })
}

fn id_set(tx: &Transaction<'_>, sql: &str) -> anyhow::Result<HashSet<String>> {
    let mut stmt = tx.prepare(sql)?;
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<HashSet<_>, _>>()?;
    Ok(ids)
}

/// One row per statement in the batch, as it stands *after* the rolled-back
/// import — so `account_id` is whatever the import resolved it to.
#[derive(Debug)]
struct StatementAfter {
    id: String,
    source: String,
    account_name: String,
    account_number: Option<String>,
    account_id: Option<String>,
    source_path: Option<String>,
}

fn describe_accounts(
    tx: &Transaction<'_>,
    statement_ids: &[String],
    outcomes: &[ImportOutcome],
    accounts_before: &HashSet<String>,
) -> anyhow::Result<Vec<PreviewAccount>> {
    let mut booked: HashMap<&str, usize> = HashMap::new();
    for outcome in outcomes {
        let created = outcome
            .lines
            .iter()
            .filter(|line| line.status == "imported" && line.transaction_id.is_some())
            .count();
        *booked.entry(outcome.statement_id.as_str()).or_insert(0) += created;
    }

    // BTreeMap so the order is stable for the UI across re-previews.
    let mut groups: BTreeMap<String, PreviewAccount> = BTreeMap::new();
    for statement_id in statement_ids {
        let Some(statement) = load_statement_after(tx, statement_id)? else {
            continue;
        };
        // An account the import resolved wins; otherwise ask the same lookup
        // the import would have asked.
        let resolved = match statement.account_id.clone() {
            Some(id) => Some(id),
            None => match statement.account_number.as_deref() {
                Some(external_id) => lookup_external_account(tx, &statement.source, external_id)?,
                None => None,
            },
        };

        let transactions = booked.get(statement.id.as_str()).copied().unwrap_or(0);
        // An account this run minted inside the rollback does not exist from
        // the user's side, so it is described like any other proposal: its
        // name and type are the ones the import would use, but it gets no id
        // (the id is minted when the row is really created) and it still
        // carries suggestions.
        let existing = resolved
            .as_ref()
            .filter(|id| accounts_before.contains(id.as_str()))
            .cloned();
        let identity_key = format!(
            "new:{}:{}",
            statement.source,
            statement.account_number.as_deref().unwrap_or("")
        );
        let (key, mut account) = match existing {
            Some(account_id) => {
                let (name, account_type) = account_row(tx, &account_id)?.unwrap_or_else(|| {
                    (
                        statement.account_name.clone(),
                        default_account_type(&statement.source, &statement.account_name)
                            .to_string(),
                    )
                });
                (
                    format!("id:{account_id}"),
                    PreviewAccount {
                        account_id: Some(account_id),
                        name,
                        account_type,
                        source: statement.source.clone(),
                        external_id: statement.account_number.clone(),
                        resolution: AccountResolution::Existing,
                        statement_ids: Vec::new(),
                        files: Vec::new(),
                        transactions: 0,
                        suggestions: Vec::new(),
                    },
                )
            }
            None => {
                // The run's own row when it created one (the import's words),
                // falling back to the statement's description when nothing was
                // selected and no row was made.
                let (name, account_type) = match resolved.as_deref() {
                    Some(id) => account_row(tx, id)?,
                    None => None,
                }
                .unwrap_or_else(|| {
                    (
                        statement.account_name.clone(),
                        default_account_type(&statement.source, &statement.account_name)
                            .to_string(),
                    )
                });
                let resolution = if resolved.is_some() || transactions > 0 {
                    AccountResolution::WillCreate
                } else {
                    AccountResolution::Untouched
                };
                (
                    identity_key,
                    PreviewAccount {
                        account_id: None,
                        name,
                        account_type,
                        source: statement.source.clone(),
                        external_id: statement.account_number.clone(),
                        resolution,
                        statement_ids: Vec::new(),
                        files: Vec::new(),
                        transactions: 0,
                        suggestions: suggest_accounts(
                            tx,
                            &statement.source,
                            &statement.account_name,
                            accounts_before,
                        )?,
                    },
                )
            }
        };

        let entry = groups.entry(key).or_insert_with(|| {
            account.statement_ids = Vec::new();
            account
        });
        entry.statement_ids.push(statement.id.clone());
        entry.transactions += transactions;
        if let Some(path) = statement.source_path.clone() {
            if !entry.files.contains(&path) {
                entry.files.push(path);
            }
        }
        // A group is "will create" if any of its statements would create it.
        if entry.resolution == AccountResolution::Untouched && transactions > 0 {
            entry.resolution = AccountResolution::WillCreate;
        }
    }
    Ok(groups.into_values().collect())
}

fn load_statement_after(
    tx: &Transaction<'_>,
    id: &str,
) -> anyhow::Result<Option<StatementAfter>> {
    let row = tx
        .query_row(
            "SELECT id, source, account_name, account_number, account_id, source_path
             FROM bank_statements WHERE id=?1",
            params![id],
            |r| {
                Ok(StatementAfter {
                    id: r.get(0)?,
                    source: r.get(1)?,
                    account_name: r.get(2)?,
                    account_number: r.get(3)?,
                    account_id: r.get(4)?,
                    source_path: r.get(5)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

fn account_row(tx: &Transaction<'_>, id: &str) -> anyhow::Result<Option<(String, String)>> {
    let row = tx
        .query_row(
            "SELECT name, type FROM accounts WHERE id=?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row)
}

/// Existing accounts the user might have meant, for an identity with none.
///
/// A proposal, never an action: accepting one writes the identity onto the
/// chosen account through `PUT /accounts/:id/identity`, which turns it into an
/// exact match from then on — so the guessing never has to be trusted twice.
fn suggest_accounts(
    tx: &Transaction<'_>,
    source: &str,
    account_name: &str,
    accounts_before: &HashSet<String>,
) -> anyhow::Result<Vec<AccountSuggestion>> {
    let mut stmt = tx.prepare(
        "SELECT id, name, type, source, external_id FROM accounts
         WHERE deleted_at IS NULL
           AND (source = ?1 OR (source IS NULL AND external_id IS NULL))
         ORDER BY name",
    )?;
    let needle = account_name.trim().to_lowercase();
    let rows = stmt
        .query_map(params![source], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut suggestions = Vec::new();
    for (id, name, account_type, row_source, external_id) in rows {
        // Never suggest an account this very run created: it is the proposal,
        // not an alternative to it.
        if !accounts_before.contains(&id) {
            continue;
        }
        let lower = name.trim().to_lowercase();
        let reason = if row_source.as_deref() == Some(source) {
            match external_id {
                Some(other) => format!("another {source} account ({other})"),
                None => format!("another {source} account"),
            }
        } else if !needle.is_empty() && (lower.contains(&needle) || needle.contains(&lower)) {
            "name matches the statement".to_string()
        } else {
            continue;
        };
        suggestions.push(AccountSuggestion {
            id,
            name,
            account_type,
            reason,
        });
    }
    Ok(suggestions)
}

fn describe_transfers(
    tx: &Transaction<'_>,
    linked_before: &HashSet<String>,
) -> anyhow::Result<Vec<PreviewTransfer>> {
    let mut stmt = tx.prepare(
        "SELECT m.id, d.id, c.id, d.posted_at, d.description, d.amount, d.currency_code,
                COALESCE(da.name, ds.account_name), COALESCE(ca.name, cs.account_name)
         FROM transfer_matches m
         JOIN bank_statement_lines d ON d.id = m.debit_line_id
         JOIN bank_statement_lines c ON c.id = m.credit_line_id
         JOIN bank_statements ds ON ds.id = d.statement_id
         JOIN bank_statements cs ON cs.id = c.statement_id
         LEFT JOIN accounts da ON da.id = ds.account_id
         LEFT JOIN accounts ca ON ca.id = cs.account_id
         WHERE m.status='linked'
         ORDER BY d.posted_at",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(PreviewTransfer {
                match_id: r.get(0)?,
                debit_line_id: r.get(1)?,
                credit_line_id: r.get(2)?,
                posted_at: r.get(3)?,
                description: r.get(4)?,
                amount: r.get::<_, f64>(5)?.abs(),
                currency_code: r.get(6)?,
                from_account: r.get(7)?,
                to_account: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    // Only the pairs *this* run linked: ones already linked are old news.
    Ok(rows
        .into_iter()
        .filter(|row| !linked_before.contains(&row.match_id))
        .collect())
}

fn describe_duplicates(
    tx: &Transaction<'_>,
    statement_ids: &[String],
) -> anyhow::Result<Vec<PreviewDuplicate>> {
    let mut stmt = tx.prepare(
        "SELECT l.id, l.posted_at, l.description, l.amount, l.currency_code,
                k.id, ks.source_path
         FROM bank_statement_lines l
         LEFT JOIN bank_statement_lines k ON k.id = l.duplicate_of
         LEFT JOIN bank_statements ks ON ks.id = k.statement_id
         WHERE l.statement_id = ?1 AND l.status='duplicate'
         ORDER BY l.posted_at",
    )?;
    let mut out = Vec::new();
    for statement_id in statement_ids {
        let rows = stmt.query_map(params![statement_id], |r| {
            Ok(PreviewDuplicate {
                line_id: r.get(0)?,
                posted_at: r.get(1)?,
                description: r.get(2)?,
                amount: r.get(3)?,
                currency_code: r.get(4)?,
                kept_line_id: r.get(5)?,
                kept_from: r.get(6)?,
            })
        })?;
        for row in rows {
            out.push(row?);
        }
    }
    Ok(out)
}

fn describe_informational(
    tx: &Transaction<'_>,
    statement_ids: &[String],
) -> anyhow::Result<Vec<PreviewLine>> {
    let mut stmt = tx.prepare(
        "SELECT id, posted_at, description, amount, currency_code
         FROM bank_statement_lines
         WHERE statement_id = ?1 AND status='informational'
         ORDER BY posted_at",
    )?;
    let mut out = Vec::new();
    for statement_id in statement_ids {
        let rows = stmt.query_map(params![statement_id], |r| {
            Ok(PreviewLine {
                line_id: r.get(0)?,
                posted_at: r.get(1)?,
                description: r.get(2)?,
                amount: r.get(3)?,
                currency_code: r.get(4)?,
            })
        })?;
        for row in rows {
            out.push(row?);
        }
    }
    Ok(out)
}

/// Per-line failures, as the import reported them.
///
/// `rate_on_or_before` fails an import with "no USD/EUR FX rate on or before
/// <date>", and Firstrade is entirely USD — so this is the path that makes an
/// FX gap visible before the button is pressed rather than after.
fn describe_errors(
    tx: &Transaction<'_>,
    outcomes: &[ImportOutcome],
) -> anyhow::Result<Vec<PreviewError>> {
    let mut out = Vec::new();
    for outcome in outcomes {
        for line in &outcome.lines {
            if line.status != "error" {
                continue;
            }
            let row = tx
                .query_row(
                    "SELECT posted_at, description, amount, currency_code
                     FROM bank_statement_lines WHERE id=?1",
                    params![line.line_id],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, f64>(2)?,
                            r.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()?;
            let (posted_at, description, amount, currency_code) =
                row.unwrap_or((0, String::new(), 0.0, String::new()));
            out.push(PreviewError {
                line_id: line.line_id.clone(),
                posted_at,
                description,
                amount,
                currency_code,
                error: line
                    .error
                    .clone()
                    .unwrap_or_else(|| "import failed".to_string()),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::statements::{
        DocumentKind, ParseContext, Parser, RawDocument, ToDatabaseRecord, hash_documents,
        store::ImportAction,
        t212_csv::Trading212CsvSource,
    };

    const HEADER: &str = "Action,Time (UTC),ISIN,Ticker,Name,Notes,ID,No. of shares,Price / share,Currency (Price / share),Exchange rate,Total,Currency (Total),Currency conversion fee,Currency (Currency conversion fee),Merchant name,Merchant category";

    fn stage(pool: &crate::db::DbPool, csv: &str, name: &str) -> String {
        let docs = vec![RawDocument::file(
            DocumentKind::Csv,
            csv.as_bytes().to_vec(),
            PathBuf::from(name),
        )];
        let ctx = ParseContext {
            source_hash: hash_documents(&docs),
            stored_path: None,
            source_path: Some(name.to_string()),
            fetched_at: 0,
        };
        let parsed = Trading212CsvSource::new(PathBuf::from(name))
            .parse(&docs, &ctx)
            .unwrap();
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        let outcomes = parsed.save(&tx).unwrap();
        tx.commit().unwrap();
        outcomes[0].statement_id.clone()
    }

    fn request(id: &str) -> PreviewStatement {
        PreviewStatement {
            id: id.to_string(),
            request: ImportRequest {
                lines: Vec::new(),
                account_id: None,
                counterpart_account_id: None,
                auto_link: Some(true),
            },
        }
    }

    fn preview(pool: &crate::db::DbPool, ids: &[&str]) -> ImportPreview {
        let mut conn = pool.get().unwrap();
        preview_import(
            &mut conn,
            ids.iter().map(|id| request(id)).collect(),
            "EUR",
        )
        .unwrap()
    }

    /// Every row of every table an import can touch, as JSON.
    fn snapshot(pool: &crate::db::DbPool) -> String {
        let conn = pool.get().unwrap();
        let mut out = String::new();
        for table in [
            "accounts",
            "transactions",
            "bank_statements",
            "bank_statement_lines",
            "transfer_matches",
        ] {
            let mut stmt = conn
                .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
                .unwrap();
            let columns = stmt.column_count();
            let rows = stmt
                .query_map([], |r| {
                    let mut row = Vec::new();
                    for i in 0..columns {
                        row.push(format!("{:?}", r.get_ref(i).unwrap()));
                    }
                    Ok(row.join("|"))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            out.push_str(table);
            out.push('\n');
            out.push_str(&rows.join("\n"));
            out.push('\n');
        }
        out
    }

    fn csv(rows: &[(&str, &str, &str, f64)]) -> String {
        let mut text = format!("{HEADER}\n");
        for (action, id, when, amount) in rows {
            text.push_str(&format!(
                "{action},{when} 12:00:00+00:00,,,,,{id},,,,,{amount:.2},\"EUR\",,,,\n"
            ));
        }
        text
    }

    /// The contract: a dry run leaves the database exactly as it found it.
    /// Asserted by comparing every row of every table an import writes to,
    /// including that no account was created.
    #[test]
    fn a_preview_leaves_the_database_untouched() {
        let pool = crate::db::init_memory().unwrap();
        let id = stage(
            &pool,
            &csv(&[
                ("Card debit", "a", "2026-02-01", -10.0),
                ("Deposit", "b", "2026-02-02", 100.0),
            ]),
            "one.csv",
        );

        let before = snapshot(&pool);
        let result = preview(&pool, &[&id]);
        assert_eq!(result.totals.transactions, 2, "it really did import them");
        assert_eq!(
            result.totals.accounts_to_create, 1,
            "and really did create the account"
        );
        assert_eq!(snapshot(&pool), before, "...and then undid all of it");

        let conn = pool.get().unwrap();
        let accounts: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(accounts, 0, "no account survived the dry run");
    }

    /// Importing straight after a preview produces exactly the numbers the
    /// preview promised. This is the property the whole design exists for.
    #[test]
    fn importing_afterwards_produces_the_previewed_numbers() {
        let pool = crate::db::init_memory().unwrap();
        let id = stage(
            &pool,
            &csv(&[
                ("Card debit", "a", "2026-02-01", -10.0),
                ("Market buy", "t", "2026-02-02", 0.0),
                ("Deposit", "b", "2026-02-03", 100.0),
            ]),
            "one.csv",
        );
        let promised = preview(&pool, &[&id]);

        let outcome = {
            let mut conn = pool.get().unwrap();
            let tx = conn.transaction().unwrap();
            let outcome = import_statement(
                &tx,
                &id,
                ImportRequest {
                    lines: Vec::new(),
                    account_id: None,
                    counterpart_account_id: None,
                    auto_link: Some(true),
                },
                "EUR",
            )
            .unwrap();
            tx.commit().unwrap();
            outcome
        };

        assert_eq!(outcome.created, promised.totals.transactions);
        assert_eq!(outcome.errors, promised.totals.errors);
        let conn = pool.get().unwrap();
        let accounts: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(accounts as usize, promised.totals.accounts_to_create);
        assert_eq!(promised.totals.informational, 1, "the trade is reported");
    }

    /// Two files of the same source and account are one account to create,
    /// listing both files — not one proposal each.
    #[test]
    fn two_files_of_one_identity_are_one_account_to_create() {
        let pool = crate::db::init_memory().unwrap();
        let first = stage(
            &pool,
            &csv(&[("Card debit", "a", "2026-02-01", -10.0)]),
            "one.csv",
        );
        let second = stage(
            &pool,
            &csv(&[("Card debit", "b", "2026-03-01", -20.0)]),
            "two.csv",
        );

        let result = preview(&pool, &[&first, &second]);
        assert_eq!(result.accounts.len(), 1, "one identity, one account row");
        let account = &result.accounts[0];
        assert_eq!(account.resolution, AccountResolution::WillCreate);
        assert_eq!(account.account_id, None, "an unmade account has no id");
        assert_eq!(account.account_type, "investment");
        assert_eq!(account.statement_ids.len(), 2);
        assert_eq!(account.files, vec!["one.csv", "two.csv"]);
        assert_eq!(account.transactions, 2);
        assert_eq!(result.totals.accounts_to_create, 1);
        assert_eq!(result.totals.files, 2);
    }

    /// Once the account exists the preview says so, and names it.
    #[test]
    fn an_existing_account_is_reported_as_reused_with_its_id() {
        let pool = crate::db::init_memory().unwrap();
        let id = stage(
            &pool,
            &csv(&[("Card debit", "a", "2026-02-01", -10.0)]),
            "one.csv",
        );
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "INSERT INTO accounts (id, name, type, opening_balance, note, created_at,
                                       updated_at, source, external_id)
                 VALUES ('mine','My broker','investment',0.0,NULL,1,1,'trading212','csv')",
                [],
            )
            .unwrap();
        }

        let result = preview(&pool, &[&id]);
        let account = &result.accounts[0];
        assert_eq!(account.resolution, AccountResolution::Existing);
        assert_eq!(account.account_id.as_deref(), Some("mine"));
        assert_eq!(account.name, "My broker");
        assert_eq!(result.totals.accounts_to_create, 0);
        assert_eq!(result.totals.accounts_reused, 1);
    }

    /// An identity with no account lists candidates, and never acts on them.
    #[test]
    fn an_unmatched_identity_suggests_candidates() {
        let pool = crate::db::init_memory().unwrap();
        let id = stage(
            &pool,
            &csv(&[("Card debit", "a", "2026-02-01", -10.0)]),
            "one.csv",
        );
        {
            let conn = pool.get().unwrap();
            conn.execute(
                "INSERT INTO accounts (id, name, type, opening_balance, note, created_at, updated_at)
                 VALUES ('hand','Trading 212','investment',0.0,NULL,1,1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO accounts (id, name, type, opening_balance, note, created_at, updated_at)
                 VALUES ('other','Groceries','bank',0.0,NULL,1,1)",
                [],
            )
            .unwrap();
        }

        let result = preview(&pool, &[&id]);
        let account = &result.accounts[0];
        assert_eq!(
            account.resolution,
            AccountResolution::WillCreate,
            "a suggestion is not a match: the import still creates one"
        );
        let names: Vec<&str> = account
            .suggestions
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, ["Trading 212"], "only the plausible one");
    }

    /// A transfer between two of your own accounts is one transaction, and the
    /// preview says so naming both sides — rather than two rows the user has
    /// to reconcile mentally.
    #[test]
    fn a_transfer_pair_is_reported_as_one_merged_row() {
        use crate::statements::draft::{LineKind, StatementDraft, StatementLineDraft};
        use crate::statements::{SourceId, store::save_statement};

        fn draft(key: &str, account: &str, amount: f64, kind: LineKind) -> StatementDraft {
            StatementDraft {
                source: SourceId::CreditAgricole,
                institution: "Crédit Agricole".into(),
                account_name: account.into(),
                account_number: Some(format!("num-{account}")),
                currency_code: "EUR".into(),
                period_start: Some("2026-02-01".into()),
                period_end: Some("2026-02-28".into()),
                opening_balance: None,
                closing_balance: None,
                source_path: Some("ca.csv".into()),
                stored_path: None,
                source_hash: format!("hash-{key}"),
                source_key: key.into(),
                parsed: serde_json::json!({}),
                warnings: Vec::new(),
                lines: vec![StatementLineDraft {
                    source_ref: None,
                    posted_at: 1_770_000_000_000,
                    description: "VIREMENT INTERNE".into(),
                    amount,
                    currency_code: "EUR".into(),
                    balance_after: None,
                    category_hint: None,
                    line_kind: kind,
                    suggested_type: None,
                    suggested_category: None,
                    metadata: serde_json::json!({}),
                    fingerprint: format!("fp-{key}"),
                    dedupe_key: format!("dd-{key}"),
                }],
            }
        }

        let pool = crate::db::init_memory().unwrap();
        let (out_id, in_id) = {
            let mut conn = pool.get().unwrap();
            let tx = conn.transaction().unwrap();
            let out = save_statement(
                &tx,
                draft("ca:cheques", "Compte de chèques", -50.0, LineKind::TransferOut),
            )
            .unwrap();
            let into = save_statement(
                &tx,
                draft("ca:eko", "Eko by CA", 50.0, LineKind::TransferIn),
            )
            .unwrap();
            crate::statements::store::match_pending(&tx).unwrap();
            tx.commit().unwrap();
            (out.statement_id, into.statement_id)
        };

        let result = preview(&pool, &[&out_id, &in_id]);
        assert_eq!(result.totals.transfer_pairs, 1, "two lines, one transfer");
        let transfer = &result.transfers[0];
        assert_eq!(transfer.from_account, "Compte de chèques");
        assert_eq!(transfer.to_account, "Eko by CA");
        assert_eq!(transfer.amount, 50.0);
        assert_eq!(
            result.totals.transactions, 1,
            "the pair books one transaction, not two"
        );
        assert_eq!(
            result.totals.accounts_to_create, 2,
            "both sides' accounts are proposed, once each"
        );
    }

    /// A duplicate is reported as a suppression, naming the file whose copy
    /// survives — rather than two unexplained rows.
    #[test]
    fn a_duplicate_names_the_file_it_was_kept_from() {
        let pool = crate::db::init_memory().unwrap();
        let first = stage(
            &pool,
            &csv(&[("Card debit", "shared", "2026-02-01", -10.0)]),
            "one.csv",
        );
        let second = stage(
            &pool,
            &csv(&[
                ("Card debit", "shared", "2026-02-01", -10.0),
                ("Card debit", "own", "2026-02-05", -5.0),
            ]),
            "two.csv",
        );

        let result = preview(&pool, &[&first, &second]);
        assert_eq!(result.totals.duplicates, 1);
        let duplicate = &result.duplicates[0];
        assert_eq!(duplicate.kept_from.as_deref(), Some("one.csv"));
        assert_eq!(result.totals.transactions, 2, "the repeat is not booked");
    }

    /// A missing FX rate is a per-line error the user sees before clicking,
    /// not a failed import afterwards.
    #[test]
    fn a_missing_fx_rate_is_reported_per_line() {
        let pool = crate::db::init_memory().unwrap();
        let id = stage(
            &pool,
            &csv(&[("Card debit", "a", "2026-02-01", -10.0)]),
            "one.csv",
        );
        // The statement is in EUR; previewing against a USD base needs a rate
        // that does not exist.
        let result = {
            let mut conn = pool.get().unwrap();
            preview_import(&mut conn, vec![request(&id)], "USD").unwrap()
        };
        assert_eq!(result.totals.errors, 1);
        assert!(
            result.errors[0].error.contains("EUR/USD FX rate"),
            "the message names the missing rate: {}",
            result.errors[0].error
        );
        assert_eq!(result.errors[0].currency_code, "EUR");
    }

    /// The desktop reads these field names directly (`maia-ui/src/lib/api.ts`,
    /// `ImportPreview`), and nothing but this test connects the two sides.
    #[test]
    fn the_payload_keys_are_the_ones_the_desktop_reads() {
        let pool = crate::db::init_memory().unwrap();
        let id = stage(
            &pool,
            &csv(&[("Card debit", "a", "2026-02-01", -10.0)]),
            "one.csv",
        );
        let json = serde_json::to_value(preview(&pool, &[&id])).unwrap();

        for key in [
            "statements",
            "accounts",
            "transfers",
            "duplicates",
            "informational",
            "errors",
            "totals",
        ] {
            assert!(json.get(key).is_some(), "missing {key}: {json}");
        }
        for key in [
            "transactions",
            "skipped",
            "errors",
            "accountsToCreate",
            "accountsReused",
            "transferPairs",
            "duplicates",
            "informational",
            "files",
        ] {
            assert!(json["totals"].get(key).is_some(), "missing totals.{key}");
        }
        let account = &json["accounts"][0];
        for key in [
            "accountId",
            "name",
            "accountType",
            "source",
            "externalId",
            "resolution",
            "statementIds",
            "files",
            "transactions",
            "suggestions",
        ] {
            assert!(account.get(key).is_some(), "missing accounts[].{key}");
        }
        assert_eq!(
            account["resolution"], "willCreate",
            "the resolution is a camelCase string, as the desktop's union expects"
        );
        assert_eq!(json["statements"][0]["statementId"], serde_json::json!(id));
    }

    /// Lines the user skipped are not counted as transactions.
    #[test]
    fn a_skipped_line_is_not_promised_as_a_transaction() {
        let pool = crate::db::init_memory().unwrap();
        let id = stage(
            &pool,
            &csv(&[
                ("Card debit", "a", "2026-02-01", -10.0),
                ("Card debit", "b", "2026-02-02", -20.0),
            ]),
            "one.csv",
        );
        let line_ids: Vec<String> = {
            let conn = pool.get().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT id FROM bank_statement_lines WHERE statement_id=?1 ORDER BY posted_at",
                )
                .unwrap();
            let ids = stmt
                .query_map(params![id], |r| r.get(0))
                .unwrap()
                .collect::<Result<Vec<String>, _>>()
                .unwrap();
            ids
        };

        let mut conn = pool.get().unwrap();
        let result = preview_import(
            &mut conn,
            vec![PreviewStatement {
                id: id.clone(),
                request: ImportRequest {
                    lines: vec![crate::statements::store::ImportLine {
                        id: line_ids[0].clone(),
                        action: ImportAction::Skip,
                        type_: None,
                        category: None,
                        account_id: None,
                        counterpart_account_id: None,
                    }],
                    account_id: None,
                    counterpart_account_id: None,
                    auto_link: Some(true),
                },
            }],
            "EUR",
        )
        .unwrap();
        assert_eq!(result.totals.transactions, 0);
        assert_eq!(result.totals.skipped, 1);
    }
}
