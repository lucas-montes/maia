//! Crédit Agricole statement parser.
//!
//! The exported CSV is Windows-1252, semicolon separated, with one section per
//! account in a single file. A section looks like:
//!
//! ```text
//! Eko by CA carte n° 84890097587;
//! Solde au 27/10/2026 188,76 €
//! Liste des opérations du compte entre le 01/10/2025 et le 27/10/2026;
//! Date;Libellé;Débit euros;Crédit euros;
//! 27/10/2026;"Virement émis
//! VIR INST vers Dora Pavelic";250,00;;
//! ```
//!
//! Descriptions are quoted and can span lines, so this uses the `csv` crate
//! rather than splitting on `;` by hand.

use std::{collections::HashMap, future::Future, path::PathBuf};

use chrono::NaiveDate;
use serde::Serialize;

use super::{
    DocumentKind, ParseContext, Parser, RawDocument, SourceError, SourceId, ToDatabaseRecord,
    draft::{LineKind, StatementDraft, StatementLineDraft, SuggestedType},
    sha256_hex,
};

pub struct CreditAgricoleSource {
    path: PathBuf,
}

impl CreditAgricoleSource {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Parser for CreditAgricoleSource {
    type Output = Vec<CaStatement>;

    fn source(&self) -> SourceId {
        SourceId::CreditAgricole
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
            let kind = path
                .extension()
                .and_then(|ext| ext.to_str())
                .and_then(DocumentKind::parse_extension)
                .unwrap_or(DocumentKind::Csv);
            Ok(vec![RawDocument::file(kind, bytes, path)])
        }
    }

