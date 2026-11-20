//! Periodic FX rate pull.
//!
//! The server is the only writer of `fx_rates`: the phone pulls them, it never
//! pushes or edits them. Statement imports convert through
//! `statements::store::rate_for`, which takes the latest rate on or before a
//! line's date. So any currency in use needs rates going back to its oldest
//! dated row, or that row fails with `no USD/EUR FX rate on or before …`.
//!
//! Each tick does three things:
//!
//! 1. It lists every non-base currency that a dated row uses (transactions,
//!    statement lines, ingredient prices), with the earliest date it needs.
//! 2. It works out which part of `[earliest, today]` has no rates yet. That is
//!    the history before the oldest stored rate and the days after the newest.
//! 3. It fetches those ranges from a Frankfurter-compatible API (ECB daily
//!    rates, no key) one calendar year at a time, and upserts them.
//!
//! An upsert only touches a row whose value actually changed. Re-fetching the
//! last business day over a weekend therefore does not bump `updated_at`, and
//! the phone does not re-pull the same row every tick.
//!
//! Shaped like `crate::notifier`: a fixed-interval loop spawned from
//! `start_server_with_shutdown`, where the first tick runs at boot.

use std::time::Duration;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use chrono::{Datelike, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;

use crate::{db, error::error_response, server::AppState};

/// Starts the puller. Lives for as long as the server process.
pub fn spawn(state: AppState) {
    if !state.config.fx.enabled {
        tracing::info!("fx puller disabled");
        return;
    }
    tokio::spawn(main_loop(state));
}

async fn main_loop(state: AppState) {
    let hours = state.config.fx.interval_hours.max(1);
    let mut interval = tokio::time::interval(Duration::from_secs(hours * 3600));
    // A tick that overran (a long backfill, a suspended laptop) should not
    // fire a burst of catch-up pulls.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        match run_once(&state).await {
            Ok(written) => tracing::info!(written, "fx pull finished"),
            Err(error) => tracing::warn!(error = %error, "fx pull failed"),
        }
    }
}

/// `POST /fx-rates/refresh`: one pull now. Returns how many rows changed.
pub async fn refresh(State(state): State<AppState>) -> axum::response::Response {
    match run_once(&state).await {
        Ok(written) => Json(serde_json::json!({ "written": written })).into_response(),
        Err(error) => {
            error_response(StatusCode::BAD_GATEWAY, format!("fx pull failed: {error:#}")).into_response()
        }
    }
}

/// One full pull. A currency the provider does not know (a crypto code, a
/// typo) is logged and skipped, so it cannot block the others.
pub async fn run_once(state: &AppState) -> anyhow::Result<usize> {
    let base = state.config.base_currency.to_uppercase();
    let today = Utc::now().date_naive();
    let plan = {
        let conn = state.db.get()?;
        plan(&conn, &base, today)?
    };

    let mut written = 0usize;
    for fetch in plan {
        for (start, end) in year_chunks(fetch.start, fetch.end) {
            let rates = match fetch_range(state, &fetch.code, &base, start, end).await {
                Ok(rates) => rates,
                Err(error) => {
                    tracing::warn!(code = %fetch.code, %start, %end, error = %error, "fx fetch failed");
                    break;
                }
            };
            let conn = state.db.get()?;
            written += store(&conn, &fetch.code, &base, &rates, db::server_time_ms())?;
        }
    }
    Ok(written)
}

/// One `[start, end]` range of one currency still missing rates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    pub code: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

/// Rows written before dates existed carry this placeholder `rate_date`. They
/// do not count as coverage for any real day.
const UNDATED: &str = "1900-01-01";

