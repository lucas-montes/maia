use std::collections::HashSet;

use crate::todo::utils::Priority;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: i16,
    pub title: String,
    pub description: String,
    pub start: String,
    pub end: String,
    pub notes: String,
    pub priority: Priority,
    pub accomplished: bool,
    pub tasks: HashSet<i16>,
    pub hours_per_week: f64,
}

impl Project {
    pub fn new(
        title: String,
        description: String,
        start: String,
        end: String,
        notes: String,
        priority: Priority,
    ) -> Self {
        Project {
            id: 0,
            title,
            description,
            start,
            end,
            notes,
            priority,
            accomplished: false,
            tasks: HashSet::new(),
            hours_per_week: 0.0,
        }
    }

    pub fn set_id(&mut self, id: i16) -> &Self {
        self.id = id;
        self
    }

    pub fn in_stand_by(&self) -> bool {
        self.start.is_empty()
    }
}
