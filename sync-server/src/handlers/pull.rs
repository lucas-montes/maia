use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::db;
use crate::server::AppState;

#[derive(Deserialize)]
pub struct SinceQuery {
    pub since: Option<i64>,
}

#[derive(Deserialize)]
pub struct FxRatesQuery {
    pub since: Option<i64>,
    pub base: String,
    pub date: String,
}

#[derive(Serialize)]
pub struct PullExercisesResponse {
    pub server_time: i64,
    pub items: Vec<Exercise>,
    pub deleted: Vec<String>,
}

#[derive(Serialize)]
pub struct Exercise {
    pub id: String,
    pub name: String,
    #[serde(rename = "exerciseType")]
    pub exercise_type: Option<String>,
    #[serde(rename = "bodyPart")]
    pub body_part: Option<String>,
    pub equipment: Option<String>,
    #[serde(rename = "primaryMuscle")]
    pub primary_muscle: Option<String>,
    #[serde(rename = "secondaryMuscle")]
    pub secondary_muscle: Option<String>,
    pub instructions: Option<Vec<String>>,
    pub tips: Option<Vec<String>>,
    pub faqs: Option<String>,
    pub keywords: Option<Vec<String>>,
    #[serde(rename = "similarTo")]
    pub similar_to: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(rename = "isCanonical")]
    pub is_canonical: bool,
    #[serde(rename = "created_at")]
    pub created_at: i64,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<i64>,
    #[serde(rename = "hasImage")]
    pub has_image: bool,
    #[serde(rename = "hasVideo")]
    pub has_video: bool,
}

#[derive(Serialize)]
pub struct PullIngredientsResponse {
    pub server_time: i64,
    pub stores: Vec<Store>,
    pub items: Vec<Ingredient>,
    pub deleted: Vec<String>,
}

#[derive(Serialize)]
pub struct Store {
    pub id: String,
    pub name: String,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<i64>,
}

#[derive(Serialize)]
pub struct Ingredient {
    pub id: String,
    pub name: String,
    #[serde(rename = "caloriesPer100g")]
    pub calories_per100g: f64,
    #[serde(rename = "proteinPer100g")]
    pub protein_per100g: f64,
    #[serde(rename = "carbsPer100g")]
    pub carbs_per100g: f64,
    #[serde(rename = "fatPer100g")]
    pub fat_per100g: f64,
    #[serde(rename = "sodiumPer100g")]
    pub sodium_per100g: Option<f64>,
    #[serde(rename = "fiberPer100g")]
    pub fiber_per100g: Option<f64>,
    #[serde(rename = "sugarPer100g")]
    pub sugar_per100g: Option<f64>,
    #[serde(rename = "isArchived")]
    pub is_archived: bool,
    pub brand: Option<String>,
    pub barcode: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<i64>,
    #[serde(rename = "created_at")]
    pub created_at: i64,
    pub pictures: Vec<IngredientPicture>,
    pub prices: Vec<IngredientPrice>,
}

#[derive(Serialize)]
pub struct IngredientPicture {
    pub id: String,
    #[serde(rename = "ingredientId")]
    pub ingredient_id: String,
    #[serde(rename = "imagePath")]
    pub image_path: String,
    #[serde(rename = "sortOrder")]
    pub sort_order: i64,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Serialize)]
pub struct IngredientPrice {
    pub id: String,
    #[serde(rename = "ingredientId")]
    pub ingredient_id: String,
    #[serde(rename = "storeId")]
    pub store_id: String,
    pub price: f64,
    #[serde(rename = "currencyCode")]
    pub currency_code: String,
    #[serde(rename = "packageGrams")]
    pub package_grams: Option<f64>,
    #[serde(rename = "recordedAt")]
    pub recorded_at: i64,
}

#[derive(Serialize)]
pub struct PullFxRatesResponse {
    pub server_time: i64,
    pub items: Vec<FxRate>,
}

#[derive(Serialize)]
pub struct FxRate {
    pub code: String,
    #[serde(rename = "rateToBase")]
    pub rate_to_base: f64,
    pub date: String,
    #[serde(rename = "updated_at")]
    pub updated_at: i64,
}