/// What to fetch so that every dated row in a foreign currency has a rate on
/// or before its date, up to `today`.
pub fn plan(conn: &Connection, base: &str, today: NaiveDate) -> anyhow::Result<Vec<Fetch>> {
    // The earliest day each currency is needed. Every source column is epoch
    // milliseconds.
    let mut stmt = conn.prepare(
        "SELECT code, MIN(at) FROM (
             SELECT UPPER(TRIM(currency_code)) AS code, date AS at
               FROM transactions WHERE deleted_at IS NULL
             UNION ALL
             SELECT UPPER(TRIM(currency_code)), posted_at FROM bank_statement_lines
             UNION ALL
             SELECT UPPER(TRIM(currency_code)), recorded_at
               FROM ingredient_prices WHERE deleted_at IS NULL
         )
         WHERE code <> '' AND code <> ?1
         GROUP BY code
         ORDER BY code",
    )?;
    let needs: Vec<(String, i64)> = stmt
        .query_map(params![base], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;

    let mut out = Vec::new();
    for (code, earliest_ms) in needs {
        // Only three-letter codes can be ISO currencies. Anything else is
        // junk that would only produce a failed request on every tick.
        if code.len() != 3 || !code.chars().all(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        let earliest = chrono::DateTime::from_timestamp_millis(earliest_ms)
            .map(|dt| dt.date_naive())
            .unwrap_or(today)
            .min(today);

        let (min, max): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT MIN(rate_date), MAX(rate_date) FROM fx_rates
                 WHERE code=?1 AND base_code=?2 AND deleted_at IS NULL AND rate_date > ?3",
                params![code, base, UNDATED],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or((None, None));
        let parse = |s: Option<String>| s.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());

        match (parse(min), parse(max)) {
            (Some(min), Some(max)) => {
                if earliest < min {
                    out.push(Fetch { code: code.clone(), start: earliest, end: min.pred_opt().unwrap_or(min) });
                }
                if max < today {
                    out.push(Fetch { code: code.clone(), start: max.succ_opt().unwrap_or(max), end: today });
                }
            }
            _ => out.push(Fetch { code, start: earliest, end: today }),
        }
    }
    Ok(out)
}

/// Splits `[start, end]` at calendar-year boundaries. This keeps each response
/// to about 260 rows, however far back the history goes.
pub fn year_chunks(start: NaiveDate, end: NaiveDate) -> Vec<(NaiveDate, NaiveDate)> {
    let mut chunks = Vec::new();
    let mut from = start;
    while from <= end {
        let year_end = NaiveDate::from_ymd_opt(from.year(), 12, 31).expect("Dec 31 exists");
        let to = year_end.min(end);
        chunks.push((from, to));
        from = match to.succ_opt() {
            Some(next) => next,
            None => break,
        };
    }
    chunks
}

#[derive(Debug, Deserialize)]
struct TimeSeries {
    rates: std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>>,
}

/// Reads a Frankfurter time-series body into `(date, rate)` pairs for `base`.
///
/// With `base=CODE&symbols=BASE` each value is "units of BASE per 1 CODE",
/// which is exactly `rate_to_base`.
pub fn parse_time_series(body: &str, base: &str) -> anyhow::Result<Vec<(String, f64)>> {
    let series: TimeSeries = serde_json::from_str(body)?;
    Ok(series
        .rates
        .into_iter()
        .filter_map(|(date, by_code)| {
            let rate = *by_code.get(base)?;
            (rate.is_finite() && rate > 0.0).then_some((date, rate))
        })
        .collect())
}

async fn fetch_range(
    state: &AppState,
    code: &str,
    base: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> anyhow::Result<Vec<(String, f64)>> {
    let url = format!("{}/{start}..{end}", state.config.fx.base_url);
    let response = state
        .http
        .get(&url)
        .query(&[("base", code), ("symbols", base)])
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        anyhow::bail!("{url} answered {status}: {}", body.chars().take(200).collect::<String>());
    }
    parse_time_series(&body, base)
}

