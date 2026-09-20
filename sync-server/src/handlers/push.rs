use axum::{body::Bytes, extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};

use crate::{db, server::AppState};

#[derive(Serialize)]
pub struct SyncAck {
    pub server_time: i64,
}

#[derive(Deserialize)]
pub struct IngredientContribution {
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
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(default)]
    pub pictures: Vec<IngredientPicturePush>,
    #[serde(default)]
    pub prices: Vec<IngredientPricePush>,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Deserialize)]
pub struct IngredientPicturePush {
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

#[derive(Deserialize)]
pub struct IngredientPricePush {
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

#[derive(Deserialize)]
pub struct WorkoutsPush {
    #[serde(default)]
    pub workouts: Vec<WorkoutPush>,
    #[serde(rename = "workoutExercises", default)]
    pub workout_exercises: Vec<WorkoutExercisePush>,
    #[serde(rename = "exerciseSets", default)]
    pub exercise_sets: Vec<ExerciseSetPush>,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Deserialize)]
pub struct WorkoutPush {
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
}

#[derive(Deserialize)]
pub struct WorkoutExercisePush {
    pub id: String,
    #[serde(rename = "workoutId")]
    pub workout_id: String,
    #[serde(rename = "exerciseId")]
    pub exercise_id: String,
    #[serde(rename = "sortOrder")]
    pub sort_order: i64,
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct ExerciseSetPush {
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
    #[serde(rename = "actualReps")]
    pub actual_reps: Option<i64>,
    #[serde(rename = "actualWeightKg")]
    pub actual_weight_kg: Option<f64>,
    #[serde(rename = "actualRestSeconds")]
    pub actual_rest_seconds: Option<i64>,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<i64>,
    #[serde(rename = "durationMinutes")]
    pub duration_minutes: Option<i64>,
    #[serde(rename = "distanceMeters")]
    pub distance_meters: Option<f64>,
    #[serde(rename = "actualDurationMinutes")]
    pub actual_duration_minutes: Option<i64>,
    #[serde(rename = "actualDistanceMeters")]
    pub actual_distance_meters: Option<f64>,
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct TemplatesPush {
    #[serde(rename = "workoutTemplates", default)]
    pub workout_templates: Vec<WorkoutTemplatePush>,
    #[serde(rename = "workoutTemplateExercises", default)]
    pub workout_template_exercises: Vec<WorkoutTemplateExercisePush>,
    #[serde(rename = "workoutTemplateSets", default)]
    pub workout_template_sets: Vec<WorkoutTemplateSetPush>,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Deserialize)]
pub struct WorkoutTemplatePush {
    pub id: String,
    pub name: String,
    pub notes: Option<String>,
    #[serde(rename = "startDate")]
    pub start_date: i64,
    pub recurrence: Option<String>,
    #[serde(rename = "excludedDates")]
    pub excluded_dates: Option<String>,
    #[serde(rename = "sourceRoutineId")]
    pub source_routine_id: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Deserialize)]
pub struct WorkoutTemplateExercisePush {
    pub id: String,
    #[serde(rename = "templateId")]
    pub template_id: String,
    #[serde(rename = "exerciseId")]
    pub exercise_id: String,
    #[serde(rename = "sortOrder")]
    pub sort_order: i64,
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct WorkoutTemplateSetPush {
    pub id: String,
    #[serde(rename = "templateExerciseId")]
    pub template_exercise_id: String,
    #[serde(rename = "setNumber")]
    pub set_number: i64,
    pub reps: Option<i64>,
    #[serde(rename = "weightKg")]
    pub weight_kg: Option<f64>,
    #[serde(rename = "restSeconds")]
    pub rest_seconds: Option<i64>,
    #[serde(rename = "durationMinutes")]
    pub duration_minutes: Option<i64>,
    #[serde(rename = "distanceMeters")]
    pub distance_meters: Option<f64>,
}

pub async fn push_ingredient(
    State(state): State<AppState>,
    Json(payload): Json<IngredientContribution>,
) -> impl IntoResponse {
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction().unwrap();

    tx.execute(
        "INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, sodium_per100g, fiber_per100g, sugar_per100g, is_archived, brand, barcode, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL) ON CONFLICT(id) DO UPDATE SET name=excluded.name, calories_per100g=excluded.calories_per100g, protein_per100g=excluded.protein_per100g, carbs_per100g=excluded.carbs_per100g, fat_per100g=excluded.fat_per100g, sodium_per100g=excluded.sodium_per100g, fiber_per100g=excluded.fiber_per100g, sugar_per100g=excluded.sugar_per100g, is_archived=excluded.is_archived, brand=excluded.brand, barcode=excluded.barcode, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
        rusqlite::params![
            payload.id,
            payload.name,
            payload.calories_per100g,
            payload.protein_per100g,
            payload.carbs_per100g,
            payload.fat_per100g,
            payload.sodium_per100g,
            payload.fiber_per100g,
            payload.sugar_per100g,
            if payload.is_archived { 1 } else { 0 },
            payload.brand,
            payload.barcode,
            payload.created_at,
            server_time
        ],
    )
    .unwrap();

    for pic in payload.pictures {
        tx.execute(
            "INSERT INTO ingredient_pictures (id, ingredient_id, image_path, sort_order, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL) ON CONFLICT(id) DO UPDATE SET ingredient_id=excluded.ingredient_id, image_path=excluded.image_path, sort_order=excluded.sort_order, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![pic.id, pic.ingredient_id, pic.image_path, pic.sort_order, pic.created_at, server_time],
        )
        .unwrap();
    }

    for price in payload.prices {
        tx.execute(
            "INSERT OR IGNORE INTO stores (id, name, created_at, updated_at) VALUES (?1, ?1, ?2, ?2)",
            rusqlite::params![price.store_id, server_time],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO ingredient_prices (id, ingredient_id, store_id, price, currency_code, package_grams, recorded_at, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, NULL) ON CONFLICT(id) DO UPDATE SET ingredient_id=excluded.ingredient_id, store_id=excluded.store_id, price=excluded.price, currency_code=excluded.currency_code, package_grams=excluded.package_grams, recorded_at=excluded.recorded_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![
                price.id,
                price.ingredient_id,
                price.store_id,
                price.price,
                price.currency_code,
                price.package_grams,
                price.recorded_at,
                server_time
            ],
        )
        .unwrap();
    }
    for id in payload.deleted {
        let _ = tx.execute("UPDATE ingredients SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE ingredient_pictures SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE ingredient_prices SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
    }

    tx.commit().unwrap();
    Json(SyncAck { server_time })
}

pub async fn push_workouts(
    State(state): State<AppState>,
    Json(payload): Json<WorkoutsPush>,
) -> impl IntoResponse {
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction().unwrap();

    for w in payload.workouts {
        tx.execute(
            "INSERT INTO workouts (id, name, date, started_at, completed_at, notes, routine_id, template_id, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL) ON CONFLICT(id) DO UPDATE SET name=excluded.name, date=excluded.date, started_at=excluded.started_at, completed_at=excluded.completed_at, notes=excluded.notes, routine_id=excluded.routine_id, template_id=excluded.template_id, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![w.id, w.name, w.date, w.started_at, w.completed_at, w.notes, w.routine_id, w.template_id, w.created_at, server_time],
        )
        .unwrap();
    }
    for we in payload.workout_exercises {
        tx.execute(
            "INSERT INTO workout_exercises (id, workout_id, exercise_id, sort_order, notes, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL) ON CONFLICT(id) DO UPDATE SET workout_id=excluded.workout_id, exercise_id=excluded.exercise_id, sort_order=excluded.sort_order, notes=excluded.notes, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![we.id, we.workout_id, we.exercise_id, we.sort_order, we.notes, server_time],
        )
        .unwrap();
    }
    for s in payload.exercise_sets {
        tx.execute(
            "INSERT INTO exercise_sets (id, workout_exercise_id, set_number, reps, weight_kg, rest_seconds, actual_reps, actual_weight_kg, actual_rest_seconds, completed_at, duration_minutes, distance_meters, actual_duration_minutes, actual_distance_meters, notes, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, NULL) ON CONFLICT(id) DO UPDATE SET workout_exercise_id=excluded.workout_exercise_id, set_number=excluded.set_number, reps=excluded.reps, weight_kg=excluded.weight_kg, rest_seconds=excluded.rest_seconds, actual_reps=excluded.actual_reps, actual_weight_kg=excluded.actual_weight_kg, actual_rest_seconds=excluded.actual_rest_seconds, completed_at=excluded.completed_at, duration_minutes=excluded.duration_minutes, distance_meters=excluded.distance_meters, actual_duration_minutes=excluded.actual_duration_minutes, actual_distance_meters=excluded.actual_distance_meters, notes=excluded.notes, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![s.id, s.workout_exercise_id, s.set_number, s.reps, s.weight_kg, s.rest_seconds, s.actual_reps, s.actual_weight_kg, s.actual_rest_seconds, s.completed_at, s.duration_minutes, s.distance_meters, s.actual_duration_minutes, s.actual_distance_meters, s.notes, server_time],
        ).unwrap();
    }
    for id in payload.deleted {
        let _ = tx.execute("UPDATE workouts SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE workout_exercises SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE exercise_sets SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
    }

    tx.commit().unwrap();
    Json(SyncAck { server_time })
}

pub async fn push_templates(
    State(state): State<AppState>,
    Json(payload): Json<TemplatesPush>,
) -> impl IntoResponse {
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction().unwrap();

    for t in payload.workout_templates {
        tx.execute(
            "INSERT INTO workout_templates (id, name, notes, start_date, recurrence, excluded_dates, source_routine_id, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL) ON CONFLICT(id) DO UPDATE SET name=excluded.name, notes=excluded.notes, start_date=excluded.start_date, recurrence=excluded.recurrence, excluded_dates=excluded.excluded_dates, source_routine_id=excluded.source_routine_id, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![t.id, t.name, t.notes, t.start_date, t.recurrence, t.excluded_dates, t.source_routine_id, t.created_at, t.updated_at],
        )
        .unwrap();
    }
    for te in payload.workout_template_exercises {
        tx.execute(
            "INSERT INTO workout_template_exercises (id, template_id, exercise_id, sort_order, notes, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL) ON CONFLICT(id) DO UPDATE SET template_id=excluded.template_id, exercise_id=excluded.exercise_id, sort_order=excluded.sort_order, notes=excluded.notes, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![te.id, te.template_id, te.exercise_id, te.sort_order, te.notes, server_time],
        )
        .unwrap();
    }
    for ts in payload.workout_template_sets {
        tx.execute(
            "INSERT INTO workout_template_sets (id, template_exercise_id, set_number, reps, weight_kg, rest_seconds, duration_minutes, distance_meters, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL) ON CONFLICT(id) DO UPDATE SET template_exercise_id=excluded.template_exercise_id, set_number=excluded.set_number, reps=excluded.reps, weight_kg=excluded.weight_kg, rest_seconds=excluded.rest_seconds, duration_minutes=excluded.duration_minutes, distance_meters=excluded.distance_meters, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![ts.id, ts.template_exercise_id, ts.set_number, ts.reps, ts.weight_kg, ts.rest_seconds, ts.duration_minutes, ts.distance_meters, server_time],
        )
        .unwrap();
    }
    for id in payload.deleted {
        let _ = tx.execute("UPDATE workout_templates SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE workout_template_exercises SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE workout_template_sets SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
    }

    tx.commit().unwrap();
    Json(SyncAck { server_time })
}

#[derive(Deserialize)]
pub struct NoteAudioPush {
    pub id: String,
    #[serde(rename = "noteId")]
    pub note_id: String,
    #[serde(rename = "audioPath")]
    pub audio_path: String,
    #[serde(rename = "durationMs")]
    pub duration_ms: i64,
    pub position: i64,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct NotesPush {
    #[serde(default)]
    pub notes: Vec<NotePush>,
    #[serde(rename = "noteTags", default)]
    pub note_tags: Vec<NoteTagPush>,
    #[serde(rename = "noteAudio", default)]
    pub note_audio: Vec<NoteAudioPush>,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Deserialize)]
pub struct NotePush {
    pub id: String,
    pub title: String,
    pub body: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct NoteTagPush {
    #[serde(rename = "tagId")]
    pub tag_id: String,
    #[serde(rename = "noteId")]
    pub note_id: String,
}

#[derive(Deserialize)]
pub struct TasksPush {
    #[serde(default)]
    pub tasks: Vec<TaskPush>,
    #[serde(rename = "taskTags", default)]
    pub task_tags: Vec<TaskTagPush>,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Deserialize)]
pub struct TaskPush {
    pub id: String,
    pub date: i64,
    pub title: String,
    pub done: i64,
    #[serde(rename = "taskStatus")]
    pub task_status: Option<i64>,
    #[serde(rename = "carryOver")]
    pub carry_over: bool,
    #[serde(rename = "sortOrder")]
    pub sort_order: i64,
    #[serde(rename = "dueDate")]
    pub due_date: Option<i64>,
    #[serde(rename = "dueTimeMinutes")]
    pub due_time_minutes: Option<i64>,
    #[serde(rename = "startTimeMinutes")]
    pub start_time_minutes: Option<i64>,
    #[serde(rename = "endTimeMinutes")]
    pub end_time_minutes: Option<i64>,
    pub notes: Option<String>,
    #[serde(rename = "workoutId")]
    pub workout_id: Option<String>,
    #[serde(rename = "workoutTemplateId")]
    pub workout_template_id: Option<String>,
    pub recurrence: Option<String>,
    #[serde(rename = "seriesId")]
    pub series_id: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct TaskTagPush {
    #[serde(rename = "tagId")]
    pub tag_id: String,
    #[serde(rename = "taskId")]
    pub task_id: String,
}

#[derive(Deserialize)]
pub struct GoalsPush {
    #[serde(default)]
    pub goals: Vec<GoalPush>,
    #[serde(rename = "goalProgressEntries", default)]
    pub goal_progress_entries: Vec<GoalProgressEntryPush>,
    #[serde(rename = "goalTags", default)]
    pub goal_tags: Vec<GoalTagPush>,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Deserialize)]
pub struct GoalPush {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(rename = "startDate")]
    pub start_date: i64,
    #[serde(rename = "endDate")]
    pub end_date: Option<i64>,
    pub status: String,
    #[serde(rename = "targetType")]
    pub target_type: String,
    #[serde(rename = "targetValue")]
    pub target_value: Option<f64>,
    #[serde(rename = "baselineValue")]
    pub baseline_value: Option<f64>,
    pub unit: Option<String>,
    #[serde(rename = "reminderEnabled")]
    pub reminder_enabled: bool,
    #[serde(rename = "reminderTimeMinutes")]
    pub reminder_time_minutes: i64,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Deserialize)]
pub struct GoalProgressEntryPush {
    pub id: String,
    #[serde(rename = "goalId")]
    pub goal_id: String,
    #[serde(rename = "recordedAt")]
    pub recorded_at: i64,
    pub value: f64,
    pub note: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct GoalTagPush {
    #[serde(rename = "tagId")]
    pub tag_id: String,
    #[serde(rename = "goalId")]
    pub goal_id: String,
}

pub async fn push_notes(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let content_type = headers.get(axum::http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let server_time = db::server_time_ms();
    if content_type.starts_with("multipart/") {
        let boundary = content_type.split("boundary=").nth(1).unwrap_or("").trim_matches('"').trim().to_string();
        if boundary.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message":"missing boundary"}))).into_response();
        }
        let (note_audio_opt, audio_bytes) = parse_multipart_note_audio(&body, &boundary);
        let note_audio: NoteAudioPush = if let Some(s) = note_audio_opt {
            match serde_json::from_str(&s) {
                Ok(v) => v,
                Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message": e.to_string()}))).into_response(),
            }
        } else {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message":"missing noteAudio field"}))).into_response();
        };
        let conn = state.db.lock().unwrap();
        if let Some(bytes) = audio_bytes {
            let base = state.config.db_path.parent().unwrap_or(std::path::Path::new("/tmp"));
            let dir = base.join("note_audio");
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(format!("{}.mp3", note_audio.id));
            let _ = std::fs::write(&path, &bytes);
            let stored_path = path.to_string_lossy().to_string();
            conn.execute(
                "INSERT INTO note_audio (id, note_id, audio_path, duration_ms, position, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL) ON CONFLICT(id) DO UPDATE SET note_id=excluded.note_id, audio_path=excluded.audio_path, duration_ms=excluded.duration_ms, position=excluded.position, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
                rusqlite::params![note_audio.id, note_audio.note_id, stored_path, note_audio.duration_ms, note_audio.position, note_audio.created_at, server_time],
            ).unwrap();
        } else {
            conn.execute(
                "INSERT INTO note_audio (id, note_id, audio_path, duration_ms, position, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL) ON CONFLICT(id) DO UPDATE SET note_id=excluded.note_id, audio_path=excluded.audio_path, duration_ms=excluded.duration_ms, position=excluded.position, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
                rusqlite::params![note_audio.id, note_audio.note_id, note_audio.audio_path, note_audio.duration_ms, note_audio.position, note_audio.created_at, server_time],
            ).unwrap();
        }
        return (StatusCode::OK, Json(serde_json::json!({"server_time": server_time}))).into_response();
    }
    let payload: NotesPush = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message": e.to_string()}))).into_response(),
    };
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    for n in payload.notes {
        tx.execute(
            "INSERT INTO notes (id, title, body, updated_at, created_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL) ON CONFLICT(id) DO UPDATE SET title=excluded.title, body=excluded.body, updated_at=excluded.updated_at, created_at=excluded.created_at, deleted_at=NULL",
            rusqlite::params![n.id, n.title, n.body, n.updated_at, n.created_at],
        )
        .unwrap();
    }
    for nt in payload.note_tags {
        tx.execute(
            "INSERT OR IGNORE INTO tags (id, name, created_at, updated_at) VALUES (?1, ?1, ?2, ?2)",
            rusqlite::params![nt.tag_id, server_time],
        )
        .unwrap();
        tx.execute(
            "INSERT OR IGNORE INTO note_tags (tag_id, note_id) VALUES (?1, ?2)",
            rusqlite::params![nt.tag_id, nt.note_id],
        )
        .unwrap();
    }
    for na in payload.note_audio {
        tx.execute(
            "INSERT INTO note_audio (id, note_id, audio_path, duration_ms, position, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL) ON CONFLICT(id) DO UPDATE SET note_id=excluded.note_id, audio_path=excluded.audio_path, duration_ms=excluded.duration_ms, position=excluded.position, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![na.id, na.note_id, na.audio_path, na.duration_ms, na.position, na.created_at, server_time],
        )
        .unwrap();
    }
    for id in payload.deleted {
        let _ = tx.execute("UPDATE notes SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE note_audio SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE note_tags SET deleted_at=?1 WHERE tag_id=?2 OR note_id=?2", rusqlite::params![server_time, id]);
    }
    tx.commit().unwrap();
    (StatusCode::OK, Json(serde_json::json!({"server_time": server_time}))).into_response()
}

fn parse_multipart_note_audio(body: &[u8], boundary: &str) -> (Option<String>, Option<Vec<u8>>) {
    let boundary_bytes = format!("--{boundary}").into_bytes();
    let parts = split_bytes(body, &boundary_bytes);
    let mut note_audio_opt=None; let mut audio_opt=None;
    for part in parts {
        let text = String::from_utf8_lossy(part);
        if text.contains("name=\"noteAudio\"") || text.contains("name=noteAudio") {
            if let Some(start) = find_double_crlf(part) {
                let json_bytes = &part[start..];
                let end = find_boundary_end(json_bytes);
                let s = String::from_utf8_lossy(&json_bytes[..end]).trim().trim_matches('\r').trim_matches('\n').to_string();
                if !s.is_empty() { note_audio_opt=Some(s); }
            }
        } else if text.contains("name=\"audio\"") || text.contains("name=audio") {
            if let Some(start) = find_double_crlf(part) {
                let pic_bytes = &part[start..];
                let end = find_boundary_end(pic_bytes);
                audio_opt=Some(pic_bytes[..end].to_vec());
            }
        }
    }
    (note_audio_opt, audio_opt)
}
fn split_bytes<'a>(data: &'a [u8], pattern: &[u8]) -> Vec<&'a [u8]> {
    let mut res=Vec::new(); let mut start=0; let mut i=0;
    while i+pattern.len() <= data.len() {
        if &data[i..i+pattern.len()] == pattern { res.push(&data[start..i]); i+=pattern.len(); start=i; } else { i+=1; }
    }
    if start < data.len() { res.push(&data[start..]); }
    res
}
fn find_double_crlf(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|w| w==b"\r\n\r\n").map(|p| p+4).or_else(|| data.windows(2).position(|w| w==b"\n\n").map(|p| p+2))
}
fn find_boundary_end(data: &[u8]) -> usize {
    if let Some(pos)=data.windows(4).position(|w| w==b"\r\n--") { pos } else if let Some(pos)=data.windows(2).position(|w| w==b"\n-") { pos } else { data.len() }
}

