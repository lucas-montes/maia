use std::path::PathBuf;

use crate::state::State;

pub async fn handle_receipt_file(state: &State, image_path: PathBuf) {
    tracing::info!(?image_path, "Handling receipt file");

    let Some(model) = state.model() else {
        tracing::error!("No model configured");
        return;
    };

    // tracing::info!(?receipt, "Extracted receipt");
}
