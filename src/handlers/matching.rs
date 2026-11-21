//! The receipt's page and every action on it (`crate::matching`).
//!
//! `GET /receipt-pictures/:id/matching` is what the desktop's receipt page
//! shows: the reading, the store and what to do about it, the transaction and
//! its candidates, each line and its ingredient, and every change the receipt
//! made, each with what it points at so the page can open it. The other
//! routes are the page's actions; each runs in one database transaction and
//! records its effect, so it can be undone from the same page.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    db,
    error::ErrorBody,
    handlers::attempts_json,
    matching::{self, MANUAL, effects, products, stores, transactions},
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

fn internal(e: impl std::fmt::Display) -> ApiError {
    error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn bad(e: impl std::fmt::Display) -> ApiError {
    error(StatusCode::BAD_REQUEST, e.to_string())
}

/// Who an accept says decided: the user, or the user taking the AI's proposal.
pub fn decided_by(source: Option<&str>) -> Result<&'static str, ApiError> {
    match source {
        None | Some("manual") => Ok(MANUAL),
        Some("ai") => Ok(matching::AI),
        Some(other) => Err(bad(format!("source must be manual or ai, not {other}"))),
    }
}

/// The model to file an accept under: only an AI accept has one, and a blank
/// value is none.
pub fn proposed_by(source: &str, model: Option<&str>) -> Option<String> {
    (source == matching::AI)
        .then(|| model.map(str::trim).filter(|m| !m.is_empty()).map(str::to_owned))
        .flatten()
}

fn ensure_receipt(conn: &rusqlite::Connection, id: &str) -> Result<(), ApiError> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM receipts WHERE id = ?1 AND deleted_at IS NULL)",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if exists {
        Ok(())
    } else {
        Err(error(
            StatusCode::NOT_FOUND,
            format!("receipt {id} not found"),
        ))
    }
}

/// An effect with what its target is called now, and whether it still exists.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectView {
    #[serde(flatten)]
    pub effect: effects::Effect,
    pub target_label: Option<String>,
    pub target_live: bool,
}

fn describe(conn: &rusqlite::Connection, e: effects::Effect) -> rusqlite::Result<EffectView> {
    use rusqlite::OptionalExtension;
    let (label, live): (Option<String>, bool) = match e.target_table.as_str() {
        "stores" => conn
            .query_row(
                "SELECT name, deleted_at IS NULL FROM stores WHERE id = ?1",
                rusqlite::params![e.target_id],
                |r| Ok((Some(r.get(0)?), r.get(1)?)),
            )
            .optional()?
            .unwrap_or((None, false)),
        "transactions" => conn
            .query_row(
                "SELECT COALESCE(NULLIF(note, ''), category, type) || ' · ' || printf('%.2f', amount) || ' ' || currency_code,
                        deleted_at IS NULL
                 FROM transactions WHERE id = ?1",
                rusqlite::params![e.target_id],
                |r| Ok((Some(r.get(0)?), r.get(1)?)),
            )
            .optional()?
            .unwrap_or((None, false)),
        "ingredient_prices" => conn
            .query_row(
                "SELECT i.name || ' · ' || printf('%.2f', p.price) || ' ' || p.currency_code, p.deleted_at IS NULL
                 FROM ingredient_prices p JOIN ingredients i ON i.id = p.ingredient_id WHERE p.id = ?1",
                rusqlite::params![e.target_id],
                |r| Ok((Some(r.get(0)?), r.get(1)?)),
            )
            .optional()?
            .unwrap_or((None, false)),
        "store_aliases" => (Some(e.target_id.clone()), true),
        "product_aliases" => (
            e.after.get("printed").and_then(Value::as_str).map(str::to_string),
            true,
        ),
        _ => (None, false),
    };
    Ok(EffectView {
        effect: e,
        target_label: label,
        target_live: live,
    })
}

