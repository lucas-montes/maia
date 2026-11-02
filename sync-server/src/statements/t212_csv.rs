//! Trading 212 monthly CSV export.
//!
//! The web UI offers a statement CSV per month, and this module parses the
//! generic export that covers a date range. It is not the API feed: the
//! columns, actions and precision differ, so it is a separate parser that
//! feeds the same staging pipeline. Each month of activity becomes its own
//! statement with a distinct `source_key` (`trading212:csv:<month>`), so a CSV
//! import and a live feed sync never clobber each other's interpretations.
//!
//! The CSV `ID` column is a stable UUID, which becomes the line's
//! `source_ref`: re-importing the same month stages nothing new, and a
//! different month never collides.
//!
//! Card debits and credits are the money lines; `Spending cashback` is income;
//! `Deposit` is a transfer in, deliberately `TransferIn` so the CA↔T212
//! matching in the store can link it to a bank outgoing. `Market buy` /
//! `Market sell` are informational trades: shown for review, never turned into
//! transactions, with amount 0 because the CSV's Total is the wallet impact
//! including fees the user does not want twice-counted.

use std::{collections::BTreeMap, future::Future, path::PathBuf};

use super::{
    DocumentKind, ParseContext, Parser, RawDocument, SourceError, SourceId, ToDatabaseRecord,
    draft::{LineKind, StatementDraft, StatementLineDraft, SuggestedType},
    sha256_hex,
    t212::{PROVIDER, line_fingerprint, month_bounds, month_key},
};

pub struct Trading212CsvSource {
    path: PathBuf,
}

impl Trading212CsvSource {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Parser for Trading212CsvSource {
    type Output = Vec<T212CsvStatement>;

    fn source(&self) -> SourceId {
        SourceId::Trading212
    }

    fn acquire(&self) -> impl Future<Output = Result<Vec<RawDocument>, SourceError>> + Send {
        let path = self.path.clone();
        async move {
            let bytes = std::fs::read(&path).map_err(|error| SourceError::Io {
                path: path.display().to_string(),
                source: error,
            })?;
            Ok(vec![RawDocument::file(DocumentKind::Csv, bytes, path)])
        }
    }

    fn parse(&self, docs: &[RawDocument], ctx: &ParseContext) -> Result<Self::Output, SourceError> {
        let Some(doc) = docs.first() else {
            return Err(SourceError::malformed("no CSV document given"));
        };
        let text = csv_text(doc);
        let rows = parse_csv(&text)?;
        let statements = group_by_month(rows, ctx);
        if statements.is_empty() {
            return Err(SourceError::malformed(
                "Trading 212 CSV contains no statement lines",
            ));
        }
        Ok(statements)
    }
}

/// CSV exports may be UTF-8 or UTF-16 (Excel export); decode both.
fn csv_text(doc: &RawDocument) -> String {
    let big_endian = doc.bytes.starts_with(&[0xFE, 0xFF]);
    if big_endian || doc.bytes.starts_with(&[0xFF, 0xFE]) {
        let encoding = if big_endian {
            encoding_rs::UTF_16BE
        } else {
            encoding_rs::UTF_16LE
        };
        let (text, _, _) = encoding.decode(&doc.bytes);
        text.into_owned()
    } else {
        String::from_utf8_lossy(&doc.bytes).into_owned()
    }
}

/// A single CSV row, trimmed.
struct Row {
    action: String,
    time: Option<String>,
    isin: String,
    ticker: String,
    name: String,
    notes: String,
    id: String,
    shares: String,
    price: String,
    exchange_rate: String,
    total: String,
    total_currency: String,
    fee: String,
    fee_currency: String,
    merchant: String,
    merchant_category: String,
}

impl Row {
    fn amount(&self) -> f64 {
        self.total.parse::<f64>().unwrap_or(0.0)
    }