pub async fn push_tasks(
    State(state): State<AppState>,
    Json(payload): Json<TasksPush>,
) -> impl IntoResponse {
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    for t in payload.tasks {
        tx.execute(
            "INSERT INTO tasks (id, date, title, done, task_status, carry_over, sort_order, due_date, due_time_minutes, start_time_minutes, end_time_minutes, notes, workout_id, workout_template_id, recurrence, series_id, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, NULL) ON CONFLICT(id) DO UPDATE SET date=excluded.date, title=excluded.title, done=excluded.done, task_status=excluded.task_status, carry_over=excluded.carry_over, sort_order=excluded.sort_order, due_date=excluded.due_date, due_time_minutes=excluded.due_time_minutes, start_time_minutes=excluded.start_time_minutes, end_time_minutes=excluded.end_time_minutes, notes=excluded.notes, workout_id=excluded.workout_id, workout_template_id=excluded.workout_template_id, recurrence=excluded.recurrence, series_id=excluded.series_id, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![t.id, t.date, t.title, t.done, t.task_status, if t.carry_over {1} else {0}, t.sort_order, t.due_date, t.due_time_minutes, t.start_time_minutes, t.end_time_minutes, t.notes, t.workout_id, t.workout_template_id, t.recurrence, t.series_id, t.created_at, server_time],
        ).unwrap();
    }
    for tt in payload.task_tags {
        tx.execute(
            "INSERT OR IGNORE INTO tags (id, name, created_at, updated_at) VALUES (?1, ?1, ?2, ?2)",
            rusqlite::params![tt.tag_id, server_time],
        )
        .unwrap();
        tx.execute(
            "INSERT OR IGNORE INTO task_tags (tag_id, task_id) VALUES (?1, ?2)",
            rusqlite::params![tt.tag_id, tt.task_id],
        )
        .unwrap();
    }
    for id in payload.deleted {
        let _ = tx.execute("UPDATE tasks SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE task_tags SET deleted_at=?1 WHERE tag_id=?2 OR task_id=?2", rusqlite::params![server_time, id]);
    }
    tx.commit().unwrap();
    Json(SyncAck { server_time })
}

