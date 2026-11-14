//! Desktop-only: what a securities account held before its trades.
//!
//! Positions are derived from the booked trades (see [`crate::trades`]), but an
//! export rarely reaches back to the first purchase, so a sale can take shares
//! no booked trade bought. The opening position is how the user says what was
//! there. Like the account roots it is server-only state, off the phone sync
//! surface, so it has its own endpoint rather than a column on a synced table.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rusqlite::OptionalExtension;
use serde::Deserialize;

use crate::{
    db,
    error::ErrorBody,
    server::AppState,
    trades::{self, OpeningPosition},
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

fn internal(e: impl std::fmt::Display) -> ApiError {
    error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningRequest {
    pub quantity: f64,
    /// The total paid for the shares, not a price per share.
    pub cost_basis: f64,
    /// Epoch milliseconds: the day the holding is stated at the end of.
    pub as_of: i64,
}

/// Symbols are compared as the broker writes them, which is upper case.
fn normalise_symbol(symbol: &str) -> Result<String, ApiError> {
    let symbol = symbol.trim().to_uppercase();
    if symbol.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "a position needs a symbol"));
    }
    Ok(symbol)
}

/// The account must be a live securities account: an opening position says
/// what shares an account held, and only a securities account holds shares.
fn ensure_securities_account(conn: &rusqlite::Connection, id: &str) -> Result<(), ApiError> {
    let found: Option<(String, String)> = conn
        .query_row(
            "SELECT name, type FROM accounts WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(internal)?;
    match found {
        None => Err(error(
            StatusCode::NOT_FOUND,
            format!("account {id} not found"),
        )),
        Some((_, type_)) if type_ == "securities" => Ok(()),
        Some((name, type_)) => Err(error(
            StatusCode::BAD_REQUEST,
            format!("{name} is a {type_} account; only a securities account holds shares"),
        )),
    }
}

/// `PUT /accounts/:id/opening-positions/:symbol` — states what the account
/// held of the symbol at the end of `asOf`, and what it cost. Replaces what
/// was stated before.
pub async fn put_opening_position(
    State(state): State<AppState>,
    Path((id, symbol)): Path<(String, String)>,
    Json(request): Json<OpeningRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let symbol = normalise_symbol(&symbol)?;
    if !request.quantity.is_finite() || request.quantity <= 0.0 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "an opening position holds more than zero shares",
        ));
    }
    if !request.cost_basis.is_finite() || request.cost_basis < 0.0 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "a cost basis is what the shares cost, zero or more",
        ));
    }
    let conn = state.db.get().map_err(internal)?;
    ensure_securities_account(&conn, &id)?;
    let opening = OpeningPosition {
        quantity: request.quantity,
        cost_basis: request.cost_basis,
        as_of: request.as_of,
    };
    trades::set_opening_position(&conn, &id, &symbol, &opening, db::server_time_ms())
        .map_err(internal)?;
    Ok(Json(serde_json::json!({
        "accountId": id,
        "symbol": symbol,
        "opening": opening,
    })))
}

/// `DELETE /accounts/:id/opening-positions/:symbol` — forgets a stated
/// opening position; the position goes back to the trades alone.
pub async fn delete_opening_position(
    State(state): State<AppState>,
    Path((id, symbol)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let symbol = normalise_symbol(&symbol)?;
    let conn = state.db.get().map_err(internal)?;
    let removed = trades::clear_opening_position(&conn, &id, &symbol).map_err(internal)?;
    if !removed {
        return Err(error(
            StatusCode::NOT_FOUND,
            format!("no opening position of {symbol} on account {id}"),
        ));
    }
    Ok(Json(
        serde_json::json!({ "accountId": id, "symbol": symbol }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn state() -> AppState {
        let state = AppState::new(
            Config::from_values(serde_json::Map::new(), serde_json::Map::new()),
            crate::db::init_memory().unwrap(),
        );
        let conn = state.db.get().unwrap();
        for (id, name, type_) in [
            ("cash", "Firstrade", "cash"),
            ("sec", "Firstrade Securities", "securities"),
        ] {
            conn.execute(
                "INSERT INTO accounts (id, name, type, opening_balance, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 0.0, 1, 1)",
                rusqlite::params![id, name, type_],
            )
            .unwrap();
        }
        drop(conn);
        state
    }

    async fn put(
        state: &AppState,
        id: &str,
        symbol: &str,
        quantity: f64,
    ) -> Result<Json<serde_json::Value>, ApiError> {
        put_opening_position(
            State(state.clone()),
            Path((id.to_string(), symbol.to_string())),
            Json(OpeningRequest {
                quantity,
                cost_basis: 2_000.0,
                as_of: 0,
            }),
        )
        .await
    }

    #[tokio::test]
    async fn an_opening_position_is_stated_replaced_and_forgotten() {
        let state = state();
        let _ = put(&state, "sec", " intc ", 100.0).await.unwrap();
        let _ = put(&state, "sec", "INTC", 120.0).await.unwrap();
        let conn = state.db.get().unwrap();
        let positions = trades::positions(&conn).unwrap();
        assert_eq!(positions.len(), 1, "the symbol is the broker's, upper case");
        assert_eq!(
            positions[0].quantity, 120.0,
            "the second statement replaces the first"
        );
        drop(conn);

        let _ = delete_opening_position(State(state.clone()), Path(("sec".into(), "intc".into())))
            .await
            .unwrap();
        let conn = state.db.get().unwrap();
        assert!(trades::positions(&conn).unwrap().is_empty());
        drop(conn);
        let again =
            delete_opening_position(State(state.clone()), Path(("sec".into(), "INTC".into())))
                .await;
        assert_eq!(again.unwrap_err().0, StatusCode::NOT_FOUND);
    }

    /// Only a securities account holds shares, and a holding has some.
    #[tokio::test]
    async fn an_opening_position_needs_a_securities_account_and_shares() {
        let state = state();
        assert_eq!(
            put(&state, "cash", "INTC", 100.0).await.unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            put(&state, "gone", "INTC", 100.0).await.unwrap_err().0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            put(&state, "sec", "INTC", 0.0).await.unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            put(&state, "sec", "  ", 1.0).await.unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }
}
