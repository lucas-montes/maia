//! Firstrade (Apex Clearing) PDF statements: text first, then the model.
//!
//! Firstrade's CSV export only reaches back so far. Older history exists only
//! as the monthly or quarterly PDF statements Apex Clearing issues. Reading
//! one takes four steps:
//!
//! 1. **PDF → text** with poppler's `pdftotext -layout`. This keeps the
//!    activity table's columns aligned, which is the only thing that tells a
//!    DEBIT from a CREDIT, and the `WH` withholding apart from either.
//! 2. **The header is read here, in Rust.** The period line
//!    (`March 1, 2023 - March 31, 2023`) and the `Cash account <opening>
//!    <closing>` line sit at fixed places, so the model is never trusted with
//!    them.
//! 3. **The activity is read by the model.** Only the pages carrying the
//!    holdings and activity tables are sent. The disclosures and brochures that
//!    make up most of each PDF are dropped, along with the account-number
//!    lines.
//! 4. **The reading is checked, not trusted.** Opening cash plus every row's
//!    net movement has to land on the closing cash. A mismatch becomes a
//!    statement warning the review screen shows.
//!
//! [`Parser::parse`](super::Parser::parse) is pure, so all of this runs before
//! it (see `statements::run_file_source` / `run_upload`). The reading is cached
//! beside the statement store under `<hash>.extract.json`. Re-dropping a file
//! then neither pays for the model again nor risks a different reading of the
//! same bytes, and the line fingerprints are built from that reading.

use std::path::PathBuf;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

use super::{DocumentKind, RawDocument, SourceError, SourceId};
use crate::server::AppState;

/// Everything the parser needs from one PDF statement.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PdfExtract {
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub opening_cash: Option<f64>,
    pub closing_cash: Option<f64>,
    #[serde(default)]
    pub transactions: Vec<PdfTransaction>,
}

/// One activity row as the model read it. Amounts are positive magnitudes in
/// the column they were printed under.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PdfTransaction {
    /// `YYYY-MM-DD`.
    pub date: String,
    #[serde(default)]
    pub section: Option<String>,
    /// The TRANSACTION column verbatim: `BOUGHT`, `SOLD`, `DIVIDEND`,
    /// `INTEREST`, `WIRE`, `JOURNAL`, `REC`, …
    pub action: String,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub quantity: Option<f64>,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub debit: Option<f64>,
    #[serde(default)]
    pub credit: Option<f64>,
    #[serde(default)]
    pub withheld: Option<f64>,
    #[serde(default)]
    pub cusip: Option<String>,
}

impl PdfTransaction {
    /// Net cash movement: credit, less debit, less tax withheld.
    pub fn net(&self) -> f64 {
        let magnitude = |value: Option<f64>| value.map(f64::abs).unwrap_or(0.0);
        magnitude(self.credit) - magnitude(self.debit) - magnitude(self.withheld)
    }

    /// True when the row moves no cash at all: a security received, delivered
    /// or journaled between account types.
    pub fn moves_no_cash(&self) -> bool {
        [self.debit, self.credit, self.withheld]
            .iter()
            .all(|value| value.map(|v| v.abs() < 0.005).unwrap_or(true))
    }
}

#[derive(Deserialize)]
struct ModelReading {
    #[serde(default)]
    transactions: Vec<PdfTransaction>,
}

/// Reads the first PDF in `docs`, or returns `None` when there is none (a CSV
/// export needs nothing from here).
pub async fn extract(state: &AppState, docs: &[RawDocument]) -> Result<Option<PdfExtract>, SourceError> {
    let Some(doc) = docs.iter().find(|doc| doc.kind == DocumentKind::Pdf) else {
        return Ok(None);
    };
    let hash = super::hash_documents(docs);
    let cache = cache_path(state, &hash);
    if let Some(cached) = read_cache(&cache) {
        tracing::info!(path = %cache.display(), "firstrade pdf: reusing cached reading");
        return Ok(Some(cached));
    }

    let text = pdf_to_text(state.config.pdftotext_bin(), &doc.bytes).await?;
    let mut reading = read_header(&text);
    if reading.period_start.is_none() {
        return Err(SourceError::malformed(
            "this PDF has no Firstrade statement period; is it an account statement?",
        ));
    }

    let activity = activity_text(&text);
    if !activity.is_empty() {
        let model = state.ocr.as_ref().ok_or_else(|| {
            SourceError::Config("Firstrade PDF import needs a model key (`model_api` in the deployment config)".to_string())
        })?;
        let prompt = format!(
            "Statement period: {} to {}.\n\n{activity}",
            reading.period_start.as_deref().unwrap_or("?"),
            reading.period_end.as_deref().unwrap_or("?"),
        );
        let value = model
            .generate_json(SYSTEM.to_string(), prompt, schema())
            .await
            .map_err(|error| SourceError::malformed(format!("model call failed: {error:?}")))?
            .ok_or_else(|| SourceError::malformed("the model returned no reading of the statement"))?;
        let parsed: ModelReading = serde_json::from_value(value)
            .map_err(|error| SourceError::malformed(format!("the model's reading did not fit the schema: {error}")))?;
        reading.transactions = parsed.transactions;
    }

    write_cache(&cache, &reading);
    Ok(Some(reading))
}