pub async fn push_goals(
    State(state): State<AppState>,
    Json(payload): Json<GoalsPush>,
) -> impl IntoResponse {
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    for g in payload.goals {
        tx.execute(
            "INSERT INTO goals (id, title, description, start_date, end_date, status, target_type, target_value, baseline_value, unit, reminder_enabled, reminder_time_minutes, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL) ON CONFLICT(id) DO UPDATE SET title=excluded.title, description=excluded.description, start_date=excluded.start_date, end_date=excluded.end_date, status=excluded.status, target_type=excluded.target_type, target_value=excluded.target_value, baseline_value=excluded.baseline_value, unit=excluded.unit, reminder_enabled=excluded.reminder_enabled, reminder_time_minutes=excluded.reminder_time_minutes, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![g.id, g.title, g.description, g.start_date, g.end_date, g.status, g.target_type, g.target_value, g.baseline_value, g.unit, if g.reminder_enabled {1} else {0}, g.reminder_time_minutes, g.created_at, g.updated_at],
        ).unwrap();
    }
    for e in payload.goal_progress_entries {
        tx.execute(
            "INSERT INTO goal_progress_entries (id, goal_id, recorded_at, value, note, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL) ON CONFLICT(id) DO UPDATE SET goal_id=excluded.goal_id, recorded_at=excluded.recorded_at, value=excluded.value, note=excluded.note, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![e.id, e.goal_id, e.recorded_at, e.value, e.note, e.created_at, server_time],
        ).unwrap();
    }
    for gt in payload.goal_tags {
        tx.execute(
            "INSERT OR IGNORE INTO tags (id, name, created_at, updated_at) VALUES (?1, ?1, ?2, ?2)",
            rusqlite::params![gt.tag_id, server_time],
        )
        .unwrap();
        tx.execute(
            "INSERT OR IGNORE INTO goal_tags (tag_id, goal_id) VALUES (?1, ?2)",
            rusqlite::params![gt.tag_id, gt.goal_id],
        )
        .unwrap();
    }
    for id in payload.deleted {
        let _ = tx.execute("UPDATE goals SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE goal_progress_entries SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE goal_tags SET deleted_at=?1 WHERE tag_id=?2 OR goal_id=?2", rusqlite::params![server_time, id]);
    }
    tx.commit().unwrap();
    Json(SyncAck { server_time })
}

