//! Firstrade CSV parser.
//!
//! Firstrade has no public trading API, so this is a file source. The export
//! layout has changed over the years, so the parser finds columns by header
//! name and falls back to the common `Date,Action,Symbol,Description,Quantity,
//! Price,Amount` order. Trades are informational; cash movements become
//! importable lines with the same mapping as Trading 212.

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

        let mut by_month: BTreeMap<String, Vec<StatementLineDraft>> = BTreeMap::new();
        let mut occurrences: BTreeMap<(String, String, String, String), usize> = BTreeMap::new();
        let mut warnings = Vec::new();

        for row in rows {
            let Some(posted_at) = row.posted_at else {
                warnings.push(format!("row without a date: {}", row.raw.join(",")));
                continue;
            };
            let month = month_key(posted_at);
            let (line_kind, suggested_type) = classify(&row.action, row.amount);
            let amount = match line_kind {
                LineKind::Trade => 0.0,
                LineKind::TransferOut | LineKind::Fee => -row.amount.abs(),
                LineKind::TransferIn | LineKind::Dividend | LineKind::Interest => row.amount.abs(),
                _ => row.amount,
            };
            let key = (
                month.clone(),
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
                &month,
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
            by_month.entry(month).or_default().push(StatementLineDraft {
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

        Ok(by_month
            .into_iter()
            .map(|(month, lines)| {
                let (period_start, period_end) = month_bounds(&month);
                FirstradeStatement {
                    month,
                    account_number: self.account_number.clone(),
                    period_start,
                    period_end,
                    source_hash: ctx.source_hash.clone(),
                    stored_path: ctx.stored_path.clone(),
                    lines,
                    warnings: warnings.clone(),
                }
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct FirstradeStatement {
    pub month: String,
    pub account_number: String,
    pub period_start: String,
    pub period_end: String,
    pub source_hash: String,
    pub stored_path: Option<String>,
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
            period_start: Some(self.period_start.clone()),
            period_end: Some(self.period_end.clone()),
            opening_balance: None,
            closing_balance: None,
            source_path: None,
            stored_path: self.stored_path,
            source_hash: self.source_hash,
            source_key: format!(
                "{}:{}:{}",
                SourceId::Firstrade.as_str(),
                self.account_number,
                self.month
            ),
            parsed: serde_json::json!({
                "month": self.month,
                "lines": self.lines.len(),
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
        let action = find("action").or_else(|| find("type"));
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

fn month_key(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m").to_string())
        .unwrap_or_else(|| "1970-01".to_string())
}

fn month_bounds(month: &str) -> (String, String) {
    let mut parts = month.split('-');
    let year = parts
        .next()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(1970);
    let month_number = parts
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    let first = NaiveDate::from_ymd_opt(year, month_number, 1)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap());
    let next = first
        .checked_add_months(chrono::Months::new(1))
        .unwrap_or(first);
    let last = next.pred_opt().unwrap_or(first);
    (
        first.format("%Y-%m-%d").to_string(),
        last.format("%Y-%m-%d").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ParseContext {
        ParseContext {
            source_hash: "hash".to_string(),
            stored_path: None,
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
}