fn cache_path(state: &AppState, hash: &str) -> PathBuf {
    state
        .config
        .statements_store_dir()
        .join(SourceId::Firstrade.as_str())
        .join(format!("{hash}.extract.json"))
}

fn read_cache(path: &std::path::Path) -> Option<PdfExtract> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Best effort: a failed write costs one more model call next time, nothing
/// more.
fn write_cache(path: &std::path::Path, reading: &PdfExtract) {
    let Some(dir) = path.parent() else { return };
    if let Err(error) = std::fs::create_dir_all(dir) {
        tracing::warn!(path = %dir.display(), %error, "firstrade pdf: cannot create cache dir");
        return;
    }
    super::restrict(dir, 0o700);
    match serde_json::to_vec_pretty(reading).map(|bytes| std::fs::write(path, bytes)) {
        Ok(Ok(())) => super::restrict(path, 0o600),
        Ok(Err(error)) => tracing::warn!(path = %path.display(), %error, "firstrade pdf: cannot write cache"),
        Err(error) => tracing::warn!(%error, "firstrade pdf: cannot serialise reading"),
    }
}

/// Runs `pdftotext -layout - -`, with the PDF on stdin and the text on stdout.
pub async fn pdf_to_text(bin: &str, bytes: &[u8]) -> Result<String, SourceError> {
    let mut child = tokio::process::Command::new(bin)
        .args(["-layout", "-enc", "UTF-8", "-", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                SourceError::Config(format!(
                    "`{bin}` was not found; install poppler-utils or set pdftotext_bin"
                ))
            } else {
                SourceError::Io { path: bin.to_string(), source: error }
            }
        })?;
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let input = bytes.to_vec();
    // Written from its own task: a large PDF can fill the pipe before
    // pdftotext starts writing, and waiting on both from one task would
    // deadlock.
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(&input).await;
        drop(stdin);
    });
    let output = child
        .wait_with_output()
        .await
        .map_err(|source| SourceError::Io { path: bin.to_string(), source })?;
    let _ = writer.await;
    if !output.status.success() {
        return Err(SourceError::malformed(format!(
            "pdftotext failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The left margin prints `STATEMENT ACCOUNT` vertically, one letter per
/// line, so many lines start with a lone capital and a run of spaces. That
/// letter is noise to both this parser and the model.
fn strip_margin(line: &str) -> &str {
    let mut chars = line.char_indices();
    match (chars.next(), chars.next(), chars.next()) {
        (Some((_, c)), Some((_, ' ')), Some((_, ' '))) if c.is_ascii_uppercase() => line[1..].trim_start(),
        (Some((_, c)), None, None) if c.is_ascii_uppercase() => "",
        _ => line.trim_start(),
    }
}

fn parse_long_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%B %d, %Y").ok()
}

/// `$2,350.47`, `-9.65`, `(1.00)` → a number.
fn parse_money(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    let negative = raw.starts_with('-') || (raw.starts_with('(') && raw.ends_with(')'));
    let cleaned: String = raw.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    if cleaned.is_empty() || !cleaned.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let value: f64 = cleaned.parse().ok()?;
    Some(if negative { -value } else { value })
}

/// Period and cash balances from the summary page.
pub fn read_header(text: &str) -> PdfExtract {
    let mut out = PdfExtract::default();
    for line in text.lines() {
        let line = strip_margin(line);
        if out.period_start.is_none() {
            if let Some((start, end)) = line.split_once(" - ") {
                if let (Some(start), Some(end)) = (parse_long_date(start), parse_long_date(end)) {
                    out.period_start = Some(start.to_string());
                    out.period_end = Some(end.to_string());
                    continue;
                }
            }
        }
        if out.opening_cash.is_none() {
            if let Some(rest) = line.strip_prefix("Cash account") {
                let amounts: Vec<f64> = rest.split_whitespace().filter_map(parse_money).collect();
                if amounts.len() >= 2 {
                    out.opening_cash = Some(amounts[0]);
                    out.closing_cash = Some(amounts[1]);
                }
            }
        }
    }
    out
}

/// The holdings and activity tables, ready for the model.
///
/// A page (pdftotext separates them with a form feed) is kept when it carries
/// the holdings header (`SYMBOL/`) or the activity header (`TRANSACTION
/// DATE`). Anything from `IMPORTANT INFORMATION` onwards is dropped, and so are
/// the lines that repeat the account number.
pub fn activity_text(text: &str) -> String {
    let mut kept = Vec::new();
    for page in text.split('\x0c') {
        let has_activity = page
            .lines()
            .any(|line| {
                let words: Vec<&str> = line.split_whitespace().collect();
                words.windows(2).any(|pair| pair == ["TRANSACTION", "DATE"])
            });
        if !has_activity {
            continue;
        }
        let mut lines = Vec::new();
        for line in page.lines() {
            let line = strip_margin(line);
            if line.starts_with("IMPORTANT INFORMATION") {
                break;
            }
            if line.is_empty() || line.contains("ACCOUNT NUMBER") || line.starts_with("PAGE ") {
                continue;
            }
            lines.push(line.trim_end());
        }
        kept.push(lines.join("\n"));
    }
    kept.join("\n\n")
}

/// Checks that the reading accounts for the cash, and that every row falls
/// inside the period.
pub fn reconcile(reading: &PdfExtract) -> Vec<String> {
    let mut warnings = Vec::new();
    if let (Some(opening), Some(closing)) = (reading.opening_cash, reading.closing_cash) {
        let moved: f64 = reading.transactions.iter().map(PdfTransaction::net).sum();
        let off = opening + moved - closing;
        if off.abs() > 0.005 {
            warnings.push(format!(
                "PDF totals do not reconcile: opening ${opening:.2} + activity ${moved:.2} is off from closing ${closing:.2} by ${off:.2}; check the lines against the statement"
            ));
        }
    }
    let day = |s: &Option<String>| s.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    if let (Some(start), Some(end)) = (day(&reading.period_start), day(&reading.period_end)) {
        for row in &reading.transactions {
            if let Some(date) = NaiveDate::parse_from_str(&row.date, "%Y-%m-%d").ok() {
                if date < start || date > end {
                    warnings.push(format!(
                        "{} {} on {} is outside the statement period {start} – {end}",
                        row.action, row.description, row.date
                    ));
                }
            }
        }
    }
    warnings
}

const SYSTEM: &str = "You read the account activity of a US brokerage statement (Firstrade, cleared by Apex Clearing). \
The text was extracted from a PDF with its column layout preserved. \
Return one entry per transaction row in the ACCOUNT ACTIVITY tables, in the order printed. \
A row starts with a TRANSACTION word (BOUGHT, SOLD, DIVIDEND, INTEREST, WIRE, JOURNAL, REC, DEL, …) and a MM/DD/YY date. \
The indented lines under it continue its description: join them, with single spaces, into `description`, but put the CUSIP into `cusip` instead. \
Skip the 'Total …' rows, section titles and column headers. \
Copy every number exactly as printed, as a positive number without $ or commas. \
Put an amount in `debit` or `credit` according to the column it is aligned under. An amount marked WH is tax withheld: put it in `withheld`, not in debit. \
Leave out amounts that are not printed; never compute or guess one. \
`date` is the row's date as YYYY-MM-DD (the years are 20YY). \
`symbol` is the ticker: take it from the holdings table when the same security is listed there, otherwise give the US ticker only if you are certain of it, otherwise null. \
`section` is the table the row is under.";

/// Gemini `responseSchema` dialect, hand-written for `nullable` and `enum` as
/// in `receipt_ocr`.
pub fn schema() -> Value {
    let number = |description: &str| json!({ "type": "number", "nullable": true, "description": description });
    let text = |description: &str| json!({ "type": "string", "description": description });
    let nullable_text = |description: &str| json!({ "type": "string", "nullable": true, "description": description });
    json!({
        "type": "object",
        "properties": {
            "transactions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "date": text("YYYY-MM-DD"),
                        "section": {
                            "type": "string",
                            "nullable": true,
                            "enum": ["buy_sell", "dividends_interest", "funds", "securities", "misc"],
                        },
                        "action": text("the TRANSACTION column verbatim"),
                        "symbol": nullable_text("ticker"),
                        "description": text("description lines joined"),
                        "quantity": number("QUANTITY column, signed as printed"),
                        "price": number("PRICE column"),
                        "debit": number("DEBIT column"),
                        "credit": number("CREDIT column"),
                        "withheld": number("amount marked WH"),
                        "cusip": nullable_text("CUSIP"),
                    },
                    "required": ["date", "action", "description"],
                },
            },
        },
        "required": ["transactions"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A statement page as `pdftotext -layout` prints it, with the personal
    /// details replaced.
    fn fixture() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/statements/firstrade-2023-03.txt"
        ))
        .expect("fixture")
    }

    #[test]
    fn reads_the_period_and_cash_from_the_summary() {
        let header = read_header(&fixture());
        assert_eq!(header.period_start.as_deref(), Some("2023-03-01"));
        assert_eq!(header.period_end.as_deref(), Some("2023-03-31"));
        assert_eq!(header.opening_cash, Some(726.86));
        assert_eq!(header.closing_cash, Some(2350.47));
    }

    #[test]
    fn keeps_only_the_tables_and_drops_the_account_number() {
        let text = activity_text(&fixture());
        assert!(text.contains("SOLD"), "{text}");
        assert!(text.contains("INTEREST ON CREDIT BALANCE"), "{text}");
        assert!(text.contains("VANGUARD S&P 500 ETF"), "{text}");
        assert!(!text.contains("ACCOUNT NUMBER"), "{text}");
        assert!(!text.contains("Statement of Financial Condition"), "{text}");
        assert!(!text.contains("PAGE 2 OF"), "{text}");
        // The vertical margin letters are gone.
        assert!(!text.lines().any(|line| line.len() == 1), "{text}");
    }

    #[test]
    fn a_statement_with_no_activity_has_no_activity_text() {
        assert!(activity_text("March 1, 2023 - March 31, 2023\n Cash account $1.00 $1.00\n").is_empty());
    }

    fn row(action: &str, debit: Option<f64>, credit: Option<f64>, withheld: Option<f64>) -> PdfTransaction {
        PdfTransaction {
            date: "2023-03-16".into(),
            action: action.into(),
            debit,
            credit,
            withheld,
            ..Default::default()
        }
    }

    /// The real March 2023 activity: it lands exactly on the closing cash.
    fn march() -> PdfExtract {
        PdfExtract {
            period_start: Some("2023-03-01".into()),
            period_end: Some("2023-03-31".into()),
            opening_cash: Some(726.86),
            closing_cash: Some(2350.47),
            transactions: vec![
                row("SOLD", None, Some(1568.70), None),
                row("DIVIDEND", None, Some(58.40), Some(8.76)),
                row("INTEREST", None, Some(0.21), None),
                row("DIVIDEND", None, Some(5.95), Some(0.89)),
            ],
        }
    }

    #[test]
    fn a_complete_reading_reconciles() {
        assert!(reconcile(&march()).is_empty(), "{:?}", reconcile(&march()));
    }

    #[test]
    fn a_missed_row_is_reported() {
        let mut reading = march();
        reading.transactions.remove(2);
        let warnings = reconcile(&reading);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("by $-0.21"), "{}", warnings[0]);
    }

    #[test]
    fn a_row_outside_the_period_is_reported() {
        let mut reading = march();
        reading.transactions[2].date = "2023-04-16".into();
        assert!(reconcile(&reading).iter().any(|w| w.contains("outside the statement period")));
    }

    #[test]
    fn withholding_is_netted_and_security_moves_carry_no_cash() {
        assert!((row("DIVIDEND", None, Some(58.40), Some(8.76)).net() - 49.64).abs() < 1e-9);
        assert!(row("JOURNAL", None, None, None).moves_no_cash());
        assert!(!row("JOURNAL", Some(0.32), None, None).moves_no_cash());
    }

    #[test]
    fn money_parses_dollars_commas_and_signs() {
        assert_eq!(parse_money("$2,350.47"), Some(2350.47));
        assert_eq!(parse_money("-9.65"), Some(-9.65));
        assert_eq!(parse_money("(1.00)"), Some(-1.0));
        assert_eq!(parse_money("WH"), None);
    }
}
