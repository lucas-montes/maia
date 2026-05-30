use chrono::{DateTime, Utc};

enum Status {
    NotStarted,
    InProgress,
    Completed,
}

enum Priority {
    Low,
    Medium,
    High,
}

struct Task {
    completion: Status,
    priotity: Priority,
    completion_date: Option<DateTime<Utc>>,
    creation_date: Option<DateTime<Utc>>,
    description: String,
    project: Option<String>,
    context: Option<String>,
    due_date: Option<DateTime<Utc>>,
}
