use std::path::PathBuf;

use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use crate::server::AppState;

/// Name the uploaded database is stored under.
const BACKUP_FILE: &str = "fitfat.sqlite";

fn backup_path(state: &AppState) -> PathBuf {
    state
        .config
        .db_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("/tmp"))
        .join("backup")
        .join(BACKUP_FILE)
}

/// Stores a database uploaded by the phone.
///
/// Writes to a sibling temporary file and renames it into place, because a
/// partial write would otherwise leave a truncated file that `GET /backup/latest`
/// happily serves as if it were a valid database.
pub async fn post_backup(State(state): State<AppState>, body: axum::body::Bytes) -> Response {
    let path = backup_path(&state);
    let Some(parent) = path.parent() else {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "backup path has no parent");
    };
    if let Err(e) = std::fs::create_dir_all(parent) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, &format!("creating {}: {e}", parent.display()));
    }

    // Same directory as the target so the rename stays within one filesystem and
    // is therefore atomic.
    let tmp = path.with_extension("sqlite.tmp");
    if let Err(e) = tokio::fs::write(&tmp, &body).await {
        return error(StatusCode::INTERNAL_SERVER_ERROR, &format!("writing backup: {e}"));
    }
    if let Err(e) = tokio::fs::rename(&tmp, &path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return error(StatusCode::INTERNAL_SERVER_ERROR, &format!("finalising backup: {e}"));
    }

    tracing::info!(bytes = body.len(), path = %path.display(), "stored backup");
    (StatusCode::OK, axum::Json(serde_json::json!({
        "server_time": crate::db::server_time_ms(),
        "bytes": body.len(),
    })))
    .into_response()
}

/// Returns the most recently stored backup, if any.
pub async fn get_backup(State(state): State<AppState>) -> Response {
    let path = backup_path(&state);
    match tokio::fs::read(&path).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/vnd.sqlite3")
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{BACKUP_FILE}\""),
            )
            .body(Body::from(bytes))
            .expect("static response is well formed"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            error(StatusCode::NOT_FOUND, "no backup")
        }
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &format!("reading backup: {e}")),
    }
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(serde_json::json!({ "message": message }))).into_response()
}