#[derive(Deserialize)]
pub struct MealsPush {
    #[serde(default)]
    pub meals: Vec<MealPush>,
    #[serde(rename = "mealIngredients", default)]
    pub meal_ingredients: Vec<MealIngredientPush>,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Deserialize)]
pub struct MealPush {
    pub id: String,
    pub name: String,
    #[serde(rename = "eatenAt")]
    pub eaten_at: i64,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct MealIngredientPush {
    pub id: String,
    #[serde(rename = "mealId")]
    pub meal_id: String,
    #[serde(rename = "ingredientId")]
    pub ingredient_id: String,
    pub grams: f64,
}

#[derive(Deserialize)]
pub struct TransactionsPush {
    #[serde(default)]
    pub transactions: Vec<TransactionPush>,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Deserialize)]
pub struct TransactionPush {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub amount: f64,
    #[serde(rename = "currencyCode")]
    pub currency_code: String,
    #[serde(rename = "amountBase")]
    pub amount_base: f64,
    #[serde(rename = "rateUsed")]
    pub rate_used: f64,
    #[serde(rename = "accountId")]
    pub account_id: Option<String>,
    #[serde(rename = "toAccountId")]
    pub to_account_id: Option<String>,
    pub category: Option<String>,
    pub date: i64,
    pub note: Option<String>,
    #[serde(rename = "receiptId")]
    pub receipt_id: Option<String>,
    #[serde(rename = "isDraft")]
    pub is_draft: bool,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct BudgetAccountsPush {
    #[serde(default)]
    pub accounts: Vec<AccountPush>,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Deserialize)]
