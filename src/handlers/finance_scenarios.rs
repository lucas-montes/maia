//! HTTP surface for financial scenario simulations: CRUD over named,
//! saved scenarios and a projection endpoint that runs one (saved or ad hoc)
//! forward from the real transaction history.
//!
//! The math itself lives in `crate::finance_scenarios`, DB-free and unit
//! tested there directly. Everything here is the thin SQL-in, JSON-out layer
//! around it, following the same shape `handlers::statements` uses.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;

use crate::{
    db,
    error::ErrorBody,
    finance_scenarios::{Assumptions, Baseline, TransactionForBaseline, derive_baseline, project},
    server::AppState,
};

type ApiError = (StatusCode, Json<ErrorBody>);

fn error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(ErrorBody {
            message: message.into(),
        }),
    )
}

fn db_error(e: impl std::fmt::Display) -> ApiError {
    error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Default trailing window, in months, when a caller does not name one. The
/// plan calls this out as a guess worth surfacing in the UI rather than
/// hiding, so every response that depends on it echoes back the window it
/// actually used.
const DEFAULT_WINDOW_MONTHS: u32 = 12;

// ---------------------------------------------------------------------------
// Shared DB access
// ---------------------------------------------------------------------------

fn load_transactions_for_baseline(conn: &Connection) -> rusqlite::Result<Vec<TransactionForBaseline>> {
    let mut stmt = conn.prepare(
        "SELECT type, amount_base, category, date FROM transactions WHERE deleted_at IS NULL",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TransactionForBaseline {
            type_: row.get(0)?,
            amount_base: row.get(1)?,
            category: row.get(2)?,
            date_ms: row.get(3)?,
        })
    })?;
    rows.collect()
}

/// Current net worth: every live account's opening balance, plus all income,
/// minus all expense, in the base currency. Transfers are excluded on
/// purpose — a transfer's debit and credit sides are each recorded as
/// `amount_base` on the same row (see `statements::store::create_transaction`),
/// so summing only the two ends that actually belong to this user's own
/// accounts nets to zero across the whole book; including it would double
/// count nothing but would also rely on every transfer's counterpart account
/// still being live, which is not guaranteed. Draft transactions (`is_draft`)
/// count like any other: nothing in this schema distinguishes "a true draft
/// awaiting confirmation" from "came in through statement import", and the
/// existing transaction list/history views do not filter on it either.
fn current_net_worth(conn: &Connection) -> rusqlite::Result<f64> {
    let opening: f64 = conn.query_row(
        "SELECT COALESCE(SUM(opening_balance), 0.0) FROM accounts WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let income: f64 = conn.query_row(
        "SELECT COALESCE(SUM(amount_base), 0.0) FROM transactions WHERE deleted_at IS NULL AND type='income'",
        [],
        |r| r.get(0),
    )?;
    let expense: f64 = conn.query_row(
        "SELECT COALESCE(SUM(amount_base), 0.0) FROM transactions WHERE deleted_at IS NULL AND type='expense'",
        [],
        |r| r.get(0),
    )?;
    Ok(opening + income - expense)
}

fn baseline_for(conn: &Connection, window_months: u32) -> rusqlite::Result<Baseline> {
    let transactions = load_transactions_for_baseline(conn)?;
    Ok(derive_baseline(&transactions, window_months, db::server_time_ms()))
}

// ---------------------------------------------------------------------------
// Scenario rows
// ---------------------------------------------------------------------------

fn scenario_row(conn: &Connection, id: &str) -> Result<Option<serde_json::Value>, ApiError> {
    conn.query_row(
        "SELECT id, name, assumptions_json, created_at, updated_at
         FROM finance_scenarios WHERE id=?1 AND deleted_at IS NULL",
        params![id],
        |row| {
            let assumptions_json: String = row.get(2)?;
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "assumptions": serde_json::from_str::<serde_json::Value>(&assumptions_json)
                    .unwrap_or(serde_json::Value::Null),
                "createdAt": row.get::<_, i64>(3)?,
                "updatedAt": row.get::<_, i64>(4)?,
            }))
        },
    )
    .optional()
    .map_err(db_error)
}

fn load_assumptions(conn: &Connection, id: &str) -> Result<Option<Assumptions>, ApiError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT assumptions_json FROM finance_scenarios WHERE id=?1 AND deleted_at IS NULL",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let Some(json) = json else { return Ok(None) };
    let assumptions = serde_json::from_str(&json)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, format!("stored scenario has invalid assumptions: {e}")))?;
    Ok(Some(assumptions))
}

