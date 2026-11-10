//! Firstrade CSV parser.
//!
//! Firstrade has no public trading API, so this is a file source. The export
//! layout has changed over the years, so the parser finds columns by header
//! name and falls back to the common `Date,Action,Symbol,Description,Quantity,
//! Price,Amount` order. Trades are informational; cash movements become
//! importable lines with the same mapping as Trading 212.
//!
//! **One file is one statement.** An export used to be split into one
//! statement per month, which the review screen reads as one account each --
//! a single file offered to create 34 accounts when the right answer was one.
//! The file is the unit instead, keyed `firstrade:<account>:<source_hash>` the
//! way a Crédit Agricole section already was, and the period is the span of
//! the rows actually in the file. Overlapping exports are resolved per line by
//! `dedupe_key`, not by replacing a month's bucket.

use std::{collections::BTreeMap, future::Future, path::PathBuf};

use chrono::NaiveDate;

use super::{
    DocumentKind, ParseContext, Parser, RawDocument, SourceError, SourceId, ToDatabaseRecord,
    draft::{LineKind, StatementDraft, StatementLineDraft, SuggestedType},
    sha256_hex,
};

pub struct FirstradeSource {
    path: PathBuf,
    account_number: String,
}

impl FirstradeSource {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            account_number: "firstrade".to_string(),
        }
    }
}

impl Parser for FirstradeSource {
    type Output = Vec<FirstradeStatement>;

    fn source(&self) -> SourceId {
        SourceId::Firstrade
    }

    fn acquire(&self) -> impl Future<Output = Result<Vec<RawDocument>, SourceError>> + Send {
        let path = self.path.clone();
        async move {
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|source| SourceError::Io {
                    path: path.display().to_string(),
                    source,
                })?;
            Ok(vec![RawDocument::file(DocumentKind::Csv, bytes, path)])
        }
    }

    fn parse(&self, docs: &[RawDocument], ctx: &ParseContext) -> Result<Self::Output, SourceError> {
        let doc = docs
            .first()
            .ok_or_else(|| SourceError::malformed("no Firstrade document"))?;
        if doc.kind != DocumentKind::Csv {
            return Err(SourceError::Unsupported(
                "Firstrade import supports CSV exports".to_string(),
            ));
        }
        let (text, _, _) = encoding_rs::UTF_8.decode(&doc.bytes);
        let rows = parse_csv(&text)?;

        let mut lines: Vec<StatementLineDraft> = Vec::new();
        let mut occurrences: BTreeMap<(String, String, String, String), usize> = BTreeMap::new();
        let mut warnings = Vec::new();

        for row in rows {
            let Some(posted_at) = row.posted_at else {
                warnings.push(format!("row without a date: {}", row.raw.join(",")));
                continue;
            };
            // The row's own day disambiguates repeats, where the month used to:
            // now that a whole file is one statement, two identical rows a
            // month apart must still fingerprint differently, and a day is the
            // finest grain the export actually carries.
            let day = day_key(posted_at);
            let (line_kind, suggested_type) = classify(&row.action, row.amount);
            let amount = match line_kind {
                LineKind::Trade => 0.0,
                LineKind::TransferOut | LineKind::Fee => -row.amount.abs(),
                LineKind::TransferIn | LineKind::Dividend | LineKind::Interest => row.amount.abs(),
                _ => row.amount,
            };
            let key = (
                day.clone(),
                row.action.clone(),
                row.symbol.clone(),
                format!("{:.4}:{}", amount, row.description),
            );
            let occurrence = occurrences.entry(key).or_insert(0);
            let index = *occurrence;
            *occurrence += 1;

            let fingerprint = sha256_hex(&[
                SourceId::Firstrade.as_str(),
                &self.account_number,
                &day,
                &row.action,
                &row.symbol,
                &format!("{amount:.4}"),
                &row.description,
                &index.to_string(),
            ]);
            let dedupe_key = sha256_hex(&[
                SourceId::Firstrade.as_str(),
                &self.account_number,
                &fingerprint,
            ]);
            let category = suggested_category(line_kind);
            lines.push(StatementLineDraft {
                source_ref: row.reference.clone(),
                posted_at,
                description: if row.description.is_empty() {
                    format!("{} {}", row.action, row.symbol).trim().to_string()
                } else {
                    row.description.clone()
                },
                amount,
                currency_code: "USD".to_string(),
                balance_after: None,
                category_hint: category.clone(),
                line_kind,
                suggested_type,
                suggested_category: category,
                metadata: serde_json::json!({
                    "action": row.action,
                    "symbol": row.symbol,
                    "quantity": row.quantity,
                    "price": row.price,
                    "reference": row.reference,
                    "raw": row.raw,
                }),
                fingerprint,
                dedupe_key,
            });
        }

        let (period_start, period_end) = span(&lines);
        Ok(vec![FirstradeStatement {
            account_number: self.account_number.clone(),
            period_start,
            period_end,
            source_hash: ctx.source_hash.clone(),
            stored_path: ctx.stored_path.clone(),
            source_path: ctx.source_path.clone(),
            lines,
            warnings,
        }])
    }
}