    fn currency(&self) -> String {
        if self.total_currency.is_empty() {
            "EUR".to_string()
        } else {
            self.total_currency.clone()
        }
    }
}

/// Header names accepted for each column. Matching is exact (case kept), the
/// same way Trading 212 writes them; anything else is a rejected file.
struct Header {
    index: BTreeMap<&'static str, usize>,
}

fn detect_header(fields: &[String]) -> Option<Header> {
    let mut index = BTreeMap::new();
    for (position, field) in fields.iter().enumerate() {
        let seen = field.trim();
        let key = match seen {
            "Action" => "action",
            "Time (UTC)" => "time",
            "ISIN" => "isin",
            "Ticker" => "ticker",
            "Name" => "name",
            "Notes" => "notes",
            "ID" => "id",
            "No. of shares" => "shares",
            "Price / share" => "price",
            "Exchange rate" => "exchange_rate",
            "Total" => "total",
            "Currency (Total)" => "total_currency",
            "Currency conversion fee" => "fee",
            "Currency (Currency conversion fee)" => "fee_currency",
            "Merchant name" => "merchant",
            "Merchant category" => "merchant_category",
            _ => continue,
        };
        // Two columns with the same header would be ambiguous.
        if index.contains_key(key) {
            return None;
        }
        index.insert(key, position);
    }
    if !index.contains_key("action") || !index.contains_key("total") {
        return None;
    }
    Some(Header { index })
}

impl Header {
    fn get(&self, fields: &[String], key: &str) -> String {
        self.index
            .get(key)
            .and_then(|position| fields.get(*position))
            .map(|value| value.trim().to_string())
            .unwrap_or_default()
    }
}

fn parse_csv(text: &str) -> Result<Vec<Row>, SourceError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());

    let mut rows = Vec::new();
    let mut header: Option<Header> = None;
    let mut warned = false;

    for record in reader.records() {
        let fields: Vec<String> = record
            .map_err(|error| SourceError::malformed(format!("Trading 212 CSV: {error}")))?
            .iter()
            .map(|field| field.trim().to_string())
            .collect();
        if fields.iter().all(|field| field.is_empty()) {
            continue;
        }
        if header.is_none() {
            match detect_header(&fields) {
                Some(found) => header = Some(found),
                None => {
                    let message = match fields.first() {
                        Some(first) => format!(
                            "not a Trading 212 export: first row starts with {first:?},\
                             expected an Action/Time (UTC)/Total header"
                        ),
                        None => "empty CSV".to_string(),
                    };
                    return Err(SourceError::malformed(message));
                }
            }
            continue;
        }
        let header = header.as_ref().expect("header detected above");
        let action = header.get(&fields, "action");
        if action.is_empty() {
            continue;
        }
        let row = Row {
            action,
            time: {
                let value = header.get(&fields, "time");
                if value.is_empty() { None } else { Some(value) }
            },
            isin: header.get(&fields, "isin"),
            ticker: header.get(&fields, "ticker"),
            name: header.get(&fields, "name"),
            notes: header.get(&fields, "notes"),
            id: header.get(&fields, "id"),
            shares: header.get(&fields, "shares"),
            price: header.get(&fields, "price"),
            exchange_rate: header.get(&fields, "exchange_rate"),
            total: header.get(&fields, "total"),
            total_currency: header.get(&fields, "total_currency"),
            fee: header.get(&fields, "fee"),
            fee_currency: header.get(&fields, "fee_currency"),
            merchant: header.get(&fields, "merchant"),
            merchant_category: header.get(&fields, "merchant_category"),
        };
        if row.total.is_empty() && !warned {
            warned = true;
            tracing::warn!("Trading 212 CSV rows with an empty Total are staged at 0.00");
        }
        rows.push(row);
    }
    if header.is_none() {
        return Err(SourceError::malformed(
            "Trading 212 CSV has no header row (expected Action, Time (UTC), Total)",
        ));
    }
    if rows.is_empty() {
        return Err(SourceError::malformed(
            "Trading 212 CSV contains no statement rows",
        ));
    }
    Ok(rows)
}

/// Exported times are `2026-02-03 02:07:26+00:00`. Shorter or offset-less
/// variants are treated as UTC when a plain stamp appears.
fn posted_at(raw: &str) -> Option<i64> {
    if let Ok(when) = chrono::DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%:z") {
        return Some(when.timestamp_millis());
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Some(naive.and_utc().timestamp_millis());
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis());
    }
    None
}

/// Which kind of line the action names.
fn classify(action: &str, amount: f64) -> (LineKind, Option<SuggestedType>) {
    match action {
        "Card debit" => (LineKind::Debit, Some(SuggestedType::Expense)),
        "Card credit" => (LineKind::Credit, Some(SuggestedType::Income)),
        "Spending cashback" => (LineKind::Credit, Some(SuggestedType::Income)),
        "Deposit" => (LineKind::TransferIn, Some(SuggestedType::Transfer)),
        "Withdraw" => (LineKind::TransferOut, Some(SuggestedType::Transfer)),
        "Market buy" | "Market sell" => (LineKind::Trade, None),
        "Dividend" => (LineKind::Dividend, Some(SuggestedType::Income)),
        other => {
            tracing::warn!(action = other, "unknown Trading 212 CSV action");
            let _ = amount;
            (LineKind::Other, None)
        }
    }
}

