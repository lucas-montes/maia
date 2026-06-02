use std::borrow::Cow;

use chrono::NaiveDateTime;
use maia_macros::Crud;
use serde::{Deserialize, Serialize};

use crate::{
    finances::{Currency, Money},
    purchases::schema,
};

#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "receipts_metadata"]
pub struct ReceiptMetadata<'a> {
    #[skip_crud]
    id: Option<i64>,
    receipt_id: i64,
    image: Cow<'a, str>,
    raw: Vec<u8>,
}

impl<'a> ReceiptMetadata<'a> {
    pub fn new(receipt_id: i64, image: Cow<'a, str>, raw: Vec<u8>) -> Self {
        Self {
            id: None,
            receipt_id,
            image,
            raw,
        }
    }
}

/// The purcase base model representing an entry in a receipt
#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "receipts"]
pub struct Receipt {
    #[skip_crud]
    id: Option<i64>,
    store: String,
    date: NaiveDateTime,
    total: Money,
    currency: Currency,
}

impl From<schema::Receipt> for Receipt {
    fn from(receipt: schema::Receipt) -> Self {
        Self {
            id: None,
            store: receipt.store,
            date: NaiveDateTime::parse_from_str(&receipt.date, "%Y-%m-%d %H:%M:%S")
                .unwrap_or_else(|_| chrono::Local::now().naive_local()),
            total: Money::from(receipt.total),
            currency: receipt.currency.into(),
        }
    }
}

/// The purcase base model representing an entry in a receipt
#[derive(Debug, Serialize, Deserialize, Crud)]
#[table_name = "purchases"]
pub struct Purchase {
    #[skip_crud]
    id: Option<i64>,
    aliment_id: Option<i64>,
    product_name: String,
    price: Money,
    currency: Currency,
}

//TODO: maybe I want a one to many for receipt -> purchases, and purchases maybe should be generic to keep track of any purchase?
