use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{db, server::AppState};

#[derive(Deserialize)]
pub struct SinceQuery {
    pub since: Option<i64>,
}

#[derive(Serialize)]
pub struct PullWorkoutsResponse {
    pub server_time: i64,
    pub items: Vec<serde_json::Value>,
    pub deleted: Vec<String>,
    #[serde(rename = "deletedExercises")]
    pub deleted_exercises: Vec<String>,
    #[serde(rename = "deletedSets")]
    pub deleted_sets: Vec<String>,
}

#[derive(Serialize)]
pub struct PullMealsResponse {
    pub server_time: i64,
    pub meals: Vec<MealRead>,
    #[serde(rename = "mealFoods")]
    pub meal_foods: Vec<MealFoodRead>,
    pub deleted: Vec<String>,
    #[serde(rename = "deletedMealFoods")]
    pub deleted_meal_foods: Vec<String>,
}

#[derive(Serialize)]
pub struct MealRead {
    pub id: String,
    pub name: String,
    #[serde(rename = "eatenAt")]
    pub eaten_at: i64,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Serialize)]
/// A logged food with its portion snapshot.
///
/// The macros are replayed exactly as they were frozen at log time, and the
/// food name is joined live so a renamed recipe shows up in old meals without
/// their numbers moving.
pub struct MealFoodRead {
    pub id: String,
    #[serde(rename = "mealId")]
    pub meal_id: String,
    #[serde(rename = "foodId")]
    pub food_id: String,
    #[serde(rename = "foodName")]
    pub food_name: Option<String>,
    pub amount: f64,
    pub calories: f64,
    pub protein: f64,
    pub carbs: f64,
    pub fat: f64,
    pub sodium: Option<f64>,
    pub fiber: Option<f64>,
    pub sugar: Option<f64>,
}

#[derive(Serialize)]
pub struct PullBodyMetricsResponse {
    pub server_time: i64,
    pub items: Vec<BodyMetricRead>,
    pub deleted: Vec<String>,
}

