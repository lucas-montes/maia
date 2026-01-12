use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};


/// This is the generic for data fetched from the database
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record<T>{
    id: u64,
    data: T,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    id: Option<i64>,
    shop_name: String,
    shop_address: Option<String>,
    purchase_date: NaiveDate,
    total_amount: f64,
    currency: String,
    receipt_image_path: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptItem {
    id: Option<i64>,
    receipt_id: i64,
    product_id: Option<i64>,
    product_name: String,
    quantity: f64,
    unit: String,
    unit_price: f64,
    total_price: f64,
    created_at: DateTime<Utc>,
}
