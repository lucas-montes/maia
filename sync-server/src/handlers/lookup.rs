use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{db, openfoodfacts, server::AppState};

#[derive(Deserialize)]
pub struct LookupQuery {
    pub barcode: String,
}

#[derive(Deserialize)]
pub struct ImportBody {
    pub barcode: String,
}

#[derive(Serialize)]
struct LookupResponse {
    source: &'static str,
    barcode: String,
    #[serde(rename = "openfoodUrl")]
    openfood_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ingredient: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    draft: Option<openfoodfacts::IngredientDraft>,
}

fn bad_request(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message": msg})))
}

fn upstream_error() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(serde_json::json!({"message": "OpenFoodFacts unavailable"})),
    )
}

fn read_ingredient_row(
    id: &str,
    name: &str,
    cal: f64,
    prot: f64,
    carb: f64,
    fat: f64,
    sod: Option<f64>,
    fiber: Option<f64>,
    sugar: Option<f64>,
    is_arch: i64,
    brand: Option<String>,
    barcode: Option<String>,
    created_at: i64,
    updated_at: i64,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "caloriesPer100g": cal,
        "proteinPer100g": prot,
        "carbsPer100g": carb,
        "fatPer100g": fat,
        "sodiumPer100g": sod,
        "fiberPer100g": fiber,
        "sugarPer100g": sugar,
        "isArchived": is_arch != 0,
        "brand": brand,
        "barcode": barcode,
        "created_at": created_at,
        "updated_at": updated_at,
        "pictures": [],
    })
}

fn find_local(conn: &rusqlite::Connection, barcode: &str) -> Option<serde_json::Value> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, sodium_per100g, fiber_per100g, sugar_per100g, is_archived, brand, barcode, created_at, updated_at FROM ingredients WHERE barcode = ?1 AND deleted_at IS NULL LIMIT 1",
        )
        .ok()?;
    let row: rusqlite::Result<(String, String, f64, f64, f64, f64, Option<f64>, Option<f64>, Option<f64>, i64, Option<String>, Option<String>, i64, i64)> =
        stmt.query_row([barcode], |r| {
            Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?,
                r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?,
                r.get(11)?, r.get(12)?, r.get(13)?,
            ))
        });
    row.ok().map(
        |(id, name, cal, prot, carb, fat, sod, fiber, sugar, arch, brand, bc, ca, ua)| {
            read_ingredient_row(
                &id, &name, cal, prot, carb, fat, sod, fiber, sugar, arch, brand, bc, ca, ua,
            )
        },
    )
}

pub async fn lookup_ingredient(
    State(state): State<AppState>,
    Query(q): Query<LookupQuery>,
) -> impl IntoResponse {
    let barcode = q.barcode.trim().to_string();
    if !openfoodfacts::valid_barcode(&barcode) {
        return bad_request("invalid barcode").into_response();
    }
    let openfood_url = openfoodfacts::openfood_url(&barcode);
    {
        let conn = state.db.lock().unwrap();
        if let Some(item) = find_local(&conn, &barcode) {
            return (
                StatusCode::OK,
                Json(serde_json::to_value(LookupResponse {
                    source: "local",
                    barcode: barcode.clone(),
                    openfood_url,
                    ingredient: Some(item),
                    draft: None,
                }).unwrap()),
            )
                .into_response();
        }
    }
    match openfoodfacts::fetch_product(&barcode).await {
        Ok(Some(draft)) => (
            StatusCode::OK,
            Json(serde_json::to_value(LookupResponse {
                source: "openfoodfacts",
                barcode: barcode.clone(),
                openfood_url,
                ingredient: None,
                draft: Some(draft),
            }).unwrap()),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"source": "none", "barcode": barcode, "openfoodUrl": openfood_url})),
        )
            .into_response(),
        Err(_) => upstream_error().into_response(),
    }
}

pub async fn import_from_barcode(
    State(state): State<AppState>,
    Json(body): Json<ImportBody>,
) -> impl IntoResponse {
    let barcode = body.barcode.trim().to_string();
    if !openfoodfacts::valid_barcode(&barcode) {
        return bad_request("invalid barcode").into_response();
    }
    {
        let conn = state.db.lock().unwrap();
        if let Some(item) = find_local(&conn, &barcode) {
            return (StatusCode::OK, Json(item)).into_response();
        }
    }
    let draft = match openfoodfacts::fetch_product(&barcode).await {
        Ok(Some(d)) => d,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"message": "product not found"})),
            )
                .into_response();
        }
        Err(_) => return upstream_error().into_response(),
    };
    let id = uuid::Uuid::now_v7().to_string();
    let now = db::server_time_ms();
    {
        let conn = state.db.lock().unwrap();
        let res = conn.execute(
            "INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, sodium_per100g, fiber_per100g, sugar_per100g, brand, barcode, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            rusqlite::params![
                id, draft.name, draft.calories_per100g, draft.protein_per100g,
                draft.carbs_per100g, draft.fat_per100g, draft.sodium_per100g,
                draft.fiber_per100g, draft.sugar_per100g, draft.brand,
                draft.barcode, now, now,
            ],
        );
        if let Err(e) = res {
            // Race: another import won; return existing row.
            if e.to_string().contains("UNIQUE") {
                if let Some(item) = find_local(&conn, &barcode) {
                    return (StatusCode::OK, Json(item)).into_response();
                }
            }
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"message": "insert failed"})),
            )
                .into_response();
        }
    }
    let conn = state.db.lock().unwrap();
    match find_local(&conn, &barcode) {
        Some(item) => (StatusCode::OK, Json(item)).into_response(),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"message": "insert failed"})),
        )
            .into_response(),
    }
}