#[derive(Serialize)]
pub struct BodyMetricRead {
    pub id: String,
    pub date: i64,
    #[serde(rename = "weightKg")]
    pub weight_kg: Option<f64>,
    #[serde(rename = "heightCm")]
    pub height_cm: Option<f64>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

pub async fn pull_workouts(
    State(state): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    let since = q.since.unwrap_or(0);
    let server_time = db::server_time_ms();
    let conn = state.db.get().unwrap();

    // A workout is returned whole whenever *anything* in its graph changed:
    // the row itself, one of its exercises, or one of their sets. Filtering on
    // the parent alone was why a pull could return a header with no children —
    // the phone had already seen `workouts.updated_at`, so an exercise added on
    // another device never came back.
    let u = if since > 0 { since } else { 0 };
    let mut stmt = conn
        .prepare(
            "SELECT w.id, w.name, w.date, w.started_at, w.completed_at, w.notes, \
                    w.routine_id, w.template_id, w.created_at, w.updated_at \
             FROM workouts w \
             WHERE w.deleted_at IS NULL AND ( \
                 ?1 <= 0 \
                 OR w.updated_at > ?1 \
                 OR EXISTS (SELECT 1 FROM workout_exercises we \
                            WHERE we.workout_id = w.id AND we.updated_at > ?1) \
                 OR EXISTS (SELECT 1 FROM exercise_sets s \
                            JOIN workout_exercises we ON we.id = s.workout_exercise_id \
                            WHERE we.workout_id = w.id AND s.updated_at > ?1) \
             ) ORDER BY w.updated_at",
        )
        .unwrap();
    let workouts = stmt
        .query_map([u], |row| {
            Ok((
                row.get::<_, String>(0)?,
                serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "name": row.get::<_, String>(1)?,
                    "date": row.get::<_, i64>(2)?,
                    "startedAt": row.get::<_, Option<i64>>(3)?,
                    "completedAt": row.get::<_, Option<i64>>(4)?,
                    "notes": row.get::<_, Option<String>>(5)?,
                    "routineId": row.get::<_, Option<String>>(6)?,
                    "templateId": row.get::<_, Option<String>>(7)?,
                    "createdAt": row.get::<_, i64>(8)?,
                    "updatedAt": row.get::<_, i64>(9)?,
                }),
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect::<Vec<(String, serde_json::Value)>>();

    let mut ex_stmt = conn
        .prepare(
            "SELECT id, exercise_id, sort_order, notes FROM workout_exercises \
             WHERE workout_id = ?1 AND deleted_at IS NULL ORDER BY sort_order",
        )
        .unwrap();
    let mut set_stmt = conn
        .prepare(
            "SELECT id, set_number, reps, weight_kg, rest_seconds, actual_reps, \
                    actual_weight_kg, actual_rest_seconds, completed_at, \
                    duration_minutes, distance_meters, actual_duration_minutes, \
                    actual_distance_meters, notes \
             FROM exercise_sets \
             WHERE workout_exercise_id = ?1 AND deleted_at IS NULL ORDER BY set_number",
        )
        .unwrap();

    let mut items = Vec::with_capacity(workouts.len());
    for (workout_id, mut workout) in workouts {
        let exercise_rows: Vec<(String, String, i64, Option<String>)> = ex_stmt
            .query_map([&workout_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        let mut exercises = Vec::with_capacity(exercise_rows.len());
        for (ex_id, exercise_id, sort_order, notes) in exercise_rows {
            let sets: Vec<serde_json::Value> = set_stmt
                .query_map([&ex_id], |r| {
                    Ok(serde_json::json!({
                        "id": r.get::<_, String>(0)?,
                        "workoutExerciseId": ex_id,
                        "setNumber": r.get::<_, i64>(1)?,
                        "reps": r.get::<_, Option<i64>>(2)?,
                        "weightKg": r.get::<_, Option<f64>>(3)?,
                        "restSeconds": r.get::<_, Option<i64>>(4)?,
                        "actualReps": r.get::<_, Option<i64>>(5)?,
                        "actualWeightKg": r.get::<_, Option<f64>>(6)?,
                        "actualRestSeconds": r.get::<_, Option<i64>>(7)?,
                        "completedAt": r.get::<_, Option<i64>>(8)?,
                        "durationMinutes": r.get::<_, Option<i64>>(9)?,
                        "distanceMeters": r.get::<_, Option<f64>>(10)?,
                        "actualDurationMinutes": r.get::<_, Option<i64>>(11)?,
                        "actualDistanceMeters": r.get::<_, Option<f64>>(12)?,
                        "notes": r.get::<_, Option<String>>(13)?,
                    }))
                })
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            exercises.push(serde_json::json!({
                "id": ex_id,
                "workoutId": workout_id,
                "exerciseId": exercise_id,
                "sortOrder": sort_order,
                "notes": notes,
                "sets": sets,
            }));
        }
        workout
            .as_object_mut()
            .map(|o| o.insert("exercises".into(), serde_json::Value::Array(exercises)));
        items.push(workout);
    }

    let deleted: Vec<String> = conn
        .prepare("SELECT id FROM workouts WHERE deleted_at IS NOT NULL AND deleted_at > ?1")
        .unwrap()
        .query_map([since], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let deleted_exercises: Vec<String> = conn
        .prepare(
            "SELECT id FROM workout_exercises \
             WHERE deleted_at IS NOT NULL AND deleted_at > ?1",
        )
        .unwrap()
        .query_map([since], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let deleted_sets: Vec<String> = conn
        .prepare(
            "SELECT id FROM exercise_sets \
             WHERE deleted_at IS NOT NULL AND deleted_at > ?1",
        )
        .unwrap()
        .query_map([since], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    Json(PullWorkoutsResponse {
        server_time,
        items,
        deleted,
        deleted_exercises,
        deleted_sets,
    })
}

pub async fn pull_meals(
    State(state): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    let since = q.since.unwrap_or(0);
    let server_time = db::server_time_ms();
    let conn = state.db.get().unwrap();

    // A meal is returned when the meal row changed, or when any of its logged
    // foods changed. Filtering on the parent alone meant adding or editing a
    // food on another device never reached this one.
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.name, m.eaten_at, m.created_at, m.updated_at \
             FROM meals m \
             WHERE m.deleted_at IS NULL AND ( \
                 ?1 <= 0 \
                 OR m.updated_at > ?1 \
                 OR EXISTS (SELECT 1 FROM meal_foods mf \
                            WHERE mf.meal_id = m.id AND mf.updated_at > ?1) \
             ) ORDER BY m.updated_at",
        )
        .unwrap();
    let meals = stmt
        .query_map([since], |row| {
            Ok(MealRead {
                id: row.get(0)?,
                name: row.get(1)?,
                eaten_at: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();

    let meal_ids: Vec<String> = meals.iter().map(|m| m.id.clone()).collect();
    let mut mis = Vec::new();
    if !meal_ids.is_empty() {
        let ph = meal_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT mf.id, mf.meal_id, mf.food_id, f.name, mf.amount, mf.calories, mf.protein, mf.carbs, mf.fat, mf.sodium, mf.fiber, mf.sugar FROM meal_foods mf LEFT JOIN foods f ON f.id = mf.food_id WHERE mf.meal_id IN ({}) AND mf.deleted_at IS NULL", ph);
        let mut stmt2 = conn.prepare(&sql).unwrap();
        let params: Vec<&dyn rusqlite::ToSql> = meal_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        mis = stmt2
            .query_map(params.as_slice(), |row| {
                Ok(MealFoodRead {
                    id: row.get(0)?,
                    meal_id: row.get(1)?,
                    food_id: row.get(2)?,
                    food_name: row.get(3)?,
                    amount: row.get(4)?,
                    calories: row.get(5)?,
                    protein: row.get(6)?,
                    carbs: row.get(7)?,
                    fat: row.get(8)?,
                    sodium: row.get(9)?,
                    fiber: row.get(10)?,
                    sugar: row.get(11)?,
                })
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default();
    }

    let mut del_stmt = conn
        .prepare("SELECT id FROM meals WHERE deleted_at IS NOT NULL AND deleted_at > ?1")
        .unwrap();
    let deleted = del_stmt
        .query_map([since], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap_or_default();

    let mut food_del_stmt = conn
        .prepare("SELECT id FROM meal_foods WHERE deleted_at IS NOT NULL AND deleted_at > ?1")
        .unwrap();
    let deleted_meal_foods = food_del_stmt
        .query_map([since], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap_or_default();

    Json(PullMealsResponse {
        server_time,
        meals,
        meal_foods: mis,
        deleted,
        deleted_meal_foods,
    })
}

pub async fn pull_body_metrics(
    State(state): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    let since = q.since.unwrap_or(0);
    let server_time = db::server_time_ms();
    let conn = state.db.get().unwrap();

    let mut stmt = conn
        .prepare("SELECT id, date, weight_kg, height_cm, created_at, updated_at FROM body_metrics WHERE deleted_at IS NULL AND (?1 <= 0 OR updated_at > ?1) ORDER BY updated_at")
        .unwrap();
    let items = stmt
        .query_map([since], |row| {
            Ok(BodyMetricRead {
                id: row.get(0)?,
                date: row.get(1)?,
                weight_kg: row.get(2)?,
                height_cm: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();

    let mut del_stmt = conn
        .prepare("SELECT id FROM body_metrics WHERE deleted_at IS NOT NULL AND deleted_at > ?1")
        .unwrap();
    let deleted = del_stmt
        .query_map([since], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap_or_default();

    Json(PullBodyMetricsResponse {
        server_time,
        items,
        deleted,
    })
}