/// The real span of a file: the first and last day it actually posts, rather
/// than the calendar bounds of a month it happens to touch.
fn span(lines: &[StatementLineDraft]) -> (Option<String>, Option<String>) {
    let first = lines.iter().map(|line| line.posted_at).min();
    let last = lines.iter().map(|line| line.posted_at).max();
    (first.map(day_key), last.map(day_key))
}

#[derive(Debug, Clone)]
pub struct FirstradeStatement {
    pub account_number: String,
    /// The span of the rows in the file, `None` for a file whose every row was
    /// unreadable (its warnings are still worth showing).
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub source_hash: String,
    pub stored_path: Option<String>,
    /// The dropped file's own path, carried through so the list endpoint can
    /// group a file's statements (see [`ParseContext::source_path`]).
    pub source_path: Option<String>,
    pub lines: Vec<StatementLineDraft>,
    pub warnings: Vec<String>,
}

impl ToDatabaseRecord for Vec<FirstradeStatement> {
    fn into_drafts(self) -> Vec<StatementDraft> {
        self.into_iter()
            .map(FirstradeStatement::into_draft)
            .collect()
    }
}

impl FirstradeStatement {
    fn into_draft(self) -> StatementDraft {
        StatementDraft {
            source: SourceId::Firstrade,
            institution: SourceId::Firstrade.institution().to_string(),
            account_name: "Firstrade".to_string(),
            account_number: Some(self.account_number.clone()),
            currency_code: "USD".to_string(),
            period_start: self.period_start.clone(),
            period_end: self.period_end.clone(),
            opening_balance: None,
            closing_balance: None,
            source_path: self.source_path,
            stored_path: self.stored_path,
            // The file is the unit, so the key is the bytes: re-dropping the
            // same export is a no-op, and a second export that overlaps it
            // stages beside it with its repeats marked `duplicate` rather
            // than replacing a shared month bucket.
            source_key: format!(
                "{}:{}:{}",
                SourceId::Firstrade.as_str(),
                self.account_number,
                self.source_hash
            ),
            source_hash: self.source_hash,
            parsed: serde_json::json!({
                "lines": self.lines.len(),
                "periodStart": self.period_start,
                "periodEnd": self.period_end,
            }),
            warnings: self.warnings,
            lines: self.lines,
        }
    }
}

#[derive(Debug, Clone)]
struct FirstradeRow {
    posted_at: Option<i64>,
    action: String,
    symbol: String,
    description: String,
    quantity: Option<f64>,
    price: Option<f64>,
    amount: f64,
    reference: Option<String>,
    raw: Vec<String>,
}