    fn parse(&self, docs: &[RawDocument], ctx: &ParseContext) -> Result<Self::Output, SourceError> {
        let doc = docs
            .first()
            .ok_or_else(|| SourceError::malformed("no statement document"))?;
        let sections = match doc.kind {
            DocumentKind::Csv => parse_csv(&doc.bytes)?,
            DocumentKind::Pdf => {
                return Err(SourceError::Unsupported(
                    "Crédit Agricole PDF extraction is not configured; use the CSV export"
                        .to_string(),
                ));
            }
            DocumentKind::Json => {
                return Err(SourceError::Unsupported(
                    "Crédit Agricole statements are CSV or PDF, not JSON".to_string(),
                ));
            }
        };
        Ok(sections
            .into_iter()
            .map(|section| CaStatement {
                section,
                source_hash: ctx.source_hash.clone(),
                stored_path: ctx.stored_path.clone(),
            })
            .collect())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CaStatement {
    pub section: CaSection,
    pub source_hash: String,
    pub stored_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaSection {
    pub account_name: String,
    pub account_number: Option<String>,
    pub currency_code: String,
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    pub closing_date: Option<NaiveDate>,
    pub closing_balance: Option<f64>,
    pub opening_balance: Option<f64>,
    pub movements: Vec<CaMovement>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaMovement {
    pub posted_at: NaiveDate,
    pub description: String,
    pub amount: f64,
    pub raw_description: String,
    pub reference: Option<String>,
}

impl ToDatabaseRecord for Vec<CaStatement> {
    fn into_drafts(self) -> Vec<StatementDraft> {
        self.into_iter().map(CaStatement::into_draft).collect()
    }
}

impl CaStatement {
    fn into_draft(self) -> StatementDraft {
        let section = self.section;
        let account_key = section
            .account_number
            .clone()
            .unwrap_or_else(|| section.account_name.clone());
        let source_key = format!(
            "{}:{}:{}",
            SourceId::CreditAgricole.as_str(),
            account_key,
            self.source_hash
        );

        let mut occurrences: HashMap<(String, String, String), usize> = HashMap::new();
        let lines = section
            .movements
            .iter()
            .map(|movement| {
                let date = movement.posted_at.format("%Y-%m-%d").to_string();
                let amount = format!("{:.2}", movement.amount);
                let key = (date.clone(), amount.clone(), movement.description.clone());
                let occurrence = occurrences.entry(key).or_insert(0);
                let index = *occurrence;
                *occurrence += 1;

                let fingerprint =
                    sha256_hex(&[&date, &amount, &movement.description, &index.to_string()]);
                let dedupe_key = sha256_hex(&[
                    SourceId::CreditAgricole.as_str(),
                    &account_key,
                    &fingerprint,
                ]);
                let (line_kind, suggested_type, suggested_category) =
                    classify(&movement.description, movement.amount);
                let metadata = serde_json::json!({
                    "raw_description": movement.raw_description,
                    "reference": movement.reference,
                    "occurrence": index,
                });
                StatementLineDraft {
                    source_ref: movement.reference.clone(),
                    posted_at: date_ms(movement.posted_at),
                    description: movement.description.clone(),
                    amount: movement.amount,
                    currency_code: section.currency_code.clone(),
                    balance_after: None,
                    category_hint: suggested_category.clone(),
                    line_kind,
                    suggested_type: Some(suggested_type),
                    suggested_category,
                    metadata,
                    fingerprint,
                    dedupe_key,
                }
            })
            .collect::<Vec<_>>();

        let parsed = serde_json::to_value(&section).unwrap_or(serde_json::Value::Null);
        StatementDraft {
            source: SourceId::CreditAgricole,
            institution: SourceId::CreditAgricole.institution().to_string(),
            account_name: section.account_name,
            account_number: section.account_number,
            currency_code: section.currency_code,
            period_start: section
                .period_start
                .map(|d| d.format("%Y-%m-%d").to_string()),
            period_end: section.period_end.map(|d| d.format("%Y-%m-%d").to_string()),
            opening_balance: section.opening_balance,
            closing_balance: section.closing_balance,
            source_path: None,
            stored_path: self.stored_path,
            source_hash: self.source_hash,
            source_key,
            parsed,
            warnings: section.warnings,
            lines,
        }
    }
}

struct SectionBuilder {
    account_name: String,
    account_number: Option<String>,
    currency_code: String,
    period_start: Option<NaiveDate>,
    period_end: Option<NaiveDate>,
    closing_date: Option<NaiveDate>,
    closing_balance: Option<f64>,
    movements: Vec<CaMovement>,
    warnings: Vec<String>,
}

impl SectionBuilder {
    fn new(account_name: String, account_number: Option<String>) -> Self {
        Self {
            account_name,
            account_number,
            currency_code: "EUR".to_string(),
            period_start: None,
            period_end: None,
            closing_date: None,
            closing_balance: None,
            movements: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn finish(self) -> CaSection {
        let mut warnings = self.warnings;
        if self.closing_balance.is_none() {
            warnings.push("closing balance not found".to_string());
        }
        if self.movements.is_empty() {
            warnings.push("no movements found".to_string());
        }
        let sum: f64 = self.movements.iter().map(|m| m.amount).sum();
        let opening_balance = self.closing_balance.map(|closing| closing - sum);
        CaSection {
            account_name: self.account_name,
            account_number: self.account_number,
            currency_code: self.currency_code,
            period_start: self.period_start,
            period_end: self.period_end,
            closing_date: self.closing_date,
            closing_balance: self.closing_balance,
            opening_balance,
            movements: self.movements,
            warnings,
        }
    }
}

fn parse_csv(bytes: &[u8]) -> Result<Vec<CaSection>, SourceError> {
    let (text, _, had_errors) = encoding_rs::WINDOWS_1252.decode(bytes);
    if had_errors {
        // Windows-1252 maps every byte, so this is defensive rather than likely.
        tracing::warn!("Crédit Agricole CSV contained undecodable bytes");
    }

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());

    let mut sections = Vec::new();
    let mut current: Option<SectionBuilder> = None;

    for record in reader.records() {
        let record = record.map_err(|error| SourceError::malformed(format!("CSV: {error}")))?;
        let fields: Vec<&str> = record.iter().collect();
        let first = fields.first().copied().unwrap_or("").trim();

        if let Some((name, number)) = parse_preamble(first) {
            if let Some(section) = current.take() {
                sections.push(section.finish());
            }
            current = Some(SectionBuilder::new(name, number));
            continue;
        }

        let Some(section) = current.as_mut() else {
            continue;
        };

        if let Some(rest) = first.strip_prefix("Solde au ") {
            match parse_balance(rest) {
                Some((date, amount)) => {
                    section.closing_date = date;
                    section.closing_balance = Some(amount);
                }
                None => section
                    .warnings
                    .push(format!("unreadable closing balance: {first}")),
            }
            continue;
        }

        if let Some(rest) = first.strip_prefix("Liste des opérations du compte entre le ") {
            match parse_period(rest) {
                Some((start, end)) => {
                    section.period_start = Some(start);
                    section.period_end = Some(end);
                }
                None => section
                    .warnings
                    .push(format!("unreadable statement period: {first}")),
            }
            continue;
        }

        if first.eq_ignore_ascii_case("Date") || first.is_empty() {
            continue;
        }

        let Some(date) = parse_date(first) else {
            // Account holder name, blank separators, and the trailing download
            // banner are all expected here.
            continue;
        };

        let raw_description = fields.get(1).copied().unwrap_or("").to_string();
        let description = normalize_description(&raw_description);
        let debit = fields.get(2).copied().unwrap_or("").trim();
        let credit = fields.get(3).copied().unwrap_or("").trim();

        let amount = match (parse_amount(debit), parse_amount(credit)) {
            (Some(value), None) => -value.abs(),
            (None, Some(value)) => value.abs(),
            (Some(debit), Some(credit)) => {
                section.warnings.push(format!(
                    "row has both debit ({debit}) and credit ({credit}); using credit"
                ));
                credit.abs()
            }
            (None, None) => {
                section
                    .warnings
                    .push(format!("row has no amount: {description}"));
                continue;
            }
        };

        section.movements.push(CaMovement {
            posted_at: date,
            description: description.clone(),
            amount,
            raw_description,
            reference: extract_reference(&description),
        });
    }

    if let Some(section) = current.take() {
        sections.push(section.finish());
    }
    if sections.is_empty() {
        return Err(SourceError::malformed(
            "no Crédit Agricole account sections found; is this the right export?",
        ));
    }
    Ok(sections)
}

fn parse_preamble(line: &str) -> Option<(String, Option<String>)> {
    let (name, number) = line.split_once("carte n°")?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let number = number.trim().trim_end_matches(';').trim();
    Some((
        name.to_string(),
        (!number.is_empty()).then(|| number.to_string()),
    ))
}

fn parse_balance(rest: &str) -> Option<(Option<NaiveDate>, f64)> {
    let (date, amount) = rest.split_once(' ')?;
    let date = parse_date(date);
    let amount = parse_amount(amount)?;
    Some((date, amount))
}

fn parse_period(rest: &str) -> Option<(NaiveDate, NaiveDate)> {
    let (start, end) = rest.split_once(" et ")?;
    let start = parse_date(start)?;
    // The export repeats the preposition: "entre le 01/10/2025 et le 27/10/2026".
    let end = end.trim().trim_end_matches(';').trim();
    let end = end.strip_prefix("le ").unwrap_or(end);
    let end = parse_date(end)?;
    Some((start, end))
}

fn parse_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%d/%m/%Y").ok()
}

fn parse_amount(raw: &str) -> Option<f64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{00a0}' && *c != '\u{202f}' && *c != '€')
        .map(|c| if c == ',' { '.' } else { c })
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

fn normalize_description(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Pull the provider reference out of a description. T212 deposits carry an
/// 8-digit reference in the bank line (e.g. `36873033`), which is what makes
/// the cross-source transfer match high confidence.
fn extract_reference(description: &str) -> Option<String> {
    let mut runs = Vec::new();
    let mut current = String::new();
    for character in description.chars() {
        if character.is_ascii_digit() {
            current.push(character);
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs.into_iter()
        .filter(|run| (6..=10).contains(&run.len()))
        .next_back()
}

fn classify(description: &str, amount: f64) -> (LineKind, SuggestedType, Option<String>) {
    let lower = description.to_lowercase();
    let debit = amount < 0.0;

    if lower.contains("virement") || lower.contains("vir inst") {
        return (
            if debit {
                LineKind::TransferOut
            } else {
                LineKind::TransferIn
            },
            SuggestedType::Transfer,
            Some("Transfer".to_string()),
        );
    }
    if lower.contains("intérêts") || lower.contains("interets") {
        return (
            LineKind::Interest,
            SuggestedType::Income,
            Some("Interest".to_string()),
        );
    }
    if lower.contains("avoir") || lower.contains("annul") {
        return (
            if debit {
                LineKind::Debit
            } else {
                LineKind::Credit
            },
            if debit {
                SuggestedType::Expense
            } else {
                SuggestedType::Income
            },
            Some("Refund".to_string()),
        );
    }
    if lower.contains("cotisation") || lower.contains("frais") {
        return (
            LineKind::Fee,
            SuggestedType::Expense,
            Some("Bank fees".to_string()),
        );
    }

    let category = if ["sfr", "orange", "free", "bouygues"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        Some("Telecom".to_string())
    } else if lower.contains("eko by ca") || lower.contains("credit agricole") {
        Some("Bank fees".to_string())
    } else if lower.contains("prélèvement") || lower.contains("prelevement") {
        Some("Bills".to_string())
    } else if lower.contains("paiement par carte") || lower.contains("retrait") {
        Some("Shopping".to_string())
    } else {
        None
    };

    if debit {
        (LineKind::Debit, SuggestedType::Expense, category)
    } else {
        (LineKind::Credit, SuggestedType::Income, category)
    }
}

fn date_ms(date: NaiveDate) -> i64 {
    date.and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_utc()
        .timestamp_millis()
}

/// True when a date falls inside the section period. Used by tests and future
/// PDF parsing; kept here so the rule is not re-derived.
#[allow(dead_code)]
fn in_period(date: NaiveDate, start: Option<NaiveDate>, end: Option<NaiveDate>) -> bool {
    start.map(|s| date >= s).unwrap_or(true) && end.map(|e| date <= e).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real export is personal, so it is not committed. These tests run
    /// when the file is present -- locally, or wherever `MAIA_CA_FIXTURE`
    /// points -- and skip otherwise. The Nix sandbox has no home directory and
    /// must still build.
    fn fixture() -> Option<Vec<u8>> {
        let path = std::env::var("MAIA_CA_FIXTURE")
            .unwrap_or_else(|_| "/home/lucas/Downloads/CA20261027_184837.csv".to_string());
        match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(_) => {
                eprintln!(
                    "skipping CA parser test: set MAIA_CA_FIXTURE to a Crédit Agricole CSV export"
                );
                None
            }
        }
    }

    #[test]
    fn parses_every_account_section_and_movement() {
        let Some(bytes) = fixture() else { return };
        let sections = parse_csv(&bytes).unwrap();
        assert_eq!(sections.len(), 3, "three account sections");

        let by_number: HashMap<_, _> = sections
            .iter()
            .map(|s| (s.account_number.clone().unwrap(), s))
            .collect();

        let eko = by_number.get("84890097587").unwrap();
        assert_eq!(eko.account_name, "Eko by CA");
        assert_eq!(eko.movements.len(), 82);
        assert_eq!(eko.closing_balance, Some(188.76));
        assert_eq!(
            eko.period_start,
            Some(NaiveDate::from_ymd_opt(2025, 10, 1).unwrap())
        );
        assert_eq!(
            eko.period_end,
            Some(NaiveDate::from_ymd_opt(2026, 10, 27).unwrap())
        );

        let livret = by_number.get("84890102343").unwrap();
        assert_eq!(livret.account_name, "Livret A");
        assert_eq!(livret.movements.len(), 12);
        assert_eq!(livret.closing_balance, Some(8501.21));

        let ldd = by_number.get("84891165314").unwrap();
        assert_eq!(ldd.account_name, "LDD Solidaire");
        assert_eq!(ldd.movements.len(), 4);
        assert_eq!(ldd.closing_balance, Some(10000.08));

        assert_eq!(
            sections.iter().map(|s| s.movements.len()).sum::<usize>(),
            98
        );
    }

    #[test]
    fn parses_multi_line_descriptions_and_amounts() {
        let Some(bytes) = fixture() else { return };
        let sections = parse_csv(&bytes).unwrap();
        let eko = sections
            .iter()
            .find(|s| s.account_number.as_deref() == Some("84890097587"))
            .unwrap();

        let t212 = eko
            .movements
            .iter()
            .find(|m| m.description.contains("Trading 212"))
            .unwrap();
        assert_eq!(t212.amount, -1000.0);
        assert_eq!(t212.reference.as_deref(), Some("36873033"));
        assert_eq!(
            t212.description,
            "Virement émis VIR INST vers Trading 212 Market 36873033"
        );
    }

    #[test]
    fn classifies_transfers_interest_and_bills() {
        let Some(bytes) = fixture() else { return };
        let sections = parse_csv(&bytes).unwrap();
        let eko = sections
            .iter()
            .find(|s| s.account_number.as_deref() == Some("84890097587"))
            .unwrap();

        let transfer = eko
            .movements
            .iter()
            .find(|m| m.description.starts_with("Virement émis"))
            .unwrap();
        assert_eq!(
            classify(&transfer.description, transfer.amount),
            (
                LineKind::TransferOut,
                SuggestedType::Transfer,
                Some("Transfer".to_string())
            )
        );

        let interest = eko
            .movements
            .iter()
            .find(|m| m.description.contains("Intérêts"))
            .unwrap();
        assert_eq!(
            classify(&interest.description, interest.amount).0,
            LineKind::Interest
        );
    }

    #[test]
    fn draft_has_stable_ids_and_one_statement_per_section() {
        let Some(bytes) = fixture() else { return };
        let sections = parse_csv(&bytes).unwrap();
        let statements: Vec<CaStatement> = sections
            .into_iter()
            .map(|section| CaStatement {
                section,
                source_hash: "abc".to_string(),
                stored_path: None,
            })
            .collect();
        let drafts = statements.into_drafts();
        assert_eq!(drafts.len(), 3);

        let eko = drafts
            .iter()
            .find(|d| d.account_number.as_deref() == Some("84890097587"))
            .unwrap();
        assert_eq!(eko.lines.len(), 82);
        assert_eq!(eko.source_key, "credit_agricole:84890097587:abc");

        let first = &eko.lines[0];
        let second = &eko.lines[1];
        assert_ne!(first.dedupe_key, second.dedupe_key);
        assert_eq!(
            first.dedupe_key,
            sha256_hex(&["credit_agricole", "84890097587", &first.fingerprint])
        );
    }
}