pub struct AccountPush {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(rename = "openingBalance")]
    pub opening_balance: f64,
    pub note: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

pub async fn push_meals(
    State(state): State<AppState>,
    Json(payload): Json<MealsPush>,
) -> impl IntoResponse {
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    for m in payload.meals {
        tx.execute(
            "INSERT INTO meals (id, name, eaten_at, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL) ON CONFLICT(id) DO UPDATE SET name=excluded.name, eaten_at=excluded.eaten_at, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![m.id, m.name, m.eaten_at, m.created_at, server_time],
        )
        .unwrap();
    }
    for mi in payload.meal_ingredients {
        tx.execute(
            "INSERT INTO meal_ingredients (id, meal_id, ingredient_id, grams, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL) ON CONFLICT(id) DO UPDATE SET meal_id=excluded.meal_id, ingredient_id=excluded.ingredient_id, grams=excluded.grams, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![mi.id, mi.meal_id, mi.ingredient_id, mi.grams, server_time],
        )
        .unwrap();
    }
    for id in payload.deleted {
        let _ = tx.execute("UPDATE meals SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
        let _ = tx.execute("UPDATE meal_ingredients SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
    }
    tx.commit().unwrap();
    Json(SyncAck { server_time })
}

pub async fn push_transactions(
    State(state): State<AppState>,
    Json(payload): Json<TransactionsPush>,
) -> impl IntoResponse {
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    for t in payload.transactions {
        tx.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, account_id, to_account_id, category, date, note, receipt_id, is_draft, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, NULL) ON CONFLICT(id) DO UPDATE SET type=excluded.type, amount=excluded.amount, currency_code=excluded.currency_code, amount_base=excluded.amount_base, rate_used=excluded.rate_used, account_id=excluded.account_id, to_account_id=excluded.to_account_id, category=excluded.category, date=excluded.date, note=excluded.note, receipt_id=excluded.receipt_id, is_draft=excluded.is_draft, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![t.id, t.type_, t.amount, t.currency_code, t.amount_base, t.rate_used, t.account_id, t.to_account_id, t.category, t.date, t.note, t.receipt_id, if t.is_draft {1} else {0}, t.created_at, server_time],
        ).unwrap();
    }
    for id in payload.deleted {
        let _ = tx.execute("UPDATE transactions SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
    }
    tx.commit().unwrap();
    Json(SyncAck { server_time })
}

pub async fn push_budget_accounts(
    State(state): State<AppState>,
    Json(payload): Json<BudgetAccountsPush>,
) -> impl IntoResponse {
    let server_time = db::server_time_ms();
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    for a in payload.accounts {
        tx.execute(
            "INSERT INTO accounts (id, name, type, opening_balance, note, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL) ON CONFLICT(id) DO UPDATE SET name=excluded.name, type=excluded.type, opening_balance=excluded.opening_balance, note=excluded.note, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
            rusqlite::params![a.id, a.name, a.type_, a.opening_balance, a.note, a.created_at, server_time],
        ).unwrap();
    }
    for id in payload.deleted {
        let _ = tx.execute("UPDATE accounts SET deleted_at=?1, updated_at=?1 WHERE id=?2", rusqlite::params![server_time, id]);
    }
    tx.commit().unwrap();
    Json(SyncAck { server_time })
}