fn description(row: &Row) -> String {
    match row.action.as_str() {
        "Spending cashback" => "Spending cashback".to_string(),
        "Deposit" if !row.notes.is_empty() => format!("Deposit ({})", row.notes),
        "Deposit" => "Deposit".to_string(),
        "Market buy" | "Market sell" => {
            let side = if row.action == "Market buy" {
                "Buy"
            } else {
                "Sell"
            };
            match (row.shares.parse::<f64>().ok(), row.ticker.as_str()) {
                (Some(quantity), ticker) if quantity > 0.0 && !ticker.is_empty() => {
                    format!("{side} {quantity:.6} {ticker}")
                        .trim_end()
                        .to_string()
                }
                _ if !row.ticker.is_empty() => format!("{side} {}", row.ticker),
                _ => row.action.clone(),
            }
        }
        _ if !row.merchant.is_empty() => row.merchant.clone(),
        _ => row.action.clone(),
    }
}

fn category_hint(kind: LineKind) -> Option<String> {
    match kind {
        LineKind::TransferIn | LineKind::TransferOut | LineKind::Transfer => {
            Some("Transfer".to_string())
        }
        LineKind::Dividend => Some("Dividends".to_string()),
        LineKind::Fee => Some("Fees".to_string()),
        LineKind::Interest => Some("Interest".to_string()),
        _ => None,
    }
}

fn group_by_month(rows: Vec<Row>, ctx: &ParseContext) -> Vec<T212CsvStatement> {
    let mut by_month: BTreeMap<String, Vec<StatementLineDraft>> = BTreeMap::new();
    let mut warnings = Vec::new();
    let account_number = "csv".to_string();

    for row in rows {
        let Some(when_raw) = row.time.as_deref() else {
            warnings.push(format!("row without a date: {}", row.action));
            continue;
        };
        let Some(posted_at) = posted_at(when_raw) else {
            warnings.push(format!("row with an unreadable date {when_raw:?}"));
            continue;
        };
        let amount = if row.action == "Market buy" || row.action == "Market sell" {
            0.0
        } else {
            row.amount()
        };
        let (line_kind, suggested_type) = classify(&row.action, amount);
        let source_ref = if row.id.is_empty() {
            None
        } else {
            Some(row.id.clone())
        };
        // Distinct type token so a CSV line can never collide with a feed line
        // staging the same event by fingerprint.
        let type_token = format!("CSV:{}", row.action);
        let fingerprint = line_fingerprint(
            PROVIDER,
            &account_number,
            source_ref.as_deref(),
            &type_token,
            posted_at,
            amount,
            None,
        );
        let dedupe_key = sha256_hex(&[PROVIDER, &account_number, &fingerprint]);
        let description = description(&row);
        let hint = category_hint(line_kind);
        let metadata = serde_json::json!({
            "id": source_ref,
            "action": row.action,
            "merchant": if row.merchant.is_empty() { None } else { Some(row.merchant.clone()) },
            "merchant_category": if row.merchant_category.is_empty() { None } else { Some(row.merchant_category.clone()) },
            "ticker": row.ticker,
            "isin": row.isin,
            "instrument": row.name,
            "shares": row.shares,
            "price": row.price,
            "exchange_rate": row.exchange_rate,
            "fee": row.fee,
            "fee_currency": if row.fee_currency.is_empty() { None } else { Some(row.fee_currency.clone()) },
            "notes": row.notes,
            "raw_total": row.total,
        });
        by_month
            .entry(month_key(posted_at))
            .or_default()
            .push(StatementLineDraft {
                source_ref,
                posted_at,
                description,
                amount,
                currency_code: row.currency(),
                balance_after: None,
                category_hint: hint.clone(),
                line_kind,
                suggested_type,
                suggested_category: hint,
                metadata,
                fingerprint,
                dedupe_key,
            });
    }

    let statements = by_month
        .into_iter()
        .map(|(month, lines)| {
            let (period_start, period_end) = month_bounds(&month);
            T212CsvStatement {
                month,
                account_name: "Trading 212".to_string(),
                account_number: account_number.clone(),
                currency_code: "EUR".to_string(),
                period_start,
                period_end,
                source_hash: ctx.source_hash.clone(),
                stored_path: ctx.stored_path.clone(),
                lines,
                warnings: warnings.clone(),
            }
        })
        .collect();
    statements
}

