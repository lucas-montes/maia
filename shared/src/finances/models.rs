// Database models for the finance module

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::finances::{Money, outils::Currency};

#[derive(Debug, Serialize, Deserialize)]
pub struct Investment {
    id: Option<i64>,
    name: String,
    amount: Money,
    currency: Currency,
    invested_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Transaction {
    id: Option<i64>,
    description: String,
    amount: Money,
    transaction_type: String,
    occurred_at: NaiveDateTime,
}