pub async fn pull_exercises(
    State(state): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    let since = q.since.unwrap_or(0);
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();

    let mut stmt = conn
        .prepare(
            "SELECT id, name, exercise_type, body_part, equipment, primary_muscle, secondary_muscle, instructions, tips, faqs, keywords, similar_to, tags, is_canonical, created_at, updated_at, image_path, video_path FROM exercises WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at",
        )
        .unwrap();
    let items = stmt
        .query_map([since], |row| {
            let instructions_str: Option<String> = row.get(7)?;
            let tips_str: Option<String> = row.get(8)?;
            let keywords_str: Option<String> = row.get(10)?;
            let tags_str: Option<String> = row.get(12)?;
            let image_path: Option<String> = row.get(16)?;
            let video_path: Option<String> = row.get(17)?;
            Ok(Exercise {
                id: row.get(0)?,
                name: row.get(1)?,
                exercise_type: row.get(2)?,
                body_part: row.get(3)?,
                equipment: row.get(4)?,
                primary_muscle: row.get(5)?,
                secondary_muscle: row.get(6)?,
                instructions: instructions_str.and_then(|s| serde_json::from_str(&s).ok()),
                tips: tips_str.and_then(|s| serde_json::from_str(&s).ok()),
                faqs: row.get(9)?,
                keywords: keywords_str.and_then(|s| serde_json::from_str(&s).ok()),
                similar_to: row.get(11)?,
                tags: tags_str.and_then(|s| serde_json::from_str(&s).ok()),
                is_canonical: row.get::<_, i64>(13)? != 0,
                created_at: row.get(14)?,
                updated_at: row.get(15)?,
                has_image: image_path.is_some(),
                has_video: video_path.is_some(),
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();

    let mut del_stmt = conn
        .prepare("SELECT id FROM exercises WHERE deleted_at IS NOT NULL AND deleted_at > ?1")
        .unwrap();
    let deleted = del_stmt
        .query_map([since], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap_or_default();

    Json(PullExercisesResponse {
        server_time,
        items,
        deleted,
    })
}

pub async fn pull_ingredients(
    State(state): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    let since = q.since.unwrap_or(0);
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();

    let mut stmt = conn
        .prepare(
            "SELECT id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, sodium_per100g, fiber_per100g, sugar_per100g, is_archived, brand, barcode, created_at, updated_at FROM ingredients WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at",
        )
        .unwrap();
    let rows: Vec<(String, String, f64, f64, f64, f64, Option<f64>, Option<f64>, Option<f64>, i64, Option<String>, Option<String>, i64, i64)> = stmt
        .query_map([since], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
                row.get(12)?,
                row.get(13)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();

    let mut items = Vec::new();
    for (id, name, cal, prot, carb, fat, sod, fiber, sugar, is_arch, brand, barcode, created_at, updated_at) in rows {
        let mut pic_stmt = conn
            .prepare("SELECT id, ingredient_id, image_path, sort_order, created_at FROM ingredient_pictures WHERE ingredient_id = ?1 AND deleted_at IS NULL ORDER BY sort_order")
            .unwrap();
        let pictures = pic_stmt
            .query_map([&id], |row| {
                Ok(IngredientPicture {
                    id: row.get(0)?,
                    ingredient_id: row.get(1)?,
                    image_path: row.get(2)?,
                    sort_order: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default();

        let mut price_stmt = conn
            .prepare("SELECT id, ingredient_id, store_id, price, currency_code, package_grams, recorded_at FROM ingredient_prices WHERE ingredient_id = ?1 AND deleted_at IS NULL")
            .unwrap();
        let prices = price_stmt
            .query_map([&id], |row| {
                Ok(IngredientPrice {
                    id: row.get(0)?,
                    ingredient_id: row.get(1)?,
                    store_id: row.get(2)?,
                    price: row.get(3)?,
                    currency_code: row.get(4)?,
                    package_grams: row.get(5)?,
                    recorded_at: row.get(6)?,
                })
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default();

        items.push(Ingredient {
            id,
            name,
            calories_per100g: cal,
            protein_per100g: prot,
            carbs_per100g: carb,
            fat_per100g: fat,
            sodium_per100g: sod,
            fiber_per100g: fiber,
            sugar_per100g: sugar,
            is_archived: is_arch != 0,
            brand,
            barcode,
            updated_at: Some(updated_at),
            created_at,
            pictures,
            prices,
        });
    }

    let mut store_stmt = conn
        .prepare("SELECT id, name, updated_at FROM stores WHERE deleted_at IS NULL ORDER BY name")
        .unwrap();
    let stores = store_stmt
        .query_map([], |row| {
            Ok(Store {
                id: row.get(0)?,
                name: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();

    let mut del_stmt = conn
        .prepare("SELECT id FROM ingredients WHERE deleted_at IS NOT NULL AND deleted_at > ?1")
        .unwrap();
    let deleted = del_stmt
        .query_map([since], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap_or_default();

    Json(PullIngredientsResponse {
        server_time,
        stores,
        items,
        deleted,
    })
}

pub async fn pull_fx_rates(
    State(state): State<AppState>,
    Query(q): Query<FxRatesQuery>,
) -> impl IntoResponse {
    let since = q.since.unwrap_or(0);
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT code, rate_to_base, rate_date, updated_at FROM fx_rates WHERE base_code = ?1 AND rate_date = ?2 AND updated_at > ?3 AND deleted_at IS NULL ORDER BY updated_at")
        .unwrap();
    let items = stmt
        .query_map(rusqlite::params![q.base, q.date, since], |row| {
            Ok(FxRate {
                code: row.get(0)?,
                rate_to_base: row.get(1)?,
                date: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();

    Json(PullFxRatesResponse { server_time, items })
}

#[derive(Serialize)]
pub struct PullTemplatesResponse { pub server_time: i64, pub items: Vec<serde_json::Value>, pub deleted: Vec<String> }
#[derive(Serialize)]
pub struct PullNotesResponse { pub server_time: i64, pub items: Vec<serde_json::Value>, pub deleted: Vec<String> }
#[derive(Serialize)]
pub struct PullTasksResponse { pub server_time: i64, pub items: Vec<serde_json::Value>, pub deleted: Vec<String> }
#[derive(Serialize)]
pub struct PullGoalsResponse { pub server_time: i64, pub items: Vec<serde_json::Value>, pub deleted: Vec<String> }
#[derive(Serialize)]
pub struct PullTransactionsResponse { pub server_time: i64, pub items: Vec<serde_json::Value>, pub deleted: Vec<String> }
#[derive(Serialize)]
pub struct PullAccountsResponse { pub server_time: i64, pub items: Vec<serde_json::Value>, pub deleted: Vec<String> }
#[derive(Serialize)]
pub struct PullReceiptsResponse { pub server_time: i64, pub items: Vec<serde_json::Value>, pub deleted: Vec<String> }
#[derive(Serialize)]
pub struct PullExperimentsResponse { pub server_time: i64, pub items: Vec<serde_json::Value>, pub deleted: Vec<String> }
#[derive(Serialize)]
pub struct PullTagsResponse { pub server_time: i64, pub items: Vec<serde_json::Value>, pub deleted: Vec<String> }

pub async fn pull_templates(State(state): State<AppState>, Query(q): Query<SinceQuery>) -> impl IntoResponse {
    let since = q.since.unwrap_or(0); let server_time = db::server_time_ms(); let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, name, notes, start_date, recurrence, excluded_dates, source_routine_id, created_at, updated_at FROM workout_templates WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at").unwrap();
    let items: Vec<serde_json::Value> = stmt.query_map([since], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"notes":r.get::<_,Option<String>>(2)?,"startDate":r.get::<_,i64>(3)?,"recurrence":r.get::<_,Option<String>>(4)?,"excludedDates":r.get::<_,Option<String>>(5)?,"sourceRoutineId":r.get::<_,Option<String>>(6)?,"createdAt":r.get::<_,i64>(7)?,"updatedAt":r.get::<_,i64>(8)?}))).unwrap().filter_map(|r|r.ok()).collect();
    let deleted: Vec<String> = conn.prepare("SELECT id FROM workout_templates WHERE deleted_at IS NOT NULL AND deleted_at > ?1").unwrap().query_map([since], |r| r.get(0)).unwrap().filter_map(|r|r.ok()).collect();
    Json(PullTemplatesResponse{server_time, items, deleted})
}
pub async fn pull_notes(State(state): State<AppState>, Query(q): Query<SinceQuery>) -> impl IntoResponse {
    let since = q.since.unwrap_or(0); let server_time = db::server_time_ms(); let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, title, body, updated_at, created_at FROM notes WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at").unwrap();
    let items: Vec<serde_json::Value> = stmt.query_map([since], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"body":r.get::<_,String>(2)?,"updatedAt":r.get::<_,i64>(3)?,"createdAt":r.get::<_,i64>(4)?}))).unwrap().filter_map(|r|r.ok()).collect();
    let deleted: Vec<String> = conn.prepare("SELECT id FROM notes WHERE deleted_at IS NOT NULL AND deleted_at > ?1").unwrap().query_map([since], |r| r.get(0)).unwrap().filter_map(|r|r.ok()).collect();
    Json(PullNotesResponse{server_time, items, deleted})
}
pub async fn pull_tasks(State(state): State<AppState>, Query(q): Query<SinceQuery>) -> impl IntoResponse {
    let since = q.since.unwrap_or(0); let server_time = db::server_time_ms(); let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, date, title, done, task_status, carry_over, sort_order, due_date, due_time_minutes, start_time_minutes, end_time_minutes, notes, workout_id, workout_template_id, recurrence, series_id, created_at, updated_at FROM tasks WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at").unwrap();
    let items: Vec<serde_json::Value> = stmt.query_map([since], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"date":r.get::<_,i64>(1)?,"title":r.get::<_,String>(2)?,"done":r.get::<_,i64>(3)?,"taskStatus":r.get::<_,Option<i64>>(4)?,"carryOver":r.get::<_,i64>(5)?!=0,"sortOrder":r.get::<_,i64>(6)?,"dueDate":r.get::<_,Option<i64>>(7)?,"dueTimeMinutes":r.get::<_,Option<i64>>(8)?,"startTimeMinutes":r.get::<_,Option<i64>>(9)?,"endTimeMinutes":r.get::<_,Option<i64>>(10)?,"notes":r.get::<_,Option<String>>(11)?,"workoutId":r.get::<_,Option<String>>(12)?,"workoutTemplateId":r.get::<_,Option<String>>(13)?,"recurrence":r.get::<_,Option<String>>(14)?,"seriesId":r.get::<_,Option<String>>(15)?,"createdAt":r.get::<_,i64>(16)?,"updatedAt":r.get::<_,i64>(17)?}))).unwrap().filter_map(|r|r.ok()).collect();
    let deleted: Vec<String> = conn.prepare("SELECT id FROM tasks WHERE deleted_at IS NOT NULL AND deleted_at > ?1").unwrap().query_map([since], |r| r.get(0)).unwrap().filter_map(|r|r.ok()).collect();
    Json(PullTasksResponse{server_time, items, deleted})
}
pub async fn pull_goals(State(state): State<AppState>, Query(q): Query<SinceQuery>) -> impl IntoResponse {
    let since = q.since.unwrap_or(0); let server_time = db::server_time_ms(); let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, title, description, start_date, end_date, status, target_type, target_value, baseline_value, unit, reminder_enabled, reminder_time_minutes, created_at, updated_at FROM goals WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at").unwrap();
    let items: Vec<serde_json::Value> = stmt.query_map([since], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"description":r.get::<_,Option<String>>(2)?,"startDate":r.get::<_,i64>(3)?,"endDate":r.get::<_,Option<i64>>(4)?,"status":r.get::<_,String>(5)?,"targetType":r.get::<_,String>(6)?,"targetValue":r.get::<_,Option<f64>>(7)?,"baselineValue":r.get::<_,Option<f64>>(8)?,"unit":r.get::<_,Option<String>>(9)?,"reminderEnabled":r.get::<_,i64>(10)?!=0,"reminderTimeMinutes":r.get::<_,i64>(11)?,"createdAt":r.get::<_,i64>(12)?,"updatedAt":r.get::<_,i64>(13)?}))).unwrap().filter_map(|r|r.ok()).collect();
    let deleted: Vec<String> = conn.prepare("SELECT id FROM goals WHERE deleted_at IS NOT NULL AND deleted_at > ?1").unwrap().query_map([since], |r| r.get(0)).unwrap().filter_map(|r|r.ok()).collect();
    Json(PullGoalsResponse{server_time, items, deleted})
}
pub async fn pull_transactions(State(state): State<AppState>, Query(q): Query<SinceQuery>) -> impl IntoResponse {
    let since = q.since.unwrap_or(0); let server_time = db::server_time_ms(); let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, type, amount, currency_code, amount_base, rate_used, account_id, to_account_id, category, date, note, receipt_id, is_draft, created_at, updated_at FROM transactions WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at").unwrap();
    let items: Vec<serde_json::Value> = stmt.query_map([since], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"type":r.get::<_,String>(1)?,"amount":r.get::<_,f64>(2)?,"currencyCode":r.get::<_,String>(3)?,"amountBase":r.get::<_,f64>(4)?,"rateUsed":r.get::<_,f64>(5)?,"accountId":r.get::<_,Option<String>>(6)?,"toAccountId":r.get::<_,Option<String>>(7)?,"category":r.get::<_,Option<String>>(8)?,"date":r.get::<_,i64>(9)?,"note":r.get::<_,Option<String>>(10)?,"receiptId":r.get::<_,Option<String>>(11)?,"isDraft":r.get::<_,i64>(12)?!=0,"createdAt":r.get::<_,i64>(13)?,"updatedAt":r.get::<_,i64>(14)?}))).unwrap().filter_map(|r|r.ok()).collect();
    let deleted: Vec<String> = conn.prepare("SELECT id FROM transactions WHERE deleted_at IS NOT NULL AND deleted_at > ?1").unwrap().query_map([since], |r| r.get(0)).unwrap().filter_map(|r|r.ok()).collect();
    Json(PullTransactionsResponse{server_time, items, deleted})
}
pub async fn pull_accounts(State(state): State<AppState>, Query(q): Query<SinceQuery>) -> impl IntoResponse {
    let since = q.since.unwrap_or(0); let server_time = db::server_time_ms(); let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, name, type, opening_balance, note, created_at, updated_at FROM accounts WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at").unwrap();
    let items: Vec<serde_json::Value> = stmt.query_map([since], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"type":r.get::<_,String>(2)?,"openingBalance":r.get::<_,f64>(3)?,"note":r.get::<_,Option<String>>(4)?,"createdAt":r.get::<_,i64>(5)?,"updatedAt":r.get::<_,i64>(6)?}))).unwrap().filter_map(|r|r.ok()).collect();
    let deleted: Vec<String> = conn.prepare("SELECT id FROM accounts WHERE deleted_at IS NOT NULL AND deleted_at > ?1").unwrap().query_map([since], |r| r.get(0)).unwrap().filter_map(|r|r.ok()).collect();
    Json(PullAccountsResponse{server_time, items, deleted})
}
pub async fn pull_receipts(State(state): State<AppState>, Query(q): Query<SinceQuery>) -> impl IntoResponse {
    let since = q.since.unwrap_or(0); let server_time = db::server_time_ms(); let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, local_path, remote_path, upload_status, parsed, parsed_json, transaction_id, created_at, updated_at FROM receipts WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at").unwrap();
    let items: Vec<serde_json::Value> = stmt.query_map([since], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"localPath":r.get::<_,String>(1)?,"remotePath":r.get::<_,Option<String>>(2)?,"uploadStatus":r.get::<_,i64>(3)?,"parsed":r.get::<_,i64>(4)?!=0,"parsedJson":r.get::<_,Option<String>>(5)?,"transactionId":r.get::<_,Option<String>>(6)?,"createdAt":r.get::<_,i64>(7)?,"updatedAt":r.get::<_,i64>(8)?}))).unwrap().filter_map(|r|r.ok()).collect();
    let deleted: Vec<String> = conn.prepare("SELECT id FROM receipts WHERE deleted_at IS NOT NULL AND deleted_at > ?1").unwrap().query_map([since], |r| r.get(0)).unwrap().filter_map(|r|r.ok()).collect();
    Json(PullReceiptsResponse{server_time, items, deleted})
}
pub async fn pull_experiments(State(state): State<AppState>, Query(q): Query<SinceQuery>) -> impl IntoResponse {
    let since = q.since.unwrap_or(0); let server_time = db::server_time_ms(); let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, name, purpose, start_date, end_date, status, categories, reminder_enabled, reminder_time_minutes, created_at, updated_at FROM experiments WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at").unwrap();
    let items: Vec<serde_json::Value> = stmt.query_map([since], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"purpose":r.get::<_,Option<String>>(2)?,"startDate":r.get::<_,i64>(3)?,"endDate":r.get::<_,Option<i64>>(4)?,"status":r.get::<_,String>(5)?,"categories":r.get::<_,Option<String>>(6)?,"reminderEnabled":r.get::<_,i64>(7)?!=0,"reminderTimeMinutes":r.get::<_,i64>(8)?,"createdAt":r.get::<_,i64>(9)?,"updatedAt":r.get::<_,i64>(10)?}))).unwrap().filter_map(|r|r.ok()).collect();
    let deleted: Vec<String> = conn.prepare("SELECT id FROM experiments WHERE deleted_at IS NOT NULL AND deleted_at > ?1").unwrap().query_map([since], |r| r.get(0)).unwrap().filter_map(|r|r.ok()).collect();
    Json(PullExperimentsResponse{server_time, items, deleted})
}
pub async fn pull_tags(State(state): State<AppState>, Query(q): Query<SinceQuery>) -> impl IntoResponse {
    let since = q.since.unwrap_or(0); let server_time = db::server_time_ms(); let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, name, color, sort_order, created_at, updated_at FROM tags WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at").unwrap();
    let items: Vec<serde_json::Value> = stmt.query_map([since], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"color":r.get::<_,Option<i64>>(2)?,"sortOrder":r.get::<_,i64>(3)?,"createdAt":r.get::<_,i64>(4)?,"updatedAt":r.get::<_,i64>(5)?}))).unwrap().filter_map(|r|r.ok()).collect();
    let deleted: Vec<String> = conn.prepare("SELECT id FROM tags WHERE deleted_at IS NOT NULL AND deleted_at > ?1").unwrap().query_map([since], |r| r.get(0)).unwrap().filter_map(|r|r.ok()).collect();
    Json(PullTagsResponse{server_time, items, deleted})
}

