//! Trading 212 feed.
//!
//! Authentication is HTTP Basic (`api_key:api_secret`). The history endpoints
//! paginate with `nextPagePath`; each page is folded into one document so the
//! parser sees a single array per endpoint. Rate-limit headers are honoured and
//! a 429 is retried once after the advertised reset.

use std::{collections::BTreeMap, future::Future, time::Duration};

use chrono::{Months, NaiveDate, TimeZone, Utc};
use serde_json::Value;

use super::{
    DocumentKind, ParseContext, Parser, RawDocument, SourceError, SourceId, ToDatabaseRecord,
    draft::{LineKind, StatementDraft, StatementLineDraft, SuggestedType},
    sha256_hex,
};
use crate::config::Config;

const PROVIDER: &str = "trading212";
const MAX_PAGES: usize = 40;
const MAX_RATE_LIMIT_WAIT_SECS: u64 = 60;

pub struct Trading212Source {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    api_secret: String,
    environment: String,
    lookback_days: i64,
}

impl Trading212Source {
    pub fn from_config(config: &Config, client: &reqwest::Client) -> anyhow::Result<Self> {
        let feed = &config.t212;
        if !feed.enabled {
            anyhow::bail!("Trading 212 feed is disabled in the configuration");
        }
        if feed.api_key.trim().is_empty() || feed.api_secret.trim().is_empty() {
            anyhow::bail!(
                "Trading 212 credentials are missing; set them in the desktop Settings overlay"
            );
        }
        let base_url = match feed.environment.as_str() {
            "demo" => "https://demo.trading212.com",
            _ => "https://live.trading212.com",
        };
        Ok(Self {
            client: client.clone(),
            base_url: base_url.to_string(),
            api_key: feed.api_key.clone(),
            api_secret: feed.api_secret.clone(),
            environment: feed.environment.clone(),
            lookback_days: feed.lookback_days.max(1),
        })
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else if path.starts_with('/') {
            format!("{}{}", self.base_url, path)
        } else {
            format!("{}/{}", self.base_url, path)
        }
    }