/// `GET /receipt-pictures/:id/matching`
pub async fn get_matching(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let conn = state.db.get().map_err(internal)?;
    ensure_receipt(&conn, &id)?;
    let reading = matching::reading(&conn, &id).map_err(internal)?;
    let (store_id, store_source) = stores::current(&conn, &id)
        .map_err(internal)?
        .unwrap_or((None, None));
    let store = match &store_id {
        Some(s) => stores::load(&conn, s).map_err(internal)?,
        None => None,
    };
    let (transaction_id, link_source, read_by): (Option<String>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT transaction_id, link_source, parsed_model FROM receipts WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(internal)?;
    let (recommendations, candidates, lines) = match &reading {
        Some(r) => (
            stores::recommendations(&conn, &id, r).map_err(internal)?,
            transactions::candidates(&conn, &id, r)
                .map_err(internal)?
                .into_iter()
                .take(15)
                .collect(),
            products::lines(&conn, &id, r).map_err(internal)?,
        ),
        None => (vec![], vec![], vec![]),
    };
    let effects: Vec<EffectView> = effects::for_receipt(&conn, &id)
        .map_err(internal)?
        .into_iter()
        .map(|e| describe(&conn, e))
        .collect::<Result<_, _>>()
        .map_err(internal)?;
    Ok(Json(json!({
        "receiptId": id,
        "read": reading.is_some(),
        // The model that read the image, `"<id> (<model>)"`; null for a
        // reading older than the column.
        "readBy": read_by,
        "store": store,
        "storeSource": store_source,
        "recommendations": recommendations,
        "transactionId": transaction_id,
        "linkSource": link_source,
        "transactionCandidates": candidates,
        "lines": lines,
        "effects": effects,
    })))
}

/// Runs a page action in one transaction, then answers with the fresh page.
async fn act<F>(state: AppState, id: String, model: Option<String>, action: F) -> Result<Json<Value>, ApiError>
where
    F: FnOnce(&rusqlite::Connection, i64) -> Result<(), ApiError>,
{
    {
        let conn = state.db.get().map_err(internal)?;
        ensure_receipt(&conn, &id)?;
        let tx = conn.unchecked_transaction().map_err(internal)?;
        let now = db::server_time_ms();
        action(&tx, now)?;
        if let Some(model) = model.as_deref() {
            effects::tag_model(&tx, &id, now, model).map_err(internal)?;
        }
        tx.commit().map_err(internal)?;
    }
    get_matching(State(state), Path(id)).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreChoice {
    pub store_id: String,
    #[serde(default)]
    pub source: Option<String>,
    /// The model that proposed it, as `assist` returned it. Kept only with
    /// `source: "ai"`.
    #[serde(default)]
    pub model: Option<String>,
}

/// `PUT /receipt-pictures/:id/store` — this receipt is from that store.
pub async fn put_store(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<StoreChoice>,
) -> Result<Json<Value>, ApiError> {
    let source = decided_by(body.source.as_deref())?;
    let model = proposed_by(source, body.model.as_deref());
    let receipt = id.clone();
    act(state, id, model, move |conn, now| {
        let reading = matching::reading(conn, &receipt).map_err(internal)?;
        let issuer = reading.as_ref().map(stores::Issuer::of);
        stores::assign(
            conn,
            &receipt,
            body.store_id.trim(),
            issuer.as_ref(),
            source,
            now,
        )
        .map_err(bad)?;
        // A known store can price the lines it has learned.
        if let Some(r) = &reading {
            products::resolve(conn, &receipt, r, now).map_err(internal)?;
        }
        Ok(())
    })
    .await
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewStore {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// `POST /receipt-pictures/:id/store` — a new store, built from the reading.
pub async fn post_store(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<NewStore>,
) -> Result<Json<Value>, ApiError> {
    let source = decided_by(body.source.as_deref())?;
    let model = proposed_by(source, body.model.as_deref());
    let receipt = id.clone();
    act(state, id, model, move |conn, now| {
        let reading = matching::reading(conn, &receipt)
            .map_err(internal)?
            .ok_or_else(|| bad("the receipt has not been read"))?;
        stores::create(
            conn,
            &receipt,
            &stores::Issuer::of(&reading),
            body.name.as_deref(),
            source,
            now,
        )
        .map_err(bad)?;
        Ok(())
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rename {
    pub name: String,
    #[serde(default)]
    pub receipt_id: Option<String>,
}

/// `PUT /stores/:id/name` — renames a store; from a receipt's page, the rename
/// is one of that receipt's effects.
pub async fn put_store_name(
    State(state): State<AppState>,
    Path(store_id): Path<String>,
    Json(body): Json<Rename>,
) -> Result<Json<Value>, ApiError> {
    let conn = state.db.get().map_err(internal)?;
    let tx = conn.unchecked_transaction().map_err(internal)?;
    stores::rename(
        &tx,
        &store_id,
        &body.name,
        body.receipt_id.as_deref(),
        MANUAL,
        db::server_time_ms(),
    )
    .map_err(bad)?;
    let store = stores::load(&tx, &store_id).map_err(internal)?;
    tx.commit().map_err(internal)?;
    Ok(Json(json!({ "store": store })))
}

/// `PUT /stores/:id/details` — what identifies a store, edited by hand.
/// A key left out is left alone; an empty string clears the column.
pub async fn put_store_details(
    State(state): State<AppState>,
    Path(store_id): Path<String>,
    Json(body): Json<serde_json::Map<String, Value>>,
) -> Result<Json<Value>, ApiError> {
    let mut details = Vec::new();
    for (key, value) in body {
        // The desktop sends camelCase; the columns are snake_case.
        let column: String = key
            .chars()
            .flat_map(|c| {
                if c.is_ascii_uppercase() {
                    vec!['_', c.to_ascii_lowercase()]
                } else {
                    vec![c]
                }
            })
            .collect();
        let value = match value {
            Value::Null => Some(String::new()),
            Value::String(s) => Some(s),
            other => return Err(bad(format!("{key} must be text, not {other}"))),
        };
        details.push((column, value));
    }
    let conn = state.db.get().map_err(internal)?;
    let tx = conn.unchecked_transaction().map_err(internal)?;
    stores::edit_details(&tx, &store_id, &details).map_err(bad)?;
    let store = stores::load(&tx, &store_id).map_err(internal)?;
    tx.commit().map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            error(
                StatusCode::CONFLICT,
                "another store already has that tax number",
            )
        } else {
            internal(e)
        }
    })?;
    Ok(Json(json!({ "store": store })))
}

/// `POST /receipt-pictures/:id/effects/:effect/undo`
pub async fn undo_effect(
    State(state): State<AppState>,
    Path((id, effect_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let receipt = id.clone();
    act(state, id, None, move |conn, now| {
        let effect = effects::load(conn, &effect_id)
            .map_err(internal)?
            .filter(|e| e.receipt_id == receipt)
            .ok_or_else(|| {
                error(
                    StatusCode::NOT_FOUND,
                    format!("effect {effect_id} not found"),
                )
            })?;
        effects::undo(conn, &effect.id, now).map_err(|e| match e {
            effects::UndoError::NotFound(_) => error(StatusCode::NOT_FOUND, e.to_string()),
            effects::UndoError::Db(_) => internal(e),
            _ => error(StatusCode::CONFLICT, e.to_string()),
        })?;
        Ok(())
    })
    .await
}

/// `POST /receipt-pictures/:id/match` — runs every rule over one receipt.
pub async fn match_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let receipt = id.clone();
    act(state, id, None, move |conn, now| {
        matching::run(conn, &receipt, now).map_err(internal)?;
        Ok(())
    })
    .await
}

/// `POST /receipt-pictures/match` — the backfill: every read receipt, again.
pub async fn match_all(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = state.db.get().map_err(internal)?;
    let passes = matching::run_all(&conn, db::server_time_ms()).map_err(internal)?;
    Ok(Json(json!({ "receipts": passes })))
}

/// `GET /stores/:id/effects` — the receipts that made or changed a store, for
/// the "from receipt" back link. Also answers for `transactions` and
/// `ingredient_prices` through `GET /receipt-effects?table=…&id=…`.
pub async fn get_target_effects(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let (Some(table), Some(id)) = (q.get("table"), q.get("id")) else {
        return Err(bad("table and id are required"));
    };
    let conn = state.db.get().map_err(internal)?;
    let list = effects::for_target(&conn, table, id).map_err(internal)?;
    Ok(Json(json!({ "effects": list })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineChoice {
    pub ingredient_id: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub package_grams: Option<f64>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// `PUT /receipt-pictures/:id/lines/:line` — this line is that ingredient:
/// records the price and teaches the store the printed name.
pub async fn put_line(
    State(state): State<AppState>,
    Path((id, line)): Path<(String, usize)>,
    Json(body): Json<LineChoice>,
) -> Result<Json<Value>, ApiError> {
    if body.package_grams.is_some_and(|g| !(g > 0.0)) {
        return Err(bad("a package weighs more than zero grams"));
    }
    let source = decided_by(body.source.as_deref())?;
    let model = proposed_by(source, body.model.as_deref());
    let receipt = id.clone();
    act(state, id, model, move |conn, now| {
        products::match_manually(
            conn,
            &receipt,
            line,
            body.ingredient_id.trim(),
            body.package_grams,
            body.source_url
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty()),
            source,
            now,
        )
        .map_err(bad)?;
        Ok(())
    })
    .await
}

/// `DELETE /receipt-pictures/:id/lines/:line` — the line is not that
/// ingredient: drops its price and forgets the name.
pub async fn delete_line(
    State(state): State<AppState>,
    Path((id, line)): Path<(String, usize)>,
) -> Result<Json<Value>, ApiError> {
    let receipt = id.clone();
    act(state, id, None, move |conn, now| {
        products::unmatch(conn, &receipt, line, now).map_err(|e| match e {
            effects::UndoError::NotFound(_) => error(StatusCode::NOT_FOUND, e.to_string()),
            effects::UndoError::Db(_) => internal(e),
            _ => error(StatusCode::CONFLICT, e.to_string()),
        })
    })
    .await
}

/// `POST /receipt-pictures/:id/assist` — asks the model what the rules left
/// open. Returns proposals only; accepting one goes through the endpoint a
/// manual choice uses, with `source: "ai"`.
pub async fn assist(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let Some(models) = state.matching.clone() else {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "no model is assigned to receipt matching: add one in Settings",
        ));
    };
    let (prompt, known) = {
        let conn = state.db.get().map_err(internal)?;
        ensure_receipt(&conn, &id)?;
        let reading = matching::reading(&conn, &id)
            .map_err(internal)?
            .ok_or_else(|| bad("the receipt has not been read"))?;
        matching::assist::prompt(&conn, &id, &reading).map_err(internal)?
    };
    let generated = models
        .generate_json(matching::assist::SYSTEM, &prompt, &matching::assist::schema())
        .await
        .map_err(|e| error(StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(json!({
        "proposals": matching::assist::validate(&generated.value, &known),
        // Sent back with an accept, so the change is filed under it.
        "model": generated.model.to_string(),
        "attempts": attempts_json(&generated.attempts),
    })))
}