pub async fn list_scenarios(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state.db.get().map_err(db_error)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, assumptions_json, created_at, updated_at
             FROM finance_scenarios WHERE deleted_at IS NULL ORDER BY name",
        )
        .map_err(db_error)?;
    let items = stmt
        .query_map([], |row| {
            let assumptions_json: String = row.get(2)?;
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "assumptions": serde_json::from_str::<serde_json::Value>(&assumptions_json)
                    .unwrap_or(serde_json::Value::Null),
                "createdAt": row.get::<_, i64>(3)?,
                "updatedAt": row.get::<_, i64>(4)?,
            }))
        })
        .map_err(db_error)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "items": items })))
}

#[derive(Debug, Deserialize)]
pub struct ScenarioBody {
    pub name: String,
    pub assumptions: Assumptions,
}

pub async fn create_scenario(
    State(state): State<AppState>,
    Json(body): Json<ScenarioBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.name.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "name is required"));
    }
    let conn = state.db.get().map_err(db_error)?;
    let id = uuid::Uuid::now_v7().to_string();
    let now = db::server_time_ms();
    let assumptions_json = serde_json::to_string(&body.assumptions)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    conn.execute(
        "INSERT INTO finance_scenarios (id, name, assumptions_json, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?4, NULL)",
        params![id, body.name.trim(), assumptions_json, now],
    )
    .map_err(db_error)?;
    scenario_row(&conn, &id)?
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "scenario vanished after insert"))
        .map(Json)
}

pub async fn update_scenario(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ScenarioBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.name.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "name is required"));
    }
    let conn = state.db.get().map_err(db_error)?;
    let now = db::server_time_ms();
    let assumptions_json = serde_json::to_string(&body.assumptions)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let updated = conn
        .execute(
            "UPDATE finance_scenarios SET name=?1, assumptions_json=?2, updated_at=?3
             WHERE id=?4 AND deleted_at IS NULL",
            params![body.name.trim(), assumptions_json, now, id],
        )
        .map_err(db_error)?;
    if updated == 0 {
        return Err(error(StatusCode::NOT_FOUND, "scenario not found"));
    }
    scenario_row(&conn, &id)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "scenario not found"))
        .map(Json)
}

pub async fn delete_scenario(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state.db.get().map_err(db_error)?;
    let now = db::server_time_ms();
    let deleted = conn
        .execute(
            "UPDATE finance_scenarios SET deleted_at=?1, updated_at=?1 WHERE id=?2 AND deleted_at IS NULL",
            params![now, id],
        )
        .map_err(db_error)?;
    if deleted == 0 {
        return Err(error(StatusCode::NOT_FOUND, "scenario not found"));
    }
    Ok(Json(serde_json::json!({ "id": id })))
}

#[derive(Debug, Deserialize)]
pub struct BaselineQuery {
    #[serde(default, rename = "windowMonths")]
    pub window_months: Option<u32>,
}

