use crate::todo::utils::{notify, Day, Priority};

use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i16,
    pub title: String,
    pub description: String,
    pub start: String,
    pub end: String,
    pub priority: Priority,
    pub done: bool,
    pub days: Vec<Day>,
    pub after: Option<i16>,
}

impl Task {
    pub fn new(
        title: String,
        description: String,
        start: String,
        end: String,
        priority: Priority,
        after: Option<i16>,
        days: Vec<Day>,
    ) -> Self {
        Task {
            id: 0,
            title,
            description,
            start,
            end,
            priority,
            after,
            done: false,
            days,
        }
    }

    pub fn set_id(&mut self, id: i16) -> &Self {
        self.id = id;
        self
    }

    pub fn duration(&self) -> f64 {
        let m = match self.days.len() {
            0 => 1.0,
            _ => self.days.len() as f64,
        };

        self.duration_in_hours() * 24.0 * m
    }

    pub fn is_one_off(&self) -> bool {
        self.days.is_empty()
    }

    fn duration_in_hours(&self) -> f64 {
        let start_date = match NaiveTime::parse_from_str(&self.start, "%H:%M") {
            Ok(value) => value,
            Err(err) => panic!("oupsi, {:?}", err),
        };
        let end_date = match NaiveTime::parse_from_str(&self.end, "%H:%M") {
            Ok(value) => value,
            Err(err) => panic!("oupsi, {:?}", err),
        };
        let duration = end_date.signed_duration_since(start_date);
        duration.num_hours() as f64 + (duration.num_minutes() as f64 / 60.0)
    }

    pub async fn to_notification(&self) {
        let due_date: &str = if self.start.is_empty() { &self.end } else { &self.start };
        let summary = format!("The task {} is due {due_date}", &self.title);
        notify(&summary, &self.description, self.priority.to_dialog_alarm()).await;
    }
}
