use axum::{
    extract::{Query, State},
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