/// The baseline plus the current net worth, for the "no changes" reference
/// line and for showing the user what window their numbers came from.
pub async fn get_baseline(
    State(state): State<AppState>,
    Query(query): Query<BaselineQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state.db.get().map_err(db_error)?;
    let window_months = query.window_months.unwrap_or(DEFAULT_WINDOW_MONTHS).max(1);
    let baseline = baseline_for(&conn, window_months).map_err(db_error)?;
    let net_worth = current_net_worth(&conn).map_err(db_error)?;
    Ok(Json(serde_json::json!({
        "windowMonths": baseline.window_months,
        "monthlyIncome": baseline.monthly_income,
        "categoryMonthly": baseline
            .category_monthly
            .iter()
            .map(|(category, amount)| serde_json::json!({ "category": category, "monthlyAmount": amount }))
            .collect::<Vec<_>>(),
        "totalMonthlyExpense": baseline.total_monthly_expense(),
        "currentNetWorth": net_worth,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBody {
    /// Project a saved scenario by id.
    #[serde(default)]
    pub scenario_id: Option<String>,
    /// Project an unsaved draft's assumptions instead. Exactly one of
    /// `scenario_id` / `assumptions` must be given.
    #[serde(default)]
    pub assumptions: Option<Assumptions>,
    #[serde(default)]
    pub window_months: Option<u32>,
}

pub async fn project_scenario(
    State(state): State<AppState>,
    Json(body): Json<ProjectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state.db.get().map_err(db_error)?;

    let assumptions = match (&body.scenario_id, body.assumptions) {
        (Some(id), None) => load_assumptions(&conn, id)?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "scenario not found"))?,
        (None, Some(assumptions)) => assumptions,
        (Some(_), Some(_)) => {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "pass either scenarioId or assumptions, not both",
            ));
        }
        (None, None) => {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "pass either scenarioId or assumptions",
            ));
        }
    };

    let window_months = body.window_months.unwrap_or(DEFAULT_WINDOW_MONTHS).max(1);
    let baseline = baseline_for(&conn, window_months).map_err(db_error)?;
    let net_worth = current_net_worth(&conn).map_err(db_error)?;
    let projection = project(&baseline, &assumptions, net_worth);

    Ok(Json(serde_json::json!({
        "baseline": {
            "windowMonths": baseline.window_months,
            "monthlyIncome": baseline.monthly_income,
            "totalMonthlyExpense": baseline.total_monthly_expense(),
            "currentNetWorth": net_worth,
        },
        "projection": projection,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_account(conn: &Connection, id: &str, opening_balance: f64) {
        conn.execute(
            "INSERT INTO accounts (id, name, type, opening_balance, created_at, updated_at, deleted_at)
             VALUES (?1, ?1, 'checking', ?2, 0, 0, NULL)",
            params![id, opening_balance],
        )
        .unwrap();
    }

    fn seed_transaction(
        conn: &Connection,
        id: &str,
        type_: &str,
        amount_base: f64,
        account_id: &str,
        category: Option<&str>,
        date_ms: i64,
    ) {
        conn.execute(
            "INSERT INTO transactions
                (id, type, amount, currency_code, amount_base, rate_used, account_id,
                 category, date, is_draft, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, 'EUR', ?3, 1.0, ?4, ?5, ?6, 0, ?6, ?6, NULL)",
            params![id, type_, amount_base, account_id, category, date_ms],
        )
        .unwrap();
    }

    #[test]
    fn baseline_and_net_worth_read_straight_off_seeded_rows() {
        let pool = crate::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_account(&conn, "acc1", 1000.0);
        let now = db::server_time_ms();
        seed_transaction(&conn, "t1", "income", 500.0, "acc1", None, now);
        seed_transaction(&conn, "t2", "expense", 200.0, "acc1", Some("rent"), now);
        // A soft-deleted transaction must not count.
        seed_transaction(&conn, "t3", "expense", 9_999.0, "acc1", Some("rent"), now);
        conn.execute("UPDATE transactions SET deleted_at=?1 WHERE id='t3'", params![now])
            .unwrap();

        let net_worth = current_net_worth(&conn).unwrap();
        assert!((net_worth - (1000.0 + 500.0 - 200.0)).abs() < 1e-9);

        let baseline = baseline_for(&conn, 1).unwrap();
        assert!((baseline.monthly_income - 500.0).abs() < 1e-9);
        assert!((baseline.category_monthly["rent"] - 200.0).abs() < 1e-9);
    }

    #[test]
    fn scenario_round_trips_through_create_update_delete() {
        let pool = crate::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        let assumptions = Assumptions {
            horizon_months: 12,
            income_delta_monthly: 0.0,
            category_overrides: Vec::new(),
            inflation_annual_pct: 2.0,
            investment_return_annual_pct: 5.0,
        };
        let now = db::server_time_ms();
        let assumptions_json = serde_json::to_string(&assumptions).unwrap();
        conn.execute(
            "INSERT INTO finance_scenarios (id, name, assumptions_json, created_at, updated_at, deleted_at)
             VALUES ('s1', 'Stay', ?1, ?2, ?2, NULL)",
            params![assumptions_json, now],
        )
        .unwrap();

        let row = scenario_row(&conn, "s1").unwrap().unwrap();
        assert_eq!(row["name"], "Stay");
        assert_eq!(row["assumptions"]["horizonMonths"], 12);

        let loaded = load_assumptions(&conn, "s1").unwrap().unwrap();
        assert_eq!(loaded, assumptions);

        conn.execute(
            "UPDATE finance_scenarios SET deleted_at=?1 WHERE id='s1'",
            params![now],
        )
        .unwrap();
        assert!(scenario_row(&conn, "s1").unwrap().is_none());
        assert!(load_assumptions(&conn, "s1").unwrap().is_none());
    }
}