fn parse_csv(text: &str) -> Result<Vec<FirstradeRow>, SourceError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());

    let mut rows = Vec::new();
    let mut columns: Option<Columns> = None;

    for record in reader.records() {
        let record =
            record.map_err(|error| SourceError::malformed(format!("Firstrade CSV: {error}")))?;
        let fields: Vec<String> = record.iter().map(|s| s.trim().to_string()).collect();
        if fields.iter().all(|field| field.is_empty()) {
            continue;
        }
        if columns.is_none() {
            if let Some(found) = Columns::detect(&fields) {
                columns = Some(found);
            }
            continue;
        }
        let columns = columns.as_ref().expect("columns detected above");
        let Some(date) = columns.get(&fields, "date") else {
            continue;
        };
        let Some(posted_at) = parse_datetime(&date) else {
            continue;
        };
        let action = columns
            .get(&fields, "action")
            .or_else(|| columns.get(&fields, "type"))
            .unwrap_or_default();
        let symbol = columns.get(&fields, "symbol").unwrap_or_default();
        let description = columns.get(&fields, "description").unwrap_or_default();
        let quantity = columns
            .get(&fields, "quantity")
            .and_then(|value| parse_number(&value));
        let price = columns
            .get(&fields, "price")
            .and_then(|value| parse_number(&value));
        let amount = columns
            .get(&fields, "amount")
            .or_else(|| columns.get(&fields, "net amount"))
            .and_then(|value| parse_number(&value))
            .unwrap_or(0.0);
        let reference = columns
            .get(&fields, "reference")
            .or_else(|| columns.get(&fields, "confirmation"))
            .filter(|value| !value.is_empty());

        rows.push(FirstradeRow {
            posted_at: Some(posted_at),
            action,
            symbol,
            description,
            quantity,
            price,
            amount,
            reference,
            raw: fields,
        });
    }

    if rows.is_empty() {
        return Err(SourceError::malformed(
            "no Firstrade rows found; is this the right export?",
        ));
    }
    Ok(rows)
}

#[derive(Debug, Clone)]
struct Columns {
    date: usize,
    action: Option<usize>,
    type_: Option<usize>,
    symbol: Option<usize>,
    description: Option<usize>,
    quantity: Option<usize>,
    price: Option<usize>,
    amount: Option<usize>,
    net_amount: Option<usize>,
    reference: Option<usize>,
    confirmation: Option<usize>,
}

impl Columns {
    fn detect(fields: &[String]) -> Option<Self> {
        let normalized: Vec<String> = fields
            .iter()
            .map(|field| field.trim().to_ascii_lowercase())
            .collect();
        let find = |needle: &str| normalized.iter().position(|field| field == needle);
        let date = find("date")?;
        // Require at least an amount or an action column; a random text line
        // should not be mistaken for the header.
        let amount = find("amount").or_else(|| find("net amount"));
        // The real export heads its column `Transaction`, the API-shaped
        // export `Action`; both are the line's kind.
        let action = find("action")
            .or_else(|| find("transaction"))
            .or_else(|| find("type"));
        if amount.is_none() && action.is_none() {
            return None;
        }
        Some(Self {
            date,
            action,
            type_: find("type"),
            symbol: find("symbol"),
            description: find("description"),
            quantity: find("quantity"),
            price: find("price"),
            amount,
            net_amount: find("net amount"),
            reference: find("reference"),
            confirmation: find("confirmation"),
        })
    }

    fn get(&self, fields: &[String], column: &str) -> Option<String> {
        let index = match column {
            "date" => self.date,
            "action" => self.action?,
            "type" => self.type_?,
            "symbol" => self.symbol?,
            "description" => self.description?,
            "quantity" => self.quantity?,
            "price" => self.price?,
            "amount" => self.amount?,
            "net amount" => self.net_amount?,
            "reference" => self.reference?,
            "confirmation" => self.confirmation?,
            _ => return None,
        };
        fields
            .get(index)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}

fn parse_datetime(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    for format in ["%m/%d/%Y", "%Y-%m-%d", "%m/%d/%y"] {
        if let Ok(date) = NaiveDate::parse_from_str(raw, format) {
            return date
                .and_hms_opt(0, 0, 0)
                .map(|dt| dt.and_utc().timestamp_millis());
        }
    }
    for format in ["%m/%d/%Y %H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(raw, format) {
            return Some(dt.and_utc().timestamp_millis());
        }
    }
    None
}

fn parse_number(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    let negative = raw.starts_with('(') && raw.ends_with(')');
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '$' && *c != '(' && *c != ')')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let value = cleaned.parse::<f64>().ok()?;
    Some(if negative { -value.abs() } else { value })
}

