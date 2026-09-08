use std::path::PathBuf;

use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use crate::server::AppState;

fn backup_path(state: &AppState) -> PathBuf {
    let base = state
        .config
        .db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("/tmp"));
    base.join("backup").join("fitfat.sqlite")
}

pub async fn post_backup(State(state): State<AppState>, body: axum::body::Bytes) -> impl IntoResponse {
    let path = backup_path(&state);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match tokio::fs::write(&path, &body).await {
        Ok(_) => (StatusCode::OK, axum::Json(serde_json::json!({"server_time": crate::db::server_time_ms()}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({"message": e.to_string()}))).into_response(),
    }
}

pub async fn get_backup(State(state): State<AppState>) -> Response {
    let path = backup_path(&state);
    match tokio::fs::read(&path).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(bytes))
            .unwrap(),
        Err(_) => (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"message": "no backup"}))).into_response(),
    }
}
