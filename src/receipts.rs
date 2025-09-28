use base64::{Engine as _, engine::general_purpose};
use chrono::{DateTime, NaiveDateTime};
use serde::Deserialize;
use std::path::PathBuf;

use crate::state::State;

pub async fn handle_receipt_file(state: &State, image_path: PathBuf) {
    tracing::info!(image_path=?image_path, "Handling receipt file");
    let Ok(img) = std::fs::read(image_path).map(|bytes| general_purpose::STANDARD.encode(&bytes))
    else {
        tracing::error!("Failed to read image file");
        return;
    };
    let model = match state.model() {
        Some(m) => m,
        None => {
            tracing::error!("No model configured");
            return;
        }
    };

    let receipt = match model.extract_receipt::<Receipt>(img).await {
        Ok(receipt) => receipt,
        Err(err) => {
            tracing::error!(err=?err, "Failed to extract receipt");
            return;
        }
    };

    tracing::info!(?receipt, "Extracted receipt");
}

#[derive(Debug, Deserialize)]
struct Receipt {
    store: String,
    date: Option<DateTime<chrono::Utc>>,
    total: f32,
    currency: String,
    products: Vec<Product>,
    discounts: Vec<Discount>,
}
#[derive(Debug, Deserialize)]
struct Discount {
    description: String,
    amount: f32,
}
#[derive(Debug, Deserialize)]
struct Product {
    name: String,
    price: f32,
    currency: String,
}