/// Upserts `rates` as automatic rows and returns how many rows changed.
///
/// A row that already holds the same value, is live and is automatic is left
/// alone. Its `updated_at` stays put, so the phone's incremental pull does not
/// see it again. An older hand-entered row (`manual = 1`, from when the phone
/// could push) is replaced: the server is the source now.
pub fn store(conn: &Connection, code: &str, base: &str, rates: &[(String, f64)], now: i64) -> anyhow::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut changed = 0usize;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO fx_rates (code, base_code, rate_date, rate_to_base, updated_at, manual, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL)
             ON CONFLICT(code, base_code, rate_date) DO UPDATE SET
                rate_to_base=excluded.rate_to_base,
                updated_at=excluded.updated_at,
                manual=0,
                deleted_at=NULL
             WHERE fx_rates.rate_to_base <> excluded.rate_to_base
                OR fx_rates.manual <> 0
                OR fx_rates.deleted_at IS NOT NULL",
        )?;
        for (date, rate) in rates {
            changed += stmt.execute(params![code, base, date, rate, now])?;
        }
    }
    tx.commit()?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn ms(s: &str) -> i64 {
        day(s).and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis()
    }

    fn insert_tx(conn: &Connection, id: &str, code: &str, date: &str) {
        conn.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, date, created_at)
             VALUES (?1, 'expense', 1.0, ?2, 1.0, 1.0, ?3, 0)",
            params![id, code, ms(date)],
        )
        .unwrap();
    }

    fn insert_rate(conn: &Connection, code: &str, date: &str, rate: f64, manual: bool) {
        conn.execute(
            "INSERT INTO fx_rates (code, base_code, rate_date, rate_to_base, updated_at, manual)
             VALUES (?1, 'EUR', ?2, ?3, 1, ?4)",
            params![code, date, rate, manual as i64],
        )
        .unwrap();
    }

    #[test]
    fn with_no_rates_the_whole_history_is_fetched() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        insert_tx(&conn, "a", "usd", "2021-08-06");
        insert_tx(&conn, "b", "USD", "2023-03-30");
        // Base-currency and blank rows need nothing.
        insert_tx(&conn, "c", "EUR", "2020-01-01");
        insert_tx(&conn, "d", "", "2020-01-01");

        let plan = plan(&conn, "EUR", day("2026-11-20")).unwrap();
        assert_eq!(
            plan,
            vec![Fetch { code: "USD".into(), start: day("2021-08-06"), end: day("2026-11-20") }]
        );
    }

    #[test]
    fn only_the_uncovered_ends_are_fetched() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        insert_tx(&conn, "a", "USD", "2021-08-06");
        insert_rate(&conn, "USD", "2023-11-16", 0.92, false);
        insert_rate(&conn, "USD", "2026-11-10", 0.86, false);
        // An undated legacy row is not coverage for any real day.
        insert_rate(&conn, "USD", "0001-01-01", 0.9, false);

        let plan = plan(&conn, "EUR", day("2026-11-20")).unwrap();
        assert_eq!(
            plan,
            vec![
                Fetch { code: "USD".into(), start: day("2021-08-06"), end: day("2023-11-15") },
                Fetch { code: "USD".into(), start: day("2026-11-11"), end: day("2026-11-20") },
            ]
        );
    }

    #[test]
    fn a_fully_covered_currency_needs_nothing() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        insert_tx(&conn, "a", "USD", "2026-11-01");
        insert_rate(&conn, "USD", "2026-10-31", 0.86, false);
        insert_rate(&conn, "USD", "2026-11-20", 0.86, false);
        assert!(plan(&conn, "EUR", day("2026-11-20")).unwrap().is_empty());
    }

    #[test]
    fn statement_lines_count_as_needing_a_rate() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO bank_statements (id, source, institution, account_name, currency_code, source_hash, source_key, status, created_at, updated_at)
             VALUES ('s', 'firstrade', 'Firstrade', 'Firstrade', 'USD', 'h', 'k', 'staged', 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO bank_statement_lines (id, statement_id, posted_at, description, amount, currency_code, fingerprint, dedupe_key, line_kind, status, created_at, updated_at)
             VALUES ('l', 's', ?1, 'x', 1.0, 'USD', 'f', 'd', 'credit', 'pending', 0, 0)",
            params![ms("2022-02-16")],
        )
        .unwrap();
        let plan = plan(&conn, "EUR", day("2026-11-20")).unwrap();
        assert_eq!(plan[0].start, day("2022-02-16"));
    }

    #[test]
    fn year_chunks_split_at_new_year() {
        assert_eq!(
            year_chunks(day("2021-08-06"), day("2023-03-01")),
            vec![
                (day("2021-08-06"), day("2021-12-31")),
                (day("2022-01-01"), day("2022-12-31")),
                (day("2023-01-01"), day("2023-03-01")),
            ]
        );
        assert!(year_chunks(day("2023-03-02"), day("2023-03-01")).is_empty());
    }

    #[test]
    fn parses_a_frankfurter_time_series() {
        let body = r#"{"amount":1.0,"base":"USD","start_date":"2021-07-30","end_date":"2021-08-03",
            "rates":{"2021-07-30":{"EUR":0.84097},"2021-08-02":{"EUR":0.84133},"2021-08-03":{"GBP":0.7}}}"#;
        assert_eq!(
            parse_time_series(body, "EUR").unwrap(),
            vec![("2021-07-30".to_string(), 0.84097), ("2021-08-02".to_string(), 0.84133)]
        );
    }

    #[test]
    fn storing_the_same_value_twice_changes_nothing() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        let rates = vec![("2026-11-13".to_string(), 0.86), ("2026-11-16".to_string(), 0.861)];
        assert_eq!(store(&conn, "USD", "EUR", &rates, 10).unwrap(), 2);
        assert_eq!(store(&conn, "USD", "EUR", &rates, 20).unwrap(), 0);
        let updated: i64 = conn
            .query_row("SELECT MAX(updated_at) FROM fx_rates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(updated, 10, "an unchanged row keeps its updated_at");
    }

    #[test]
    fn an_old_manual_rate_is_replaced() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        insert_rate(&conn, "USD", "2026-11-13", 0.9, true);
        assert_eq!(store(&conn, "USD", "EUR", &[("2026-11-13".into(), 0.9)], 30).unwrap(), 1);
        let (rate, manual): (f64, i64) = conn
            .query_row("SELECT rate_to_base, manual FROM fx_rates WHERE code='USD'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((rate, manual), (0.9, 0));
    }

    /// End to end through a local stub of the provider: the boot pull fills the
    /// history, and a second pull with nothing new writes nothing.
    #[tokio::test]
    async fn run_once_backfills_from_the_provider() {
        use axum::{Router, extract::{Path, Query}, routing::get};
        use std::collections::HashMap;

        async fn series(Path(range): Path<String>, Query(q): Query<HashMap<String, String>>) -> String {
            assert_eq!(q.get("base").map(String::as_str), Some("USD"));
            assert_eq!(q.get("symbols").map(String::as_str), Some("EUR"));
            let (start, end) = range.split_once("..").unwrap();
            format!(r#"{{"rates":{{"{start}":{{"EUR":0.85}},"{end}":{{"EUR":0.86}}}}}}"#)
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let stub = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/v1/:range", get(series))).await.unwrap();
        });

        let pool = db::init_memory().unwrap();
        {
            let conn = pool.get().unwrap();
            let year_ago = Utc::now().date_naive() - chrono::Duration::days(400);
            insert_tx(&conn, "a", "USD", &year_ago.to_string());
        }
        let mut config = crate::Config::default();
        config.fx.base_url = format!("http://127.0.0.1:{port}/v1");
        let state = AppState::new(config, pool.clone());

        let first = run_once(&state).await.unwrap();
        // Two or three yearly chunks, each answered with its first and last day.
        assert!(first >= 3, "history and today should both be stored, got {first}");
        // Both ends are covered now, so there is nothing left to ask for.
        assert_eq!(run_once(&state).await.unwrap(), 0);
        let conn = pool.get().unwrap();
        let today: f64 = conn
            .query_row(
                "SELECT rate_to_base FROM fx_rates WHERE code='USD' AND base_code='EUR' AND rate_date=?1",
                params![Utc::now().date_naive().to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(today, 0.86);
        stub.abort();
    }
}
