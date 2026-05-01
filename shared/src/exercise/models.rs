// Database models for the health module

use chrono::NaiveDateTime;
use maia_macros::Crud;
use rusqlite::{ToSql, types::FromSql};
use serde::{Deserialize, Serialize};

/// Database model representing an exercise done by the user
#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "exercises"]
pub struct Exercise {
    id: Option<i64>,
    name: String,
    created_at: NaiveDateTime,
    category: ExerciseCategory,
    met: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExerciseCategory {
    Chest,
    Back,
    Legs,
    Shoulders,
    Arms,
    Core,
    Cardio,
    Other,
}

impl ToSql for ExerciseCategory {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.to_string()))
    }
}

impl From<&str> for ExerciseCategory {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "chest" => ExerciseCategory::Chest,
            "back" => ExerciseCategory::Back,
            "legs" => ExerciseCategory::Legs,
            "shoulders" => ExerciseCategory::Shoulders,
            "arms" => ExerciseCategory::Arms,
            "core" => ExerciseCategory::Core,
            "cardio" => ExerciseCategory::Cardio,
            _ => ExerciseCategory::Other,
        }
    }
}

impl FromSql for ExerciseCategory {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value {
            rusqlite::types::ValueRef::Text(text) => std::str::from_utf8(text)
                .map_err(|_| rusqlite::types::FromSqlError::InvalidType)
                .map(ExerciseCategory::from),
            _ => Err(rusqlite::types::FromSqlError::InvalidType),
        }
    }
}

impl std::fmt::Display for ExerciseCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExerciseCategory::Chest => write!(f, "chest"),
            ExerciseCategory::Back => write!(f, "back"),
            ExerciseCategory::Legs => write!(f, "legs"),
            ExerciseCategory::Shoulders => write!(f, "shoulders"),
            ExerciseCategory::Arms => write!(f, "arms"),
            ExerciseCategory::Core => write!(f, "core"),
            ExerciseCategory::Cardio => write!(f, "cardio"),
            ExerciseCategory::Other => write!(f, "other"),
        }
    }
}

/// Database model representing a body weight measurement
#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "body_weights"]
pub struct BodyWeight {
    id: Option<i64>,
    weight: f32,
    fat: Option<f32>,
    muscle: Option<f32>,
    measured_at: NaiveDateTime,
}

/// Database model representing a seance session
#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "seances"]
pub struct Seance {
    id: Option<i64>,
    start_at: NaiveDateTime,
    end_at: Option<NaiveDateTime>, //TOOD: not sure we need it to be optional
}

/// Database model representing a set within a seance, allowing to have a one to many relationship between seances and exercises
#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "seance_set"]
pub struct SeanceSet {
    id: Option<i64>,
    exercise_id: i32,
    sceance_id: i32,
    sets: i32,
    reps: i32,
    weight: f32,
    rest_time: NaiveDateTime, //TODO: use Duration instead or something like this
    notes: Option<String>,
    start_at: NaiveDateTime,
}
