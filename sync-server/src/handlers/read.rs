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
    pub workouts: Vec<WorkoutRead>,
    #[serde(rename = "workoutExercises")]
    pub workout_exercises: Vec<WorkoutExerciseRead>,
    #[serde(rename = "exerciseSets")]
    pub exercise_sets: Vec<ExerciseSetRead>,
    pub deleted: Vec<String>,
}

#[derive(Serialize)]
pub struct WorkoutRead {
    pub id: String,
    pub name: String,
    pub date: i64,
    #[serde(rename = "startedAt")]
    pub started_at: Option<i64>,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<i64>,
    pub notes: Option<String>,
    #[serde(rename = "routineId")]
    pub routine_id: Option<String>,
    #[serde(rename = "templateId")]
    pub template_id: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Serialize)]
pub struct WorkoutExerciseRead {
    pub id: String,
    #[serde(rename = "workoutId")]
    pub workout_id: String,
    #[serde(rename = "exerciseId")]
    pub exercise_id: String,
    #[serde(rename = "sortOrder")]
    pub sort_order: i64,
    pub notes: Option<String>,
}

#[derive(Serialize)]
pub struct ExerciseSetRead {
    pub id: String,
    #[serde(rename = "workoutExerciseId")]
    pub workout_exercise_id: String,
    #[serde(rename = "setNumber")]
    pub set_number: i64,
    pub reps: Option<i64>,
    #[serde(rename = "weightKg")]
    pub weight_kg: Option<f64>,
    #[serde(rename = "restSeconds")]
    pub rest_seconds: Option<i64>,
}

#[derive(Serialize)]
pub struct PullMealsResponse {
    pub server_time: i64,
    pub meals: Vec<MealRead>,
    #[serde(rename = "mealIngredients")]
    pub meal_ingredients: Vec<MealIngredientRead>,
    pub deleted: Vec<String>,
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
pub struct MealIngredientRead {
    pub id: String,
    #[serde(rename = "mealId")]
    pub meal_id: String,
    #[serde(rename = "ingredientId")]
    pub ingredient_id: String,
    pub grams: f64,
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
    let conn = state.db.lock().unwrap();

    let mut stmt = conn
        .prepare("SELECT id, name, date, started_at, completed_at, notes, routine_id, template_id, created_at, updated_at FROM workouts WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at")
        .unwrap();
    let workouts = stmt
        .query_map([since], |row| {
            Ok(WorkoutRead {
                id: row.get(0)?,
                name: row.get(1)?,
                date: row.get(2)?,
                started_at: row.get(3)?,
                completed_at: row.get(4)?,
                notes: row.get(5)?,
                routine_id: row.get(6)?,
                template_id: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();

    let workout_ids: Vec<String> = workouts.iter().map(|w| w.id.clone()).collect();
    let mut we = Vec::new();
    let mut es = Vec::new();
    if !workout_ids.is_empty() {
        let placeholders = workout_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, workout_id, exercise_id, sort_order, notes FROM workout_exercises WHERE workout_id IN ({}) AND deleted_at IS NULL", placeholders);
        let mut stmt2 = conn.prepare(&sql).unwrap();
        let params: Vec<&dyn rusqlite::ToSql> = workout_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        we = stmt2
            .query_map(params.as_slice(), |row| {
                Ok(WorkoutExerciseRead {
                    id: row.get(0)?,
                    workout_id: row.get(1)?,
                    exercise_id: row.get(2)?,
                    sort_order: row.get(3)?,
                    notes: row.get(4)?,
                })
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default();

        if !we.is_empty() {
            let we_ids: Vec<String> = we.iter().map(|w| w.id.clone()).collect();
            let ph = we_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql3 = format!("SELECT id, workout_exercise_id, set_number, reps, weight_kg, rest_seconds FROM exercise_sets WHERE workout_exercise_id IN ({}) AND deleted_at IS NULL", ph);
            let mut stmt3 = conn.prepare(&sql3).unwrap();
            let params3: Vec<&dyn rusqlite::ToSql> = we_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
            es = stmt3
                .query_map(params3.as_slice(), |row| {
                    Ok(ExerciseSetRead {
                        id: row.get(0)?,
                        workout_exercise_id: row.get(1)?,
                        set_number: row.get(2)?,
                        reps: row.get(3)?,
                        weight_kg: row.get(4)?,
                        rest_seconds: row.get(5)?,
                    })
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap_or_default();
        }
    }

    let mut del_stmt = conn
        .prepare("SELECT id FROM workouts WHERE deleted_at IS NOT NULL AND deleted_at > ?1")
        .unwrap();
    let deleted = del_stmt
        .query_map([since], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap_or_default();

    Json(PullWorkoutsResponse {
        server_time,
        workouts,
        workout_exercises: we,
        exercise_sets: es,
        deleted,
    })
}

pub async fn pull_meals(
    State(state): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    let since = q.since.unwrap_or(0);
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();

    let mut stmt = conn
        .prepare("SELECT id, name, eaten_at, created_at, updated_at FROM meals WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at")
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
        let sql = format!("SELECT id, meal_id, ingredient_id, grams FROM meal_ingredients WHERE meal_id IN ({}) AND deleted_at IS NULL", ph);
        let mut stmt2 = conn.prepare(&sql).unwrap();
        let params: Vec<&dyn rusqlite::ToSql> = meal_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        mis = stmt2
            .query_map(params.as_slice(), |row| {
                Ok(MealIngredientRead {
                    id: row.get(0)?,
                    meal_id: row.get(1)?,
                    ingredient_id: row.get(2)?,
                    grams: row.get(3)?,
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

    Json(PullMealsResponse {
        server_time,
        meals,
        meal_ingredients: mis,
        deleted,
    })
}

pub async fn pull_body_metrics(
    State(state): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    let since = q.since.unwrap_or(0);
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();

    let mut stmt = conn
        .prepare("SELECT id, date, weight_kg, height_cm, created_at, updated_at FROM body_metrics WHERE deleted_at IS NULL AND updated_at > ?1 ORDER BY updated_at")
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
