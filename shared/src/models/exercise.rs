use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exercise {
    id: Option<i64>,
    name: String,
    category: ExerciseCategory,
    description: Option<String>,
    created_at: DateTime<Utc>,
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


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkoutSession {
    id: Option<i64>,
    date: NaiveDate,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    notes: Option<String>,
    created_at: DateTime<Utc>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseSet {
    id: Option<i64>,
    workout_session_id: i64,
    exercise_id: i64,
    set_number: i32,
    reps: i32,
    weight: Option<f64>,
    duration: Option<i32>,
    distance: Option<f64>,
    notes: Option<String>,
    created_at: DateTime<Utc>,
}
