// Database models for the finance module

use serde::{Deserialize, Serialize};
use chrono::NaiveDateTime;
use maia_macros::Crud;

use crate::finances::{Money, outils::Currency};






#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "investments"]
pub struct Investment {
    id: Option<i64>,
    name: String,
    amount: Money,
    currency: Currency,
    invested_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "transactions"]
pub struct Transaction {
    id: Option<i64>,
    description: String,
    amount: Money,
    transaction_type: String,
    occurred_at: NaiveDateTime,
}