fn classify(action: &str, amount: f64) -> (LineKind, Option<SuggestedType>) {
    let lower = action.to_lowercase();
    if ["buy", "sell", "bought", "sold", "reinvest"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return (LineKind::Trade, None);
    }
    if lower.contains("dividend") || lower.contains("div ") {
        return (LineKind::Dividend, Some(SuggestedType::Income));
    }
    if lower.contains("interest") {
        return (LineKind::Interest, Some(SuggestedType::Income));
    }
    if lower.contains("fee") || lower.contains("commission") {
        return (LineKind::Fee, Some(SuggestedType::Expense));
    }
    if ["deposit", "wire in", "transfer in", "ach in"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return (LineKind::TransferIn, Some(SuggestedType::Transfer));
    }
    if ["withdrawal", "wire out", "transfer out", "ach out"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return (LineKind::TransferOut, Some(SuggestedType::Transfer));
    }
    if amount < 0.0 {
        (LineKind::Debit, Some(SuggestedType::Expense))
    } else {
        (LineKind::Credit, Some(SuggestedType::Income))
    }
}

fn suggested_category(kind: LineKind) -> Option<String> {
    match kind {
        LineKind::Dividend => Some("Dividends".to_string()),
        LineKind::Interest => Some("Interest".to_string()),
        LineKind::Fee => Some("Fees".to_string()),
        LineKind::Transfer | LineKind::TransferIn | LineKind::TransferOut => {
            Some("Transfer".to_string())
        }
        _ => None,
    }
}

