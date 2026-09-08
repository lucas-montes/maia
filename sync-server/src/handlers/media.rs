use std::path::PathBuf;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use crate::server::AppState;

fn media_dir(state: &AppState) -> PathBuf {
    let base = state
        .config
        .db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("/tmp"));
    base.join("exercise_media")
}

pub async fn get_image(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if !is_safe_id(&id) {
        return (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"message":"invalid id"}))).into_response();
    }
    let path = media_dir(&state).join(format!("{id}.jpg"));
    serve_file(path, "image/jpeg").await
}

pub async fn get_video(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if !is_safe_id(&id) {
        return (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"message":"invalid id"}))).into_response();
    }
    let path = media_dir(&state).join(format!("{id}.mp4"));
    serve_file(path, "video/mp4").await
}

pub async fn get_media(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if id.ends_with(".jpg") {
        let base = id.trim_end_matches(".jpg");
        if !is_safe_id(base) {
            return (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"message":"invalid id"}))).into_response();
        }
        let path = media_dir(&state).join(format!("{base}.jpg"));
        return serve_file(path, "image/jpeg").await;
    } else if id.ends_with(".mp4") {
        let base = id.trim_end_matches(".mp4");
        if !is_safe_id(base) {
            return (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"message":"invalid id"}))).into_response();
        }
        let path = media_dir(&state).join(format!("{base}.mp4"));
        return serve_file(path, "video/mp4").await;
    }
    (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"message":"not found"}))).into_response()
}

async fn serve_file(path: PathBuf, content_type: &str) -> Response {
    match tokio::fs::read(&path).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(bytes))
            .unwrap(),
        Err(_) => (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"message":"not found"}))).into_response(),
    }
}

fn is_safe_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
}