    async fn get_json(&self, path: &str) -> Result<Vec<u8>, SourceError> {
        let url = self.url(path);
        let mut retried = false;
        loop {
            let response = self
                .client
                .get(&url)
                .basic_auth(&self.api_key, Some(&self.api_secret))
                .send()
                .await
                .map_err(|error| SourceError::Http {
                    status: 0,
                    url: url.clone(),
                    body: error.to_string(),
                })?;
            let status = response.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && !retried {
                let wait = retry_after(response.headers());
                tracing::warn!(%url, wait, "Trading 212 rate limited; retrying");
                tokio::time::sleep(Duration::from_secs(wait)).await;
                retried = true;
                continue;
            }
            let headers = response.headers().clone();
            let body = response.bytes().await.map_err(|error| SourceError::Http {
                status: status.as_u16(),
                url: url.clone(),
                body: error.to_string(),
            })?;
            if !status.is_success() {
                return Err(SourceError::Http {
                    status: status.as_u16(),
                    url,
                    body: String::from_utf8_lossy(&body).into_owned(),
                });
            }
            if let Some(wait) = rate_limit_wait(&headers) {
                if wait > 0 {
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
            }
            return Ok(body.to_vec());
        }
    }

    async fn get_paginated(&self, path: &str) -> Result<Vec<u8>, SourceError> {
        let mut items = Vec::new();
        let mut next = Some(path.to_string());
        let mut pages = 0;
        while let Some(path) = next {
            pages += 1;
            if pages > MAX_PAGES {
                tracing::warn!(path, "Trading 212 pagination limit reached");
                break;
            }
            let body = self.get_json(&path).await?;
            let value: Value = serde_json::from_slice(&body)
                .map_err(|error| SourceError::malformed(format!("Trading 212 {path}: {error}")))?;
            if let Some(array) = value.get("items").and_then(Value::as_array) {
                items.extend(array.iter().cloned());
            } else if let Some(array) = value.as_array() {
                items.extend(array.iter().cloned());
            }
            next = value
                .get("nextPagePath")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        serde_json::to_vec(&serde_json::json!({ "items": items }))
            .map_err(|error| SourceError::malformed(error.to_string()))
    }

    /// True when the line is older than the configured lookback. Used to stop
    /// the parser from staging a whole account history on the first sync.
    ///
    /// The cutoff is anchored to the fetch time in [`ParseContext`] rather than
    /// the wall clock: parsing the same bytes twice must produce the same
    /// drafts, or a re-run could silently drop a line near the boundary.
    fn within_lookback(&self, posted_at: i64, fetched_at: i64) -> bool {
        let cutoff = fetched_at - self.lookback_days * 86_400_000;
        posted_at >= cutoff
    }
}

impl Parser for Trading212Source {
    type Output = Vec<T212Statement>;

    fn source(&self) -> SourceId {
        SourceId::Trading212
    }

    fn acquire(&self) -> impl Future<Output = Result<Vec<RawDocument>, SourceError>> + Send {
        async move {
            let summary = self.get_json("/api/v0/equity/account/summary").await?;
            let transactions = self
                .get_paginated("/api/v0/equity/history/transactions?limit=50")
                .await?;
            let dividends = self
                .get_paginated("/api/v0/equity/history/dividends?limit=50")
                .await?;
            let orders = self
                .get_paginated("/api/v0/equity/history/orders?limit=50")
                .await?;
            Ok(vec![
                RawDocument::api(DocumentKind::Json, summary, "equity/account/summary"),
                RawDocument::api(
                    DocumentKind::Json,
                    transactions,
                    "equity/history/transactions",
                ),
                RawDocument::api(DocumentKind::Json, dividends, "equity/history/dividends"),
                RawDocument::api(DocumentKind::Json, orders, "equity/history/orders"),
            ])
        }
    }

    fn parse(&self, docs: &[RawDocument], ctx: &ParseContext) -> Result<Self::Output, SourceError> {
        if docs.len() < 4 {
            return Err(SourceError::malformed(
                "expected Trading 212 summary, transactions, dividends and orders",
            ));
        }
        let summary = parse_json(&docs[0], "summary")?;
        let transactions = parse_json(&docs[1], "transactions")?;
        let dividends = parse_json(&docs[2], "dividends")?;
        let orders = parse_json(&docs[3], "orders")?;

        let currency = summary_string(&summary, &["currencyCode"])
            .or_else(|| summary_string(&summary, &["currency"]))
            .or_else(|| summary_string(&summary, &["account", "currency"]))
            .unwrap_or_else(|| "EUR".to_string());
        let account_number = summary_string(&summary, &["id"])
            .or_else(|| summary_string(&summary, &["accountId"]))
            .or_else(|| summary_string(&summary, &["account", "id"]))
            .unwrap_or_else(|| self.environment.clone());

        let mut by_month: BTreeMap<String, Vec<StatementLineDraft>> = BTreeMap::new();
        let mut warnings = Vec::new();

        for item in array_items(&transactions) {
            let Some(posted_at) = item_date(&item) else {
                warnings.push(format!("transaction without a date: {item}"));
                continue;
            };
            if !self.within_lookback(posted_at, ctx.fetched_at) {
                continue;
            }
            let type_ = item
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("OTHER")
                .to_string();
            let reference = item
                .get("reference")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let amount = item.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
            let amount = match type_.to_ascii_uppercase().as_str() {
                "WITHDRAW" | "FEE" => -amount.abs(),
                "DEPOSIT" | "INTEREST_ON_FREE_CASH" | "LENDING_INTEREST" => amount.abs(),
                _ => amount,
            };
            let line_currency = item
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or(&currency)
                .to_string();
            let (line_kind, suggested_type) = cash_kind(&type_, amount);
            let description = cash_description(&type_, reference.as_deref());
            let source_ref = item
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| reference.clone());
            let fingerprint = line_fingerprint(
                PROVIDER,
                &account_number,
                source_ref.as_deref(),
                &type_,
                posted_at,
                amount,
                reference.as_deref(),
            );
            let dedupe_key = sha256_hex(&[PROVIDER, &account_number, &fingerprint]);
            by_month
                .entry(month_key(posted_at))
                .or_default()
                .push(StatementLineDraft {
                    source_ref,
                    posted_at,
                    description,
                    amount,
                    currency_code: line_currency,
                    balance_after: None,
                    category_hint: suggested_category(&type_, line_kind),
                    line_kind,
                    suggested_type,
                    suggested_category: suggested_category(&type_, line_kind),
                    metadata: serde_json::json!({
                        "reference": reference,
                        "type": type_,
                        "raw": item,
                    }),
                    fingerprint,
                    dedupe_key,
                });
        }

        for item in array_items(&dividends) {
            let Some(posted_at) = item_date(&item) else {
                warnings.push(format!("dividend without a date: {item}"));
                continue;
            };
            if !self.within_lookback(posted_at, ctx.fetched_at) {
                continue;
            }
            let amount = item
                .get("amount")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .abs();
            let ticker = item
                .get("ticker")
                .and_then(Value::as_str)
                .or_else(|| {
                    item.get("instrument")
                        .and_then(|v| v.get("ticker"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("")
                .to_string();
            let reference = item
                .get("reference")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| item.get("id").and_then(Value::as_str).map(str::to_owned));
            let line_currency = item
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or(&currency)
                .to_string();
            let fingerprint = line_fingerprint(
                PROVIDER,
                &account_number,
                reference.as_deref(),
                "DIVIDEND",
                posted_at,
                amount,
                reference.as_deref(),
            );
            let dedupe_key = sha256_hex(&[PROVIDER, &account_number, &fingerprint]);
            by_month
                .entry(month_key(posted_at))
                .or_default()
                .push(StatementLineDraft {
                    source_ref: reference.clone(),
                    posted_at,
                    description: if ticker.is_empty() {
                        "Dividend".to_string()
                    } else {
                        format!("Dividend {ticker}")
                    },
                    amount,
                    currency_code: line_currency,
                    balance_after: None,
                    category_hint: Some("Dividends".to_string()),
                    line_kind: LineKind::Dividend,
                    suggested_type: Some(SuggestedType::Income),
                    suggested_category: Some("Dividends".to_string()),
                    metadata: serde_json::json!({
                        "reference": reference,
                        "ticker": ticker,
                        "raw": item,
                    }),
                    fingerprint,
                    dedupe_key,
                });
        }

        for item in array_items(&orders) {
            let order = item.get("order").unwrap_or(&item);
            let fill = item.get("fill");
            let Some(posted_at) = fill.and_then(item_date).or_else(|| item_date(order)) else {
                warnings.push(format!("order without a fill date: {item}"));
                continue;
            };
            if !self.within_lookback(posted_at, ctx.fetched_at) {
                continue;
            }
            let ticker = order
                .get("instrument")
                .and_then(|v| v.get("ticker"))
                .and_then(Value::as_str)
                .or_else(|| order.get("ticker").and_then(Value::as_str))
                .unwrap_or("")
                .to_string();
            let side = order
                .get("side")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let quantity = order
                .get("quantity")
                .and_then(Value::as_f64)
                .or_else(|| fill.and_then(|v| v.get("quantity")).and_then(Value::as_f64));
            let price = fill.and_then(|v| v.get("price")).and_then(Value::as_f64);
            let net_value = fill
                .and_then(|v| v.get("walletImpact"))
                .and_then(|v| v.get("netValue"))
                .and_then(Value::as_f64);
            let order_id = order.get("id").and_then(Value::as_str).map(str::to_owned);
            let fill_id = fill
                .and_then(|v| v.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let source_ref = fill_id.clone().or_else(|| order_id.clone());
            let fingerprint = line_fingerprint(
                PROVIDER,
                &account_number,
                source_ref.as_deref(),
                "TRADE",
                posted_at,
                0.0,
                order_id.as_deref(),
            );
            let dedupe_key = sha256_hex(&[PROVIDER, &account_number, &fingerprint]);
            by_month
                .entry(month_key(posted_at))
                .or_default()
                .push(StatementLineDraft {
                    source_ref,
                    posted_at,
                    description: format!(
                        "{} {}{}",
                        side,
                        quantity.map(|q| format!("{q} ")).unwrap_or_default(),
                        ticker
                    )
                    .trim()
                    .to_string(),
                    amount: 0.0,
                    currency_code: currency.clone(),
                    balance_after: None,
                    category_hint: None,
                    line_kind: LineKind::Trade,
                    suggested_type: None,
                    suggested_category: None,
                    metadata: serde_json::json!({
                        "ticker": ticker,
                        "side": side,
                        "quantity": quantity,
                        "price": price,
                        "net_value": net_value,
                        "order_id": order_id,
                        "fill_id": fill_id,
                        "raw": item,
                    }),
                    fingerprint,
                    dedupe_key,
                });
        }

        let statements = by_month
            .into_iter()
            .map(|(month, lines)| {
                let (period_start, period_end) = month_bounds(&month);
                T212Statement {
                    month,
                    account_name: "Trading 212".to_string(),
                    account_number: account_number.clone(),
                    currency_code: currency.clone(),
                    period_start,
                    period_end,
                    source_hash: ctx.source_hash.clone(),
                    stored_path: ctx.stored_path.clone(),
                    lines,
                    warnings: warnings.clone(),
                }
            })
            .collect();
        Ok(statements)
    }
}

#[derive(Debug, Clone)]
pub struct T212Statement {
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

impl ToDatabaseRecord for Vec<T212Statement> {
    fn into_drafts(self) -> Vec<StatementDraft> {
        self.into_iter().map(T212Statement::into_draft).collect()
    }
}

impl T212Statement {
    fn into_draft(self) -> StatementDraft {
        StatementDraft {
            source: SourceId::Trading212,
            institution: SourceId::Trading212.institution().to_string(),
            account_name: self.account_name,
            account_number: Some(self.account_number.clone()),
            currency_code: self.currency_code,
            period_start: Some(self.period_start.clone()),
            period_end: Some(self.period_end.clone()),
            opening_balance: None,
            closing_balance: None,
            source_path: None,
            stored_path: self.stored_path,
            source_hash: self.source_hash,
            source_key: format!("{}:{}:{}", PROVIDER, self.account_number, self.month),
            parsed: serde_json::json!({
                "month": self.month,
                "lines": self.lines.len(),
            }),
            warnings: self.warnings,
            lines: self.lines,
        }
    }
}

fn parse_json(doc: &RawDocument, label: &str) -> Result<Value, SourceError> {
    serde_json::from_slice(&doc.bytes)
        .map_err(|error| SourceError::malformed(format!("Trading 212 {label}: {error}")))
}

fn array_items(value: &Value) -> Vec<Value> {
    value
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| value.as_array().cloned())
        .unwrap_or_default()
}

fn summary_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn item_date(value: &Value) -> Option<i64> {
    for key in ["filledAt", "dateTime", "date", "paidOn", "orderedAt"] {
        if let Some(raw) = value.get(key).and_then(Value::as_str) {
            if let Some(ms) = parse_datetime(raw) {
                return Some(ms);
            }
        }
    }
    None
}

fn parse_datetime(raw: &str) -> Option<i64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(dt.timestamp_millis());
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(raw, format) {
            return Some(dt.and_utc().timestamp_millis());
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc().timestamp_millis());
    }
    None
}

fn cash_kind(type_: &str, amount: f64) -> (LineKind, Option<SuggestedType>) {
    match type_.to_ascii_uppercase().as_str() {
        "DEPOSIT" => (LineKind::TransferIn, Some(SuggestedType::Transfer)),
        "WITHDRAW" => (LineKind::TransferOut, Some(SuggestedType::Transfer)),
        "TRANSFER" => (LineKind::Transfer, Some(SuggestedType::Transfer)),
        "FEE" => (LineKind::Fee, Some(SuggestedType::Expense)),
        "INTEREST_ON_FREE_CASH" | "LENDING_INTEREST" => {
            (LineKind::Interest, Some(SuggestedType::Income))
        }
        _ if amount < 0.0 => (LineKind::Debit, Some(SuggestedType::Expense)),
        _ => (LineKind::Credit, Some(SuggestedType::Income)),
    }
}

fn cash_description(type_: &str, reference: Option<&str>) -> String {
    let upper = type_.to_ascii_uppercase();
    let label = match upper.as_str() {
        "DEPOSIT" => "Deposit",
        "WITHDRAW" => "Withdrawal",
        "TRANSFER" => "Transfer",
        "FEE" => "Fee",
        "INTEREST_ON_FREE_CASH" => "Interest on free cash",
        "LENDING_INTEREST" => "Lending interest",
        other => other,
    };
    match reference {
        Some(reference) if !reference.is_empty() => format!("{label} {reference}"),
        _ => label.to_string(),
    }
}

fn suggested_category(type_: &str, kind: LineKind) -> Option<String> {
    match type_.to_ascii_uppercase().as_str() {
        "FEE" => Some("Fees".to_string()),
        "INTEREST_ON_FREE_CASH" | "LENDING_INTEREST" => Some("Interest".to_string()),
        _ => match kind {
            LineKind::Transfer | LineKind::TransferIn | LineKind::TransferOut => {
                Some("Transfer".to_string())
            }
            _ => None,
        },
    }
}

fn line_fingerprint(
    provider: &str,
    account: &str,
    source_ref: Option<&str>,
    type_: &str,
    posted_at: i64,
    amount: f64,
    reference: Option<&str>,
) -> String {
    match source_ref {
        Some(reference) if !reference.is_empty() => {
            sha256_hex(&[provider, account, "ref", reference])
        }
        _ => sha256_hex(&[
            provider,
            account,
            type_,
            &posted_at.to_string(),
            &format!("{amount:.4}"),
            reference.unwrap_or(""),
        ]),
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
    let next = first.checked_add_months(Months::new(1)).unwrap_or(first);
    let last = next.pred_opt().unwrap_or(first);
    (
        first.format("%Y-%m-%d").to_string(),
        last.format("%Y-%m-%d").to_string(),
    )
}

fn retry_after(headers: &reqwest::header::HeaderMap) -> u64 {
    headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .map(|reset| {
            let now = Utc::now().timestamp();
            let wait = if reset > 1_000_000_000 {
                reset - now
            } else {
                reset
            };
            wait.clamp(1, MAX_RATE_LIMIT_WAIT_SECS as i64) as u64
        })
        .unwrap_or(5)
}

fn rate_limit_wait(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())?;
    if remaining > 0 {
        return Some(0);
    }
    let reset = headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())?;
    let now = Utc::now().timestamp();
    let wait = if reset > 1_000_000_000 {
        reset - now
    } else {
        reset
    };
    Some(wait.clamp(0, MAX_RATE_LIMIT_WAIT_SECS as i64) as u64)
}

/// Unused import guard for `TimeZone`; kept because timestamp helpers in this
/// module use UTC consistently.
#[allow(dead_code)]
fn utc(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(body: Value, endpoint: &str) -> RawDocument {
        RawDocument::api(
            DocumentKind::Json,
            serde_json::to_vec(&body).unwrap(),
            endpoint,
        )
    }

    fn ctx() -> ParseContext {
        ParseContext {
            source_hash: "hash".to_string(),
            stored_path: None,
            fetched_at: 0,
        }
    }

    fn source() -> Trading212Source {
        Trading212Source {
            client: reqwest::Client::new(),
            base_url: "https://live.trading212.com".to_string(),
            api_key: "key".to_string(),
            api_secret: "secret".to_string(),
            environment: "live".to_string(),
            lookback_days: 100_000,
        }
    }

    #[test]
    fn maps_cash_transactions_dividends_and_trades() {
        let docs = vec![
            doc(
                serde_json::json!({"currencyCode": "EUR", "id": "acc-1"}),
                "summary",
            ),
            doc(
                serde_json::json!({"items": [
                    {"type": "DEPOSIT", "amount": 1000.0, "currency": "EUR", "dateTime": "2026-10-16T10:00:00Z", "reference": "36873033", "id": "tx-1"},
                    {"type": "FEE", "amount": 1.0, "currency": "EUR", "dateTime": "2026-10-17T10:00:00Z", "reference": "fee-1", "id": "tx-2"},
                    {"type": "INTEREST_ON_FREE_CASH", "amount": 2.5, "currency": "EUR", "dateTime": "2026-10-18T10:00:00Z", "reference": "int-1", "id": "tx-3"}
                ]}),
                "transactions",
            ),
            doc(
                serde_json::json!({"items": [
                    {"ticker": "VWCE", "amount": 12.34, "currency": "EUR", "paidOn": "2026-10-19T00:00:00Z", "reference": "div-1"}
                ]}),
                "dividends",
            ),
            doc(
                serde_json::json!({"items": [
                    {"order": {"id": "o-1", "side": "BUY", "quantity": 2.0, "instrument": {"ticker": "VWCE"}},
                     "fill": {"id": "f-1", "filledAt": "2026-10-20T12:00:00Z", "price": 100.0, "walletImpact": {"netValue": -200.0}}}
                ]}),
                "orders",
            ),
        ];
        let statements = source().parse(&docs, &ctx()).unwrap();
        assert_eq!(statements.len(), 1);
        let lines = &statements[0].lines;
        assert_eq!(lines.len(), 5);

        let deposit = lines
            .iter()
            .find(|l| l.source_ref.as_deref() == Some("tx-1"))
            .unwrap();
        assert_eq!(deposit.line_kind, LineKind::TransferIn);
        assert_eq!(deposit.amount, 1000.0);
        assert_eq!(deposit.metadata["reference"], "36873033");

        let fee = lines
            .iter()
            .find(|l| l.source_ref.as_deref() == Some("tx-2"))
            .unwrap();
        assert_eq!(fee.line_kind, LineKind::Fee);
        assert_eq!(fee.amount, -1.0);

        let dividend = lines
            .iter()
            .find(|l| l.line_kind == LineKind::Dividend)
            .unwrap();
        assert_eq!(dividend.amount, 12.34);
        assert_eq!(dividend.suggested_type, Some(SuggestedType::Income));

        let trade = lines
            .iter()
            .find(|l| l.line_kind == LineKind::Trade)
            .unwrap();
        assert_eq!(trade.amount, 0.0);
        assert!(trade.suggested_type.is_none());
        assert_eq!(trade.metadata["ticker"], "VWCE");
    }

    #[test]
    fn splits_months_and_uses_stable_source_keys() {
        let docs = vec![
            doc(
                serde_json::json!({"currencyCode": "EUR", "id": "acc-1"}),
                "summary",
            ),
            doc(
                serde_json::json!({"items": [
                    {"type": "DEPOSIT", "amount": 1.0, "currency": "EUR", "dateTime": "2026-09-30T10:00:00Z", "id": "a"},
                    {"type": "DEPOSIT", "amount": 2.0, "currency": "EUR", "dateTime": "2026-10-01T10:00:00Z", "id": "b"}
                ]}),
                "transactions",
            ),
            doc(serde_json::json!({"items": []}), "dividends"),
            doc(serde_json::json!({"items": []}), "orders"),
        ];
        let statements = source().parse(&docs, &ctx()).unwrap();
        assert_eq!(statements.len(), 2);
        let keys: Vec<_> = statements
            .into_iter()
            .map(|s| s.into_draft().source_key)
            .collect();
        assert_eq!(
            keys,
            vec!["trading212:acc-1:2026-09", "trading212:acc-1:2026-10"]
        );
    }

    #[test]
    fn parses_rate_limit_reset_as_epoch_or_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
        let reset = Utc::now().timestamp() + 10;
        headers.insert("x-ratelimit-reset", reset.to_string().parse().unwrap());
        let wait = rate_limit_wait(&headers).unwrap();
        assert!((8..=10).contains(&wait));
    }
}