fn day_key(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ParseContext {
        ParseContext {
            source_hash: "hash".to_string(),
            stored_path: None,
            source_path: None,
            fetched_at: 0,
        }
    }

    #[test]
    fn parses_a_typical_export() {
        let csv = "Date,Action,Symbol,Description,Quantity,Price,Amount\n\
                   10/16/2026,DEPOSIT,,ACH deposit,,,1000.00\n\
                   10/17/2026,BUY,VWCE,Bought 2 shares,2,100.00,-200.00\n\
                   10/18/2026,DIVIDEND,VWCE,Cash dividend,,,12.34\n";
        let doc = RawDocument::file(
            DocumentKind::Csv,
            csv.as_bytes().to_vec(),
            PathBuf::from("x.csv"),
        );
        let statements = FirstradeSource::new(PathBuf::from("x.csv"))
            .parse(&[doc], &ctx())
            .unwrap();
        assert_eq!(statements.len(), 1);
        let lines = &statements[0].lines;
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].line_kind, LineKind::TransferIn);
        assert_eq!(lines[0].amount, 1000.0);
        assert_eq!(lines[1].line_kind, LineKind::Trade);
        assert_eq!(lines[1].amount, 0.0);
        assert_eq!(lines[2].line_kind, LineKind::Dividend);
        assert_eq!(lines[2].amount, 12.34);
    }

    #[test]
    fn parses_parenthesised_negative_amounts() {
        assert_eq!(parse_number("(1,234.56)"), Some(-1234.56));
        assert_eq!(parse_number("$1,000.00"), Some(1000.0));
    }

    /// A small synthetic export committed in the repository, using the real
    /// Firstrade column set (`Transaction` rather than the API-style `Action`
    /// the inline sample uses).
    #[test]
    fn parses_the_committed_fixture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/statements/firstrade.csv"
        );
        let bytes = std::fs::read(path)
            .unwrap_or_else(|error| panic!("the fixture {path} should be committed: {error}"));
        let doc = RawDocument::file(DocumentKind::Csv, bytes, PathBuf::from(&path));
        let statements = FirstradeSource::new(PathBuf::from(&path))
            .parse(&[doc], &ctx())
            .unwrap();

        let total_lines: usize = statements.iter().map(|s| s.lines.len()).sum();
        assert_eq!(total_lines, 10, "every data row stages");
        assert_eq!(statements.len(), 1, "one file is one statement");
        assert!(
            statements.iter().all(|s| s.warnings.is_empty()),
            "unexpected warnings: {:?}",
            statements
                .iter()
                .flat_map(|s| &s.warnings)
                .collect::<Vec<_>>()
        );

        let buy = statements
            .iter()
            .flat_map(|s| &s.lines)
            .find(|l| l.line_kind == LineKind::Trade)
            .unwrap();
        assert_eq!(buy.amount, 0.0, "a firstrade trade is informational only");
        assert_eq!(buy.metadata.get("symbol"), Some(&serde_json::json!("TECV")));

        let deposit = statements
            .iter()
            .flat_map(|s| &s.lines)
            .find(|l| l.description.contains("ACH DEPOSIT"))
            .unwrap();
        assert_eq!(deposit.line_kind, LineKind::TransferIn);
        assert_eq!(deposit.amount, 500.0);

        // Every dividend of the file stages, in one statement. Asserted as a
        // set rather than "the first one": with the months collapsed, line
        // order is the file's order and no longer carries meaning.
        let mut dividends: Vec<f64> = statements
            .iter()
            .flat_map(|s| &s.lines)
            .filter(|l| l.line_kind == LineKind::Dividend)
            .map(|l| l.amount)
            .collect();
        dividends.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(dividends, [12.60, 28.05, 29.50]);
        assert!(
            statements
                .iter()
                .flat_map(|s| &s.lines)
                .filter(|l| l.line_kind == LineKind::Dividend)
                .all(|l| l.suggested_type == Some(SuggestedType::Income))
        );

        let interest = statements
            .iter()
            .flat_map(|s| &s.lines)
            .find(|l| l.line_kind == LineKind::Interest)
            .unwrap();
        assert_eq!(interest.currency_code, "USD");

        // The period is the file's real span, not a month's calendar bounds.
        assert_eq!(statements[0].period_start.as_deref(), Some("2026-06-15"));
        assert_eq!(statements[0].period_end.as_deref(), Some("2026-10-15"));

        let draft = statements.clone().into_drafts();
        assert_eq!(draft.len(), 1);
        assert_eq!(draft[0].source_key, "firstrade:firstrade:hash");
    }

    /// Two rows that differ only by month used to be told apart by the month in
    /// their fingerprint. Now that the whole file is one statement, the day has
    /// to do that job -- otherwise the second row would be dropped by the
    /// `UNIQUE(statement_id, fingerprint)` insert.
    #[test]
    fn identical_rows_in_different_months_keep_distinct_fingerprints() {
        let csv = "Date,Action,Symbol,Description,Quantity,Price,Amount\n\
                   06/15/2026,DEPOSIT,,ACH deposit,,,100.00\n\
                   07/15/2026,DEPOSIT,,ACH deposit,,,100.00\n\
                   07/15/2026,DEPOSIT,,ACH deposit,,,100.00\n";
        let doc = RawDocument::file(
            DocumentKind::Csv,
            csv.as_bytes().to_vec(),
            PathBuf::from("x.csv"),
        );
        let statements = FirstradeSource::new(PathBuf::from("x.csv"))
            .parse(&[doc], &ctx())
            .unwrap();
        assert_eq!(statements.len(), 1);
        let prints: std::collections::HashSet<&str> = statements[0]
            .lines
            .iter()
            .map(|line| line.fingerprint.as_str())
            .collect();
        assert_eq!(prints.len(), 3, "three rows, three identities");
    }

    /// An undated row never reaches a statement, and the dated ones around it
    /// still do.
    #[test]
    fn a_row_without_a_readable_date_is_skipped() {
        let csv = "Date,Action,Symbol,Description,Quantity,Price,Amount\n\
                   06/15/2026,DEPOSIT,,ACH deposit,,,100.00\n\
                   not-a-date,DEPOSIT,,ACH deposit,,,100.00\n";
        let doc = RawDocument::file(
            DocumentKind::Csv,
            csv.as_bytes().to_vec(),
            PathBuf::from("x.csv"),
        );
        let statements = FirstradeSource::new(PathBuf::from("x.csv"))
            .parse(&[doc], &ctx())
            .unwrap();
        let warnings: usize = statements.iter().map(|s| s.warnings.len()).sum();
        assert_eq!(warnings, 0, "an undated row is dropped by the CSV reader");
        assert_eq!(statements[0].lines.len(), 1);
    }
}