// ---------------------------------------------------------------------------
// Selective-sync catalog (lightweight index for the mobile picker).
// Minimal rows only: exercises -> {id, name}, ingredients -> {id, name,
// barcode?}. No nested pictures/prices, no media binaries, no cursor.
// Per-id item routes return the selective shapes the mobile import path
// upserts (exercise full + hasImage/hasVideo; ingredient minimal without
// prices/stores). Bulk `?since=` pulls above are unchanged.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ExerciseCatalogEntry {
    pub id: String,
    pub name: String,
    #[serde(rename = "has_image")]
    pub has_image: bool,
}

#[derive(Serialize)]
pub struct PullExerciseCatalogResponse {
    pub server_time: i64,
    pub items: Vec<ExerciseCatalogEntry>,
}

#[derive(Serialize)]
pub struct IngredientCatalogEntry {
    pub id: String,
    pub name: String,
    pub barcode: Option<String>,
}

#[derive(Serialize)]
pub struct PullIngredientCatalogResponse {
    pub server_time: i64,
    pub items: Vec<IngredientCatalogEntry>,
}

pub async fn pull_exercise_catalog(State(state): State<AppState>) -> impl IntoResponse {
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, name, image_path FROM exercises WHERE deleted_at IS NULL ORDER BY name")
        .unwrap();
    let items = stmt
        .query_map([], |row| {
            let image_path: Option<String> = row.get(2)?;
            Ok(ExerciseCatalogEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                has_image: image_path.is_some(),
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    Json(PullExerciseCatalogResponse { server_time, items })
}

pub async fn pull_ingredient_catalog(State(state): State<AppState>) -> impl IntoResponse {
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, name, barcode FROM ingredients WHERE deleted_at IS NULL ORDER BY name")
        .unwrap();
    let items = stmt
        .query_map([], |row| {
            Ok(IngredientCatalogEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                barcode: row.get(2)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    Json(PullIngredientCatalogResponse { server_time, items })
}

fn not_found(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({"message": message})))
}

fn read_exercise_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Exercise> {
    let instructions_str: Option<String> = row.get(7)?;
    let tips_str: Option<String> = row.get(8)?;
    let keywords_str: Option<String> = row.get(10)?;
    let tags_str: Option<String> = row.get(12)?;
    let image_path: Option<String> = row.get(16)?;
    let video_path: Option<String> = row.get(17)?;
    Ok(Exercise {
        id: row.get(0)?,
        name: row.get(1)?,
        exercise_type: row.get(2)?,
        body_part: row.get(3)?,
        equipment: row.get(4)?,
        primary_muscle: row.get(5)?,
        secondary_muscle: row.get(6)?,
        instructions: instructions_str.and_then(|s| serde_json::from_str(&s).ok()),
        tips: tips_str.and_then(|s| serde_json::from_str(&s).ok()),
        faqs: row.get(9)?,
        keywords: keywords_str.and_then(|s| serde_json::from_str(&s).ok()),
        similar_to: row.get(11)?,
        tags: tags_str.and_then(|s| serde_json::from_str(&s).ok()),
        is_canonical: row.get::<_, i64>(13)? != 0,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        has_image: image_path.is_some(),
        has_video: video_path.is_some(),
    })
}

pub async fn get_exercise_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, exercise_type, body_part, equipment, primary_muscle, secondary_muscle, instructions, tips, faqs, keywords, similar_to, tags, is_canonical, created_at, updated_at, image_path, video_path FROM exercises WHERE id = ?1 AND deleted_at IS NULL",
        )
        .unwrap();
    match stmt.query_row([&id], read_exercise_row) {
        Ok(item) => (StatusCode::OK, Json(serde_json::to_value(item).unwrap())).into_response(),
        Err(rusqlite::Error::QueryReturnedNoRows) => not_found("exercise not found").into_response(),
        Err(_) => not_found("exercise not found").into_response(),
    }
}

pub async fn get_ingredient_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, sodium_per100g, fiber_per100g, sugar_per100g, is_archived, brand, barcode, created_at, updated_at FROM ingredients WHERE id = ?1 AND deleted_at IS NULL",
        )
        .unwrap();
    let row: rusqlite::Result<(String, String, f64, f64, f64, f64, Option<f64>, Option<f64>, Option<f64>, i64, Option<String>, Option<String>, i64, i64)> =
        stmt.query_row([&id], |r| {
            Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?,
                r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?,
                r.get(11)?, r.get(12)?, r.get(13)?,
            ))
        });
    let (id, name, cal, prot, carb, fat, sod, fiber, sugar, is_arch, brand, barcode, created_at, updated_at) =
        match row {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => return not_found("ingredient not found").into_response(),
            Err(_) => return not_found("ingredient not found").into_response(),
        };

    let mut pic_stmt = conn
        .prepare("SELECT id, ingredient_id, image_path, sort_order, created_at FROM ingredient_pictures WHERE ingredient_id = ?1 AND deleted_at IS NULL ORDER BY sort_order")
        .unwrap();
    let pictures = pic_stmt
        .query_map([&id], |r| {
            Ok(IngredientPicture {
                id: r.get(0)?,
                ingredient_id: r.get(1)?,
                image_path: r.get(2)?,
                sort_order: r.get(3)?,
                created_at: r.get(4)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();

    let body = serde_json::json!({
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
        "pictures": pictures,
    });
    (StatusCode::OK, Json(body)).into_response()
}