#[derive(Debug, Clone)]
pub struct T212CsvStatement {
    pub month: String,
    pub account_name: String,
    pub account_number: String,
    pub currency_code: String,
    pub period_start: String,
    pub period_end: String,
    pub source_hash: String,
    pub stored_path: Option<String>,
    pub lines: Vec<StatementLineDraft>,
    pub warnings: Vec<String>,
}

impl ToDatabaseRecord for Vec<T212CsvStatement> {
    fn into_drafts(self) -> Vec<StatementDraft> {
        self.into_iter().map(T212CsvStatement::into_draft).collect()
    }
}

impl T212CsvStatement {
    fn into_draft(self) -> StatementDraft {
        StatementDraft {
            source: SourceId::Trading212,
            institution: SourceId::Trading212.institution().to_string(),
            account_name: self.account_name,
            account_number: Some(self.account_number.clone()),
            currency_code: self.currency_code.clone(),
            period_start: Some(self.period_start.clone()),
            period_end: Some(self.period_end.clone()),
            opening_balance: None,
            closing_balance: None,
            source_path: None,
            stored_path: self.stored_path,
            source_hash: self.source_hash,
            // CSV statements key differently from the feed: same source
            // fields, but no shared account id, and a month-scoped key so an
            // import can be retried without colliding with a feed sync.
            source_key: format!("{PROVIDER}:csv:{}:{}", self.account_number, self.month),
            parsed: serde_json::json!({
                "month": self.month,
                "lines": self.lines.len(),
                "import": "csv",
            }),
            warnings: self.warnings,
            lines: self.lines,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ParseContext {
        ParseContext {
            source_hash: "hash".to_string(),
            stored_path: Some("/stored/export.csv".to_string()),
            fetched_at: 0,
        }
    }

    fn parse(text: &str) -> Vec<T212CsvStatement> {
        let doc = RawDocument::file(
            DocumentKind::Csv,
            text.as_bytes().to_vec(),
            PathBuf::from("export.csv"),
        );
        let source = Trading212CsvSource::new(PathBuf::from("export.csv"));
        source.parse(&[doc], &ctx()).unwrap()
    }

    /// The header exactly as the export writes it.
    const HEADER: &str = "Action,Time (UTC),ISIN,Ticker,Name,Notes,ID,No. of shares,Price / share,Currency (Price / share),Exchange rate,Total,Currency (Total),Currency conversion fee,Currency (Currency conversion fee),Merchant name,Merchant category";

    #[test]
    fn maps_card_cash_and_trade_rows() {
        let csv = format!(
            "{HEADER}\n\
             Card debit,2026-02-03 22:46:16+00:00,,,,,019c2833-2eb2-796f-9b78-ee44f3859184,,,,,-92.89,\"EUR\",,,\"Amazon Pay\",\"OTHER\"\n\
             Spending cashback,2026-02-04 02:07:49+00:00,,,,,019c2667-e6c7-7d13-ab73-af21d3ea473c,,,,,0.57,\"EUR\",,,,\n\
             Deposit,2026-02-06 15:45:29+00:00,,,,\"Bank Transfer\",019ea2c2-b3bd-71b1-a986-b17f03464eed,,,,,1000.00,\"EUR\",,,,\n\
             Market buy,2026-02-06 08:04:51+00:00,IE00BK5BQT80,VWCE,\"Vanguard FTSE All-World (Acc)\",,EOF46165624235,0.0093977200,145.7800000000,EUR,1.00000000,1.37,\"EUR\",,,,\n"
        );
        let statements = parse(&csv);
        assert_eq!(statements.len(), 1, "one month, one statement");
        let lines = &statements[0].lines;
        assert_eq!(lines.len(), 4);

        let debit = &lines[0];
        assert_eq!(debit.amount, -92.89);
        assert_eq!(debit.line_kind, LineKind::Debit);
        assert_eq!(debit.description, "Amazon Pay");
        assert_eq!(debit.suggested_type, Some(SuggestedType::Expense));
        assert_eq!(
            debit.metadata.get("merchant_category"),
            Some(&serde_json::json!("OTHER"))
        );

        let cashback = &lines[1];
        assert_eq!(cashback.amount, 0.57);
        assert_eq!(cashback.line_kind, LineKind::Credit);

        let deposit = &lines[2];
        assert_eq!(deposit.amount, 1000.00);
        assert_eq!(deposit.line_kind, LineKind::TransferIn);
        assert_eq!(deposit.suggested_type, Some(SuggestedType::Transfer));

        let trade = &lines[3];
        assert_eq!(trade.amount, 0.0, "a trade line is informational only");
        assert_eq!(trade.line_kind, LineKind::Trade);
        assert_eq!(trade.description, "Buy 0.009398 VWCE");
    }

    #[test]
    fn months_split_with_a_stable_csv_key() {
        let csv = format!(
            "{HEADER}\n\
             Card debit,2026-02-03 22:46:16+00:00,,,,,id-a,,,,,-1.00,\"EUR\",,,,\n\
             Card debit,2026-03-03 22:46:16+00:00,,,,,id-b,,,,,-2.00,\"EUR\",,,,\n"
        );
        let statements = parse(&csv);
        assert_eq!(statements.len(), 2);
        let mut months: Vec<std::string::String> =
            statements.iter().map(|s| s.month.clone()).collect();
        months.sort();
        assert_eq!(months, ["2026-02", "2026-03"]);
        for statement in &statements {
            assert_eq!(
                statement.account_number,
                "csv".to_string(),
                "every CSV month maps to the single CSV account"
            );
            let draft = Vec::from([statement.clone()]).into_drafts();
            assert!(draft[0].source_key.starts_with("trading212:csv:"));
        }
    }

    #[test]
    fn fingerprints_are_deterministic_and_reference_keyed() {
        let csv = format!(
            "{HEADER}\n\
             Card debit,2026-02-03 22:46:16+00:00,,,,,same-id,,,,,-1.00,\"EUR\",,,,\n"
        );
        let statement0 = parse(&csv);
        let statement1 = parse(&csv);
        let line0 = &statement0[0].lines[0];
        let line1 = &statement1[0].lines[0];
        assert_eq!(line0.dedupe_key, line1.dedupe_key);
        assert_eq!(line0.source_ref.as_deref(), Some("same-id"));
    }

    #[test]
    fn unknown_action_survives_as_other() {
        let csv = format!(
            "{HEADER}\n\
             Mystery action,2026-02-03 22:46:16+00:00,,,,,mystery,,,,,-3.00,\"EUR\",,,,\n"
        );
        let statements = parse(&csv);
        assert_eq!(statements[0].lines[0].line_kind, LineKind::Other);
        assert_eq!(statements[0].lines[0].amount, -3.00);
    }

    #[test]
    fn a_non_t212_csv_is_rejected() {
        let doc = RawDocument::file(
            DocumentKind::Csv,
            b"Date,Amount\n2026-02-03,-1.00\n".to_vec(),
            PathBuf::from("other.csv"),
        );
        let source = Trading212CsvSource::new(PathBuf::from("other.csv"));
        let error = source.parse(&[doc], &ctx()).unwrap_err();
        assert!(error.to_string().contains("not a Trading 212 export"));
    }

    #[test]
    fn a_header_only_csv_is_rejected() {
        let doc = RawDocument::file(
            DocumentKind::Csv,
            HEADER.as_bytes().to_vec(),
            PathBuf::from("export.csv"),
        );
        let source = Trading212CsvSource::new(PathBuf::from("export.csv"));
        let error = source.parse(&[doc], &ctx()).unwrap_err();
        assert!(error.to_string().contains("no statement rows"));
    }

    /// A real export should keep parsing; the fixture is optional so the test
    /// still runs on machines that do not mount the old disk.
    #[test]
    fn parses_the_real_export_fixture() {
        let fixture = std::env::var("MAIA_T212_FIXTURE").unwrap_or_else(|_| {
            "/mnt/old/home/lucas/Documents/expenses/from_2026-02-03_to_2026-09-04_MTc4ODUxODI2NjA5NQ.csv"
                .to_string()
        });
        let Ok(bytes) = std::fs::read(&fixture) else {
            eprintln!("fixture {fixture} not present; skipping");
            return;
        };
        let doc = RawDocument::file(DocumentKind::Csv, bytes, PathBuf::from(&fixture));
        let source = Trading212CsvSource::new(PathBuf::from(&fixture));
        let statements = source.parse(&[doc], &ctx()).unwrap();
        let total_lines: usize = statements.iter().map(|s| s.lines.len()).sum();
        assert_eq!(total_lines, 379, "every data row stages");
        let months: Vec<&str> = statements.iter().map(|s| s.month.as_str()).collect();
        assert!(months.contains(&"2026-02"), "months: {months:?}");
        assert!(months.contains(&"2026-09"), "months: {months:?}");
        // The full month span is present; no warnings for a well-formed file.
        assert!(
            statements.iter().all(|s| s.warnings.is_empty()),
            "unexpected warnings"
        );
    }
}
