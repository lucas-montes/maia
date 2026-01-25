use ai::Vllm;
// use base64::{Engine as _, engine::general_purpose};
// use chrono::{DateTime, NaiveDateTime};
// use serde::Deserialize;
use std::path::PathBuf;

use crate::purchases::models::ReceiptMetadata;

use super::{models, schema};

pub async fn handle_receipt_file(
    model: &impl Vllm,
    conn: &rusqlite::Connection,
    image_path: PathBuf,
) {
    tracing::info!(?image_path, "Handling receipt file");

    let receipt = match model.extract_image::<schema::Receipt>(&image_path).await {
        Ok(Some(receipt)) => receipt,
        Ok(None) => {
            tracing::info!("Receipt is empty");
            return;
        }
        Err(err) => {
            tracing::error!(?err, "Failed to extract receipt");
            return;
        }
    };
    tracing::info!(?receipt, "Extracted receipt");

    // Keep a raw json representation of the image
    let raw = serde_json::to_vec(&receipt).unwrap_or_default();
    //TODO: upload image

    // Save the image correctly
    let entity = models::Receipt::from(receipt);

    let Ok(receipt_id) = models::Receipt::create(conn, &entity) else {
        tracing::error!("Failed to create receipt in database");
        return;
    };

    // Then create the items purchased

    ReceiptMetadata::new(receipt_id, image_path.to_string_lossy(), raw);
}
