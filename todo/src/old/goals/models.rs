use crate::todo::utils::Priority;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub id: i16,
    pub title: String,
    pub why: String,
    pub how: String,
    pub notes: String,
    pub priority: Priority,
    pub projects: HashSet<i16>,
    pub hours_per_week: f64,
    pub horizon: i8,
    pub created_at: String,
}

impl Goal {
    pub fn new(
        title: String,
        why: String,
        how: String,
        notes: String,
        priority: Priority,
        horizon: i8,
    ) -> Self {
        Goal {
            id: 0,
            title,
            why,
            how,
            notes,
            priority,
            horizon,
            projects: HashSet::new(),
            hours_per_week: 0.0,
            created_at: Local::now().naive_local().to_string(),
        }
    }

    pub fn set_id(&mut self, id: i16) -> &Self {
        self.id = id;
        self
    }
}
