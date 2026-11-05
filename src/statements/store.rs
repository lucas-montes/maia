//! Persistence for staged statements, duplicate detection, transfer matching
//! and explicit import into draft transactions.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, bail};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use super::{
    ACCOUNT_NS, LINE_NS, STATEMENT_NS, TRANSACTION_NS, TRANSFER_NS,
    draft::{SaveOutcome, StatementDraft, SuggestedType},
    stable_uuid,
};
use crate::db;

#[derive(Default, Debug, Clone, Copy)]
struct Counts {
    total: usize,
    pending: usize,
    imported: usize,
    skipped: usize,
    informational: usize,
    duplicate: usize,
    error: usize,
}

/// Stage one statement and its lines.
///
/// Rules that make re-runs safe:
/// - The same `source_key` and `source_hash` is a no-op.
/// - The same `source_key` with different bytes (a feed re-sync) updates the
///   header and replaces only `pending`/`informational` lines; imported and
///   skipped lines survive.
/// - A line whose `dedupe_key` already exists in another statement is stored as
///   `duplicate`, visible but not importable.
pub fn save_statement(tx: &Transaction<'_>, draft: StatementDraft) -> anyhow::Result<SaveOutcome> {
    let now = db::server_time_ms();
    let parsed_json = serde_json::to_string(&draft.parsed).unwrap_or_else(|_| "null".into());
    let warnings_json = serde_json::to_string(&draft.warnings).unwrap_or_else(|_| "[]".into());

    let existing: Option<(String, String, bool)> = tx
        .query_row(
            "SELECT id, source_hash, deleted_at IS NOT NULL
             FROM bank_statements WHERE source=?1 AND source_key=?2",
            params![draft.source.as_str(), draft.source_key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;

    let (statement_id, skipped_reason) = match existing {
        // A deleted statement is not a no-op: dropping the file again is how a
        // user recovers one they dismissed by mistake. The row is revived below
        // rather than re-inserted, because `(source, source_key)` is unique and
        // the feed buckets must keep one row per month.
        Some((id, hash, false)) if hash == draft.source_hash => {
            let counts = counts(tx, &id)?;
            return Ok(SaveOutcome {
                statement_id: id,
                source: draft.source.as_str().to_string(),
                account_name: draft.account_name,
                account_number: draft.account_number,
                inserted_lines: 0,
                duplicate_lines: 0,
                pending_lines: counts.pending,
                informational_lines: counts.informational,
                warnings: draft.warnings,
                skipped_reason: Some("already ingested".to_string()),
            });
        }
        Some((id, _, _)) => {
            tx.execute(
                "UPDATE bank_statements SET
                    institution=?1, account_name=?2, account_number=?3,
                    currency_code=?4, period_start=?5, period_end=?6,
                    opening_balance=?7, closing_balance=?8, source_path=?9,
                    stored_path=?10, source_hash=?11, status='staged',
                    parsed_json=?12, parse_error=NULL, warnings=?13, updated_at=?14,
                    deleted_at=NULL
                 WHERE id=?15",
                params![
                    draft.institution,
                    draft.account_name,
                    draft.account_number,
                    draft.currency_code,
                    draft.period_start,
                    draft.period_end,
                    draft.opening_balance,
                    draft.closing_balance,
                    draft.source_path,
                    draft.stored_path,
                    draft.source_hash,
                    parsed_json,
                    warnings_json,
                    now,
                    id,
                ],
            )?;
            // Imported and skipped lines are decisions the user already made.
            // Pending/informational lines are safe to replace.
            tx.execute(
                "DELETE FROM bank_statement_lines
                 WHERE statement_id=?1 AND status IN ('pending','informational')",
                params![id],
            )?;
            (id, None)
        }
        None => {
            let id = stable_uuid(
                STATEMENT_NS,
                &format!("statement:{}:{}", draft.source.as_str(), draft.source_key),
            );
            tx.execute(
                "INSERT INTO bank_statements
                    (id, source, institution, account_name, account_number, account_id,
                     currency_code, period_start, period_end, opening_balance,
                     closing_balance, source_path, stored_path, source_hash, source_key,
                     status, parsed_json, parse_error, warnings, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,NULL,?6,?7,?8,?9,?10,?11,?12,?13,?14,
                         'staged',?15,NULL,?16,?17,?17)",
                params![
                    id,
                    draft.source.as_str(),
                    draft.institution,
                    draft.account_name,
                    draft.account_number,
                    draft.currency_code,
                    draft.period_start,
                    draft.period_end,
                    draft.opening_balance,
                    draft.closing_balance,
                    draft.source_path,
                    draft.stored_path,
                    draft.source_hash,
                    draft.source_key,
                    parsed_json,
                    warnings_json,
                    now,
                ],
            )?;
            (id, None)
        }
    };

    let mut inserted = 0usize;
    let mut duplicate_lines = 0usize;
    for line in &draft.lines {
        let duplicate_of: Option<String> = tx
            .query_row(
                "SELECT id FROM bank_statement_lines
                 WHERE dedupe_key=?1 AND statement_id<>?2 AND status<>'duplicate'
                 ORDER BY created_at LIMIT 1",
                params![line.dedupe_key, statement_id],
                |r| r.get(0),
            )
            .optional()?;

        let status = if line.line_kind.is_informational() {
            "informational"
        } else if duplicate_of.is_some() {
            "duplicate"
        } else {
            "pending"
        };
        let id = stable_uuid(
            LINE_NS,
            &format!("line:{}:{}", statement_id, line.fingerprint),
        );
        let metadata = serde_json::to_string(&line.metadata).unwrap_or_else(|_| "null".into());

        let changed = tx.execute(
            "INSERT OR IGNORE INTO bank_statement_lines
                (id, statement_id, source_ref, posted_at, description, amount,
                 currency_code, balance_after, category_hint, line_kind,
                 suggested_type, suggested_category, metadata_json, fingerprint,
                 dedupe_key, duplicate_of, status, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?18)",
            params![
                id,
                statement_id,
                line.source_ref,
                line.posted_at,
                line.description,
                line.amount,
                line.currency_code,
                line.balance_after,
                line.category_hint,
                line.line_kind.as_str(),
                line.suggested_type.map(|t| t.as_str()),
                line.suggested_category,
                metadata,
                line.fingerprint,
                line.dedupe_key,
                duplicate_of,
                status,
                now,
            ],
        )?;
        if changed == 0 {
            // Same fingerprint already present, normally an imported line from a
            // previous partial import. The decision wins over the re-parse.
            continue;
        }
        match status {
            "pending" => inserted += 1,
            "duplicate" => duplicate_lines += 1,
            _ => {}
        }
    }

    // Resolve the provider account if the user already mapped it. If not, the
    // review screen shows "new account" and import creates it.
    if let Some(external_id) = draft.account_number.as_deref() {
        let account_id: Option<String> = tx
            .query_row(
                "SELECT id FROM accounts WHERE source=?1 AND external_id=?2",
                params![draft.source.as_str(), external_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(account_id) = account_id {
            tx.execute(
                "UPDATE bank_statements SET account_id=?1 WHERE id=?2",
                params![account_id, statement_id],
            )?;
        }
    }

    refresh_statement_status(tx, &statement_id, now)?;
    let counts = counts(tx, &statement_id)?;
    Ok(SaveOutcome {
        statement_id,
        source: draft.source.as_str().to_string(),
        account_name: draft.account_name,
        account_number: draft.account_number,
        inserted_lines: inserted,
        duplicate_lines,
        pending_lines: counts.pending,
        informational_lines: counts.informational,
        warnings: draft.warnings,
        skipped_reason,
    })
}

fn counts(tx: &Transaction<'_>, statement_id: &str) -> anyhow::Result<Counts> {
    let mut counts = Counts::default();
    let mut stmt = tx.prepare(
        "SELECT status, COUNT(*) FROM bank_statement_lines
         WHERE statement_id=?1 GROUP BY status",
    )?;
    let mut rows = stmt.query(params![statement_id])?;
    while let Some(row) = rows.next()? {
        let status: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        let count = count.max(0) as usize;
        counts.total += count;
        match status.as_str() {
            "pending" => counts.pending += count,
            "imported" => counts.imported += count,
            "skipped" => counts.skipped += count,
            "informational" => counts.informational += count,
            "duplicate" => counts.duplicate += count,
            "error" => counts.error += count,
            _ => {}
        }
    }
    Ok(counts)
}

fn refresh_statement_status(
    tx: &Transaction<'_>,
    statement_id: &str,
    now: i64,
) -> anyhow::Result<()> {
    let counts = counts(tx, statement_id)?;
    let status = if counts.pending > 0 {
        if counts.imported > 0 || counts.skipped > 0 {
            "partial"
        } else {
            "staged"
        }
    } else if counts.imported > 0 || counts.skipped > 0 {
        "imported"
    } else {
        "staged"
    };
    tx.execute(
        "UPDATE bank_statements SET status=?1, updated_at=?2 WHERE id=?3",
        params![status, now, statement_id],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Transfer matching
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct PendingLine {
    id: String,
    posted_at: i64,
    amount: f64,
    currency_code: String,
    description: String,
    metadata: serde_json::Value,
    source: String,
    account_number: Option<String>,
    account_id: Option<String>,
}

/// Find internal and cross-source transfer pairs among `pending` lines.
///
/// Greedy by confidence: each line participates in at most one match, so a
/// repeated amount cannot auto-link one debit to two credits. Existing matches
/// (including dismissed ones) are left alone.
pub fn match_pending(tx: &Transaction<'_>) -> anyhow::Result<()> {
    let mut stmt = tx.prepare(
        "SELECT l.id, l.posted_at, l.amount, l.currency_code, l.description,
                l.metadata_json, s.source, s.account_number, s.account_id
         FROM bank_statement_lines l
         JOIN bank_statements s ON s.id = l.statement_id
         WHERE l.status='pending' AND s.deleted_at IS NULL
           AND NOT EXISTS (
             SELECT 1 FROM transfer_matches m
             WHERE (m.debit_line_id=l.id OR m.credit_line_id=l.id)
               AND m.status IN ('auto','suggested','linked')
           )
         ORDER BY l.posted_at",
    )?;
    let lines = stmt
        .query_map([], |row| {
            let metadata: Option<String> = row.get(5)?;
            Ok(PendingLine {
                id: row.get(0)?,
                posted_at: row.get(1)?,
                amount: row.get(2)?,
                currency_code: row.get(3)?,
                description: row.get(4)?,
                metadata: metadata
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(serde_json::Value::Null),
                source: row.get(6)?,
                account_number: row.get(7)?,
                account_id: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut candidates: Vec<(usize, usize, f64, String)> = Vec::new();
    for i in 0..lines.len() {
        for j in (i + 1)..lines.len() {
            let a = &lines[i];
            let b = &lines[j];
            if a.amount == 0.0 || b.amount == 0.0 {
                continue;
            }
            if a.amount.signum() == b.amount.signum() {
                continue;
            }
            if !a.currency_code.eq_ignore_ascii_case(&b.currency_code) {
                continue;
            }
            if (a.amount.abs() - b.amount.abs()).abs() > 0.01 {
                continue;
            }
            if (a.posted_at - b.posted_at).abs() > 3 * 86_400_000 {
                continue;
            }
            if same_account(a, b) {
                continue;
            }
            if let Some((score, reason)) = score_pair(a, b) {
                candidates.push((i, j, score, reason));
            }
        }
    }
    // Highest confidence first; ties keep the earlier line pair.
    candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let mut used: HashSet<usize> = HashSet::new();
    let now = db::server_time_ms();
    for (i, j, score, reason) in candidates {
        if used.contains(&i) || used.contains(&j) {
            continue;
        }
        used.insert(i);
        used.insert(j);
        let (debit, credit) = if lines[i].amount < 0.0 {
            (&lines[i], &lines[j])
        } else {
            (&lines[j], &lines[i])
        };
        let status = if score >= 0.95 { "auto" } else { "suggested" };
        let id = stable_uuid(TRANSFER_NS, &format!("match:{}:{}", debit.id, credit.id));
        tx.execute(
            "INSERT INTO transfer_matches
                (id, debit_line_id, credit_line_id, transaction_id, confidence,
                 reason, status, created_at, updated_at)
             VALUES (?1,?2,?3,NULL,?4,?5,?6,?7,?7)
             ON CONFLICT(debit_line_id, credit_line_id) DO NOTHING",
            params![id, debit.id, credit.id, score, reason, status, now],
        )?;
    }
    Ok(())
}

fn same_account(a: &PendingLine, b: &PendingLine) -> bool {
    match (&a.account_id, &b.account_id) {
        (Some(a), Some(b)) => a == b,
        _ => match (&a.account_number, &b.account_number) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        },
    }
}

fn score_pair(a: &PendingLine, b: &PendingLine) -> Option<(f64, String)> {
    let mut score: f64 = 0.55;
    let mut reasons = vec!["same amount and opposite signs".to_string()];

    if is_transfer_text(&a.description) && is_transfer_text(&b.description) {
        score += 0.20;
        reasons.push("both look like transfers".to_string());
    }
    if a.source == b.source && a.account_number != b.account_number {
        score += 0.15;
        reasons.push("same institution, different account".to_string());
    }
    if same_day(a.posted_at, b.posted_at) {
        score += 0.10;
        reasons.push("same day".to_string());
    }
    if let Some(reference) = metadata_reference(&b.metadata) {
        if a.description.contains(&reference) {
            score += 0.30;
            reasons.push(format!("reference {reference}"));
        }
    }
    if let Some(reference) = metadata_reference(&a.metadata) {
        if b.description.contains(&reference) {
            score += 0.30;
            reasons.push(format!("reference {reference}"));
        }
    }

    let score = score.min(0.99);
    (score >= 0.70).then(|| (score, reasons.join(", ")))
}

fn is_transfer_text(description: &str) -> bool {
    let lower = description.to_lowercase();
    // "Retrait" is an ATM withdrawal, not a transfer; including it would boost
    // unrelated same-amount pairs.
    ["virement", "transfer", "versement", "sepa"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn metadata_reference(metadata: &serde_json::Value) -> Option<String> {
    metadata
        .get("reference")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

fn same_day(a: i64, b: i64) -> bool {
    date_string(a) == date_string(b)
}

fn date_string(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "0000-00-00".to_string())
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    /// Per-line decisions. Empty means "import every pending line with its
    /// suggestions".
    #[serde(default)]
    pub lines: Vec<ImportLine>,
    #[serde(rename = "accountId", default)]
    pub account_id: Option<String>,
    #[serde(rename = "counterpartAccountId", default)]
    pub counterpart_account_id: Option<String>,
    /// High-confidence pairs are expanded and imported together by default.
    #[serde(rename = "autoLink", default)]
    pub auto_link: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ImportLine {
    pub id: String,
    #[serde(default)]
    pub action: ImportAction,
    #[serde(rename = "type", default)]
    pub type_: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(rename = "accountId", default)]
    pub account_id: Option<String>,
    #[serde(rename = "counterpartAccountId", default)]
    pub counterpart_account_id: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportAction {
    #[default]
    Import,
    Skip,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub statement_id: String,
    pub created: usize,
    pub skipped: usize,
    pub errors: usize,
    pub lines: Vec<ImportLineOutcome>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportLineOutcome {
    pub line_id: String,
    pub status: String,
    pub transaction_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
struct LineRow {
    id: String,
    statement_id: String,
    posted_at: i64,
    description: String,
    amount: f64,
    currency_code: String,
    suggested_type: Option<SuggestedType>,
    suggested_category: Option<String>,
    status: String,
    dedupe_key: String,
}

#[derive(Debug, Clone)]
struct StatementRow {
    id: String,
    source: String,
    account_name: String,
    account_number: Option<String>,
    account_id: Option<String>,
}

const LINE_COLUMNS: &str = "l.id, l.statement_id, l.posted_at, l.description, l.amount,
            l.currency_code, l.suggested_type, l.suggested_category,
            l.status, l.dedupe_key";

fn line_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<LineRow> {
    let suggested_type: Option<String> = r.get(6)?;
    Ok(LineRow {
        id: r.get(0)?,
        statement_id: r.get(1)?,
        posted_at: r.get(2)?,
        description: r.get(3)?,
        amount: r.get(4)?,
        currency_code: r.get(5)?,
        suggested_type: suggested_type.as_deref().and_then(SuggestedType::parse),
        suggested_category: r.get(7)?,
        status: r.get(8)?,
        dedupe_key: r.get(9)?,
    })
}

fn load_statement(tx: &Transaction<'_>, id: &str) -> anyhow::Result<StatementRow> {
    tx.query_row(
        "SELECT id, source, account_name, account_number, account_id
         FROM bank_statements WHERE id=?1 AND deleted_at IS NULL",
        params![id],
        |r| {
            Ok(StatementRow {
                id: r.get(0)?,
                source: r.get(1)?,
                account_name: r.get(2)?,
                account_number: r.get(3)?,
                account_id: r.get(4)?,
            })
        },
    )
    .optional()?
    .with_context(|| format!("statement {id} not found"))
}

fn load_line(tx: &Transaction<'_>, id: &str) -> anyhow::Result<LineRow> {
    tx.query_row(
        &format!(
            "SELECT {LINE_COLUMNS}
             FROM bank_statement_lines l
             WHERE l.id=?1"
        ),
        params![id],
        line_from_row,
    )
    .optional()?
    .with_context(|| format!("statement line {id} not found"))
}

/// Import selected lines into `is_draft = 1` transactions.
///
/// Idempotent by construction: every transaction id is a UUID v5 over the
/// line/pair identity, inserts use `ON CONFLICT DO NOTHING`, and a tombstoned
/// transaction is never resurrected.
pub fn import_statement(
    tx: &Transaction<'_>,
    statement_id: &str,
    request: ImportRequest,
    base_currency: &str,
) -> anyhow::Result<ImportOutcome> {
    let statement = load_statement(tx, statement_id)?;
    let now = db::server_time_ms();

    // Load every line once so auto-pair expansion can reach counterparts.
    let mut stmt = tx.prepare(&format!(
        "SELECT {LINE_COLUMNS}
         FROM bank_statement_lines l
         WHERE l.statement_id=?1"
    ))?;
    let mut all_lines: HashMap<String, LineRow> = HashMap::new();
    let mut rows = stmt.query(params![statement_id])?;
    while let Some(r) = rows.next()? {
        let row = line_from_row(r)?;
        all_lines.insert(row.id.clone(), row);
    }

    // Resolve the requested set. An empty request means every pending line.
    let mut requested: Vec<ImportLine> = if request.lines.is_empty() {
        all_lines
            .values()
            .filter(|line| line.status == "pending")
            .map(|line| ImportLine {
                id: line.id.clone(),
                action: ImportAction::Import,
                type_: None,
                category: None,
                account_id: None,
                counterpart_account_id: None,
            })
            .collect()
    } else {
        request.lines
    };
    requested.sort_by_key(|line| line.id.clone());

    let by_id: HashMap<String, &ImportLine> = requested
        .iter()
        .map(|line| (line.id.clone(), line))
        .collect();

    // Expand high-confidence pairs: if one side is selected, the counterpart is
    // part of the same transfer. Explicit skips opt out.
    let auto_link = request.auto_link.unwrap_or(true);
    if auto_link {
        let mut additions = Vec::new();
        for line in &requested {
            if line.action == ImportAction::Skip {
                continue;
            }
            if let Some((match_id, counterpart_id)) = auto_match(tx, &line.id)? {
                let counterpart_skipped = by_id
                    .get(&counterpart_id)
                    .map(|entry| entry.action == ImportAction::Skip)
                    .unwrap_or(false);
                if counterpart_skipped {
                    tx.execute(
                        "UPDATE transfer_matches SET status='dismissed', updated_at=?1 WHERE id=?2",
                        params![now, match_id],
                    )?;
                    continue;
                }
                if !all_lines.contains_key(&counterpart_id) {
                    all_lines.insert(counterpart_id.clone(), load_line(tx, &counterpart_id)?);
                }
                // Only a still-pending counterpart can be imported with this
                // line. If it was already imported on its own (an earlier
                // import with auto-linking off), pairing now would create a
                // second transaction for money that is already booked and
                // repoint the old line at it. Dismiss the match instead: the
                // transfer did happen, but this import cannot represent it as
                // one row without duplicating a transaction.
                if all_lines
                    .get(&counterpart_id)
                    .is_none_or(|counterpart| counterpart.status != "pending")
                {
                    tx.execute(
                        "UPDATE transfer_matches SET status='dismissed', updated_at=?1 WHERE id=?2",
                        params![now, match_id],
                    )?;
                    continue;
                }
                if !by_id.contains_key(&counterpart_id) {
                    additions.push(ImportLine {
                        id: counterpart_id,
                        action: ImportAction::Import,
                        type_: None,
                        category: None,
                        account_id: None,
                        counterpart_account_id: None,
                    });
                }
            }
        }
        for addition in additions {
            requested.push(addition);
        }
    }

    let mut outcome = ImportOutcome {
        statement_id: statement_id.to_string(),
        created: 0,
        skipped: 0,
        errors: 0,
        lines: Vec::new(),
    };
    let mut handled: HashSet<String> = HashSet::new();

    for request_line in &requested {
        if handled.contains(&request_line.id) {
            continue;
        }
        let Some(line) = all_lines.get(&request_line.id).cloned() else {
            outcome.errors += 1;
            outcome.lines.push(ImportLineOutcome {
                line_id: request_line.id.clone(),
                status: "error".to_string(),
                transaction_id: None,
                error: Some("line not found in statement".to_string()),
            });
            continue;
        };

        if request_line.action == ImportAction::Skip {
            handled.insert(line.id.clone());
            mark_skipped(tx, &line, now)?;
            dismiss_matches_for(tx, &line.id, now)?;
            outcome.skipped += 1;
            outcome.lines.push(ImportLineOutcome {
                line_id: line.id,
                status: "skipped".to_string(),
                transaction_id: None,
                error: None,
            });
            continue;
        }

        match line.status.as_str() {
            "imported" => {
                handled.insert(line.id.clone());
                outcome.skipped += 1;
                let transaction_id = transaction_for_line(tx, &line.id)?;
                outcome.lines.push(ImportLineOutcome {
                    line_id: line.id,
                    status: "imported".to_string(),
                    transaction_id,
                    error: None,
                });
                continue;
            }
            "informational" => {
                handled.insert(line.id.clone());
                outcome.skipped += 1;
                outcome.lines.push(ImportLineOutcome {
                    line_id: line.id,
                    status: "informational".to_string(),
                    transaction_id: None,
                    error: None,
                });
                continue;
            }
            "duplicate" => {
                handled.insert(line.id.clone());
                outcome.skipped += 1;
                outcome.lines.push(ImportLineOutcome {
                    line_id: line.id,
                    status: "duplicate".to_string(),
                    transaction_id: None,
                    error: None,
                });
                continue;
            }
            _ => {}
        }

        // A high-confidence pair is one transfer transaction, not two. With
        // auto-linking off the user asked for this line alone, so the pair is
        // left for a later import of the other side.
        if auto_link {
            if let Some((match_id, counterpart_id)) = auto_match(tx, &line.id)? {
                if !handled.contains(&counterpart_id) {
                    let counterpart = match all_lines.get(&counterpart_id).cloned() {
                        Some(counterpart) => counterpart,
                        None => load_line(tx, &counterpart_id)?,
                    };
                    // Same rule as the expansion above: never pair with a line
                    // that is no longer pending, or an earlier single import
                    // would be duplicated by the transfer row.
                    if counterpart.status != "pending" {
                        tx.execute(
                            "UPDATE transfer_matches SET status='dismissed', updated_at=?1 WHERE id=?2",
                            params![now, match_id],
                        )?;
                    } else {
                        handled.insert(counterpart.id.clone());
                        let result = import_pair(
                            tx,
                            &statement,
                            &line,
                            &counterpart,
                            &match_id,
                            base_currency,
                            now,
                        );
                        match result {
                            Ok(transaction_id) => {
                                outcome.created += 1;
                                outcome.lines.push(ImportLineOutcome {
                                    line_id: line.id.clone(),
                                    status: "imported".to_string(),
                                    transaction_id: Some(transaction_id.clone()),
                                    error: None,
                                });
                                outcome.lines.push(ImportLineOutcome {
                                    line_id: counterpart.id,
                                    status: "imported".to_string(),
                                    transaction_id: Some(transaction_id),
                                    error: None,
                                });
                            }
                            Err(error) => {
                                outcome.errors += 1;
                                mark_error(tx, &line.id, &error, now)?;
                                mark_error(tx, &counterpart.id, &error, now)?;
                                outcome.lines.push(ImportLineOutcome {
                                    line_id: line.id.clone(),
                                    status: "error".to_string(),
                                    transaction_id: None,
                                    error: Some(error.to_string()),
                                });
                            }
                        }
                        continue;
                    }
                }
            }
        }

        handled.insert(line.id.clone());
        let result = import_single(
            tx,
            &statement,
            &line,
            request_line,
            request.account_id.as_deref(),
            request.counterpart_account_id.as_deref(),
            base_currency,
            now,
        );
        match result {
            Ok(transaction_id) => {
                outcome.created += 1;
                outcome.lines.push(ImportLineOutcome {
                    line_id: line.id,
                    status: "imported".to_string(),
                    transaction_id: Some(transaction_id),
                    error: None,
                });
            }
            Err(error) => {
                outcome.errors += 1;
                mark_error(tx, &line.id, &error, now)?;
                outcome.lines.push(ImportLineOutcome {
                    line_id: line.id,
                    status: "error".to_string(),
                    transaction_id: None,
                    error: Some(error.to_string()),
                });
            }
        }
    }

    refresh_statement_status(tx, statement_id, now)?;
    Ok(outcome)
}

fn auto_match(tx: &Transaction<'_>, line_id: &str) -> anyhow::Result<Option<(String, String)>> {
    tx.query_row(
        "SELECT id, debit_line_id, credit_line_id FROM transfer_matches
         WHERE status='auto' AND (debit_line_id=?1 OR credit_line_id=?1)
         LIMIT 1",
        params![line_id],
        |r| {
            let id: String = r.get(0)?;
            let debit: String = r.get(1)?;
            let credit: String = r.get(2)?;
            Ok((id, if debit == line_id { credit } else { debit }))
        },
    )
    .optional()
    .map_err(Into::into)
}

fn transaction_for_line(tx: &Transaction<'_>, line_id: &str) -> anyhow::Result<Option<String>> {
    tx.query_row(
        "SELECT transaction_id FROM bank_statement_lines WHERE id=?1",
        params![line_id],
        |r| r.get(0),
    )
    .optional()
    .map(|v| v.flatten())
    .map_err(Into::into)
}

fn mark_skipped(tx: &Transaction<'_>, line: &LineRow, now: i64) -> anyhow::Result<()> {
    tx.execute(
        "UPDATE bank_statement_lines
         SET status='skipped', import_error=NULL, updated_at=?1 WHERE id=?2",
        params![now, line.id],
    )?;
    Ok(())
}

fn mark_error(
    tx: &Transaction<'_>,
    line_id: &str,
    error: &anyhow::Error,
    now: i64,
) -> anyhow::Result<()> {
    tx.execute(
        "UPDATE bank_statement_lines
         SET status='error', import_error=?1, updated_at=?2 WHERE id=?3",
        params![error.to_string(), now, line_id],
    )?;
    Ok(())
}

fn dismiss_matches_for(tx: &Transaction<'_>, line_id: &str, now: i64) -> anyhow::Result<()> {
    tx.execute(
        "UPDATE transfer_matches SET status='dismissed', updated_at=?1
         WHERE (debit_line_id=?2 OR credit_line_id=?2) AND status IN ('auto','suggested')",
        params![now, line_id],
    )?;
    Ok(())
}

fn import_single(
    tx: &Transaction<'_>,
    statement: &StatementRow,
    line: &LineRow,
    request: &ImportLine,
    statement_account_override: Option<&str>,
    statement_counterpart_override: Option<&str>,
    base_currency: &str,
    now: i64,
) -> anyhow::Result<String> {
    let account_override = request.account_id.as_deref().or(statement_account_override);
    let account_id = resolve_account(tx, statement, account_override, now)?
        .with_context(|| format!("no account for {}", statement.account_name))?;

    let type_ = request
        .type_
        .as_deref()
        .and_then(SuggestedType::parse)
        .or(line.suggested_type)
        .unwrap_or_else(|| {
            if line.amount < 0.0 {
                SuggestedType::Expense
            } else {
                SuggestedType::Income
            }
        });

    let category = request
        .category
        .clone()
        .or_else(|| line.suggested_category.clone());

    if type_ == SuggestedType::Transfer {
        let counterpart_override = request
            .counterpart_account_id
            .as_deref()
            .or(statement_counterpart_override);
        let counterpart_id =
            resolve_counterpart_account(tx, statement, &account_id, counterpart_override, now)?;
        let id = stable_uuid(TRANSACTION_NS, &format!("line:{}", line.dedupe_key));
        let rate = rate_for(tx, &line.currency_code, base_currency, line.posted_at)?;
        create_transaction(
            tx,
            &id,
            "transfer",
            line.amount.abs(),
            &line.currency_code,
            rate,
            &account_id,
            Some(&counterpart_id),
            category.as_deref(),
            line.posted_at,
            &line.description,
            now,
        )?;
        mark_imported(tx, &line.id, &id, now)?;
        return Ok(id);
    }

    let id = stable_uuid(TRANSACTION_NS, &format!("line:{}", line.dedupe_key));
    let rate = rate_for(tx, &line.currency_code, base_currency, line.posted_at)?;
    create_transaction(
        tx,
        &id,
        type_.as_str(),
        line.amount.abs(),
        &line.currency_code,
        rate,
        &account_id,
        None,
        category.as_deref(),
        line.posted_at,
        &line.description,
        now,
    )?;
    mark_imported(tx, &line.id, &id, now)?;
    Ok(id)
}

fn import_pair(
    tx: &Transaction<'_>,
    statement: &StatementRow,
    a: &LineRow,
    b: &LineRow,
    match_id: &str,
    base_currency: &str,
    now: i64,
) -> anyhow::Result<String> {
    let (debit, credit) = if a.amount < 0.0 { (a, b) } else { (b, a) };
    if !debit
        .currency_code
        .eq_ignore_ascii_case(&credit.currency_code)
    {
        bail!("cross-currency transfers must be imported one side at a time");
    }
    let debit_statement = if debit.statement_id == statement.id {
        statement.clone()
    } else {
        load_statement(tx, &debit.statement_id)?
    };
    let credit_statement = if credit.statement_id == statement.id {
        statement.clone()
    } else {
        load_statement(tx, &credit.statement_id)?
    };
    let debit_account = resolve_account(tx, &debit_statement, None, now)?
        .with_context(|| format!("no account for {}", debit_statement.account_name))?;
    let credit_account = resolve_account(tx, &credit_statement, None, now)?
        .with_context(|| format!("no account for {}", credit_statement.account_name))?;

    let (first, second) = if debit.dedupe_key <= credit.dedupe_key {
        (&debit.dedupe_key, &credit.dedupe_key)
    } else {
        (&credit.dedupe_key, &debit.dedupe_key)
    };
    let id = stable_uuid(TRANSACTION_NS, &format!("transfer:{first}:{second}"));
    let rate = rate_for(tx, &debit.currency_code, base_currency, debit.posted_at)?;
    let note = format!("{} / {}", debit.description, credit.description);
    create_transaction(
        tx,
        &id,
        "transfer",
        debit.amount.abs(),
        &debit.currency_code,
        rate,
        &debit_account,
        Some(&credit_account),
        None,
        debit.posted_at,
        &note,
        now,
    )?;
    mark_imported(tx, &debit.id, &id, now)?;
    mark_imported(tx, &credit.id, &id, now)?;
    tx.execute(
        "UPDATE transfer_matches SET status='linked', transaction_id=?1, updated_at=?2
         WHERE id=?3",
        params![id, now, match_id],
    )?;
    Ok(id)
}

fn mark_imported(
    tx: &Transaction<'_>,
    line_id: &str,
    transaction_id: &str,
    now: i64,
) -> anyhow::Result<()> {
    tx.execute(
        "UPDATE bank_statement_lines
         SET status='imported', transaction_id=?1, import_error=NULL, updated_at=?2
         WHERE id=?3",
        params![transaction_id, now, line_id],
    )?;
    Ok(())
}

/// Create a draft transaction. `ON CONFLICT DO NOTHING` makes re-import a
/// no-op; a tombstoned row is reported rather than revived.
#[allow(clippy::too_many_arguments)]
fn create_transaction(
    tx: &Transaction<'_>,
    id: &str,
    type_: &str,
    amount: f64,
    currency_code: &str,
    rate: f64,
    account_id: &str,
    to_account_id: Option<&str>,
    category: Option<&str>,
    date: i64,
    note: &str,
    now: i64,
) -> anyhow::Result<()> {
    let amount_base = amount * rate;
    tx.execute(
        "INSERT INTO transactions
            (id, type, amount, currency_code, amount_base, rate_used, account_id,
             to_account_id, category, date, note, receipt_id, is_draft,
             created_at, updated_at, deleted_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,NULL,1,?12,?12,NULL)
         ON CONFLICT(id) DO NOTHING",
        params![
            id,
            type_,
            amount,
            currency_code,
            amount_base,
            rate,
            account_id,
            to_account_id,
            category,
            date,
            note,
            now,
        ],
    )?;
    let deleted: Option<i64> = tx.query_row(
        "SELECT deleted_at FROM transactions WHERE id=?1",
        params![id],
        |r| r.get(0),
    )?;
    if deleted.is_some() {
        bail!("transaction {id} was deleted; not resurrecting it");
    }
    Ok(())
}

fn rate_for(tx: &Transaction<'_>, code: &str, base: &str, date_ms: i64) -> anyhow::Result<f64> {
    if code.eq_ignore_ascii_case(base) {
        return Ok(1.0);
    }
    let date = date_string(date_ms);
    tx.query_row(
        "SELECT rate_to_base FROM fx_rates
         WHERE code=?1 AND base_code=?2 AND rate_date<=?3 AND deleted_at IS NULL
         ORDER BY rate_date DESC LIMIT 1",
        params![code, base, date],
        |r| r.get(0),
    )
    .optional()?
    .with_context(|| format!("no {code}/{base} FX rate on or before {date}"))
}

/// Resolve the budget account for a statement, creating it on first import.
///
/// The account id is deterministic so a re-import finds the same row. A
/// tombstoned account is an error, not something to revive.
fn resolve_account(
    tx: &Transaction<'_>,
    statement: &StatementRow,
    override_id: Option<&str>,
    now: i64,
) -> anyhow::Result<Option<String>> {
    if let Some(id) = override_id {
        ensure_account_live(tx, id)?;
        return Ok(Some(id.to_string()));
    }
    if let Some(id) = statement.account_id.as_deref() {
        if account_exists(tx, id)? {
            ensure_account_live(tx, id)?;
            return Ok(Some(id.to_string()));
        }
    }
    let Some(external_id) = statement.account_number.as_deref() else {
        return Ok(None);
    };
    if let Some(id) = lookup_external_account(tx, &statement.source, external_id)? {
        ensure_account_live(tx, &id)?;
        return Ok(Some(id));
    }

    let id = stable_uuid(
        ACCOUNT_NS,
        &format!("account:{}:{}", statement.source, external_id),
    );
    let type_ = default_account_type(&statement.source, &statement.account_name);
    tx.execute(
        "INSERT INTO accounts
            (id, name, type, opening_balance, note, created_at, updated_at,
             deleted_at, source, external_id)
         VALUES (?1,?2,?3,0.0,NULL,?4,?4,NULL,?5,?6)
         ON CONFLICT(id) DO NOTHING",
        params![
            id,
            statement.account_name,
            type_,
            now,
            statement.source,
            external_id,
        ],
    )?;
    ensure_account_live(tx, &id)?;
    tx.execute(
        "UPDATE bank_statements SET account_id=?1 WHERE id=?2",
        params![id, statement.id],
    )?;
    Ok(Some(id))
}

fn resolve_counterpart_account(
    tx: &Transaction<'_>,
    statement: &StatementRow,
    source_account: &str,
    override_id: Option<&str>,
    now: i64,
) -> anyhow::Result<String> {
    if let Some(id) = override_id {
        ensure_account_live(tx, id)?;
        if id == source_account {
            bail!("transfer source and destination are the same account");
        }
        return Ok(id.to_string());
    }
    // With no explicit counterpart, use a stable "external" account so the
    // transaction still balances instead of crediting nowhere.
    let id = stable_uuid(
        ACCOUNT_NS,
        &format!("account:{}:external", statement.source),
    );
    tx.execute(
        "INSERT INTO accounts
            (id, name, type, opening_balance, note, created_at, updated_at,
             deleted_at, source, external_id)
         VALUES (?1,?2,'other',0.0,NULL,?3,?3,NULL,?4,'external')
         ON CONFLICT(id) DO NOTHING",
        params![
            id,
            format!("{} (external)", statement.account_name),
            now,
            statement.source,
        ],
    )?;
    ensure_account_live(tx, &id)?;
    if id == source_account {
        bail!("transfer source and destination are the same account");
    }
    Ok(id)
}

fn account_exists(tx: &Transaction<'_>, id: &str) -> anyhow::Result<bool> {
    let exists: Option<String> = tx
        .query_row("SELECT id FROM accounts WHERE id=?1", params![id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(exists.is_some())
}

fn ensure_account_live(tx: &Transaction<'_>, id: &str) -> anyhow::Result<()> {
    let deleted: Option<i64> = tx
        .query_row(
            "SELECT deleted_at FROM accounts WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if deleted.is_some() {
        bail!("account {id} was deleted; restore it or map the statement to another account");
    }
    Ok(())
}

fn lookup_external_account(
    tx: &Transaction<'_>,
    source: &str,
    external_id: &str,
) -> anyhow::Result<Option<String>> {
    tx.query_row(
        "SELECT id FROM accounts WHERE source=?1 AND external_id=?2",
        params![source, external_id],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn default_account_type(source: &str, name: &str) -> &'static str {
    match source {
        "credit_agricole" => {
            let lower = name.to_lowercase();
            if ["ldd", "livret", "epargne", "épargne"]
                .iter()
                .any(|needle| lower.contains(needle))
            {
                "savings"
            } else {
                "bank"
            }
        }
        "trading212" | "firstrade" => "investment",
        _ => "other",
    }
}
