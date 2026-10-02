//! Receipt OCR pipeline: receive an uploaded image, parse it with the vision
//! model in the background, and expose the result for the client to poll.
//!
//! The upload returns `202` immediately and the parse runs detached, because a
//! vision call takes seconds and the client needs to stay responsive (and
//! survive being backgrounded) while it happens. The client polls
//! `GET /receipt-pictures/:id/parse` until `status` reaches a terminal value.

use std::path::PathBuf;

use ai::Vllm;
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tokio::io::AsyncWriteExt;

use super::media::{content_type_for_ext, is_safe_id, serve_file};
use crate::{db, error::error_response, receipt_ocr::ParsedReceipt, server::AppState};

/// Directory (relative to the DB) holding receipt pictures, mirroring how
/// `seed.rs` stores `exercise_media/<stem>.jpg`.
const PICTURE_SUBDIR: &str = "receipt_pictures";

/// `receipts.upload_status` codes, mirroring the client's `ReceiptStatus`.
/// `0 local`, `1 uploading`, `2 uploaded`, `3 error`, `4 parsing`.
const UPLOAD_STATUS_UPLOADED: i64 = 2;
const UPLOAD_STATUS_PARSING: i64 = 4;

fn db_dir(state: &AppState) -> PathBuf {
    state
        .config
        .db_path
        .parent()
        .map(|p| p.to_path_buf())
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn picture_dir(state: &AppState) -> PathBuf {
    db_dir(state).join(PICTURE_SUBDIR)
}

/// Maps a client-supplied MIME to a stored extension. Unknown image types fall
/// back to `.jpg`, which is what the model is told to expect.
fn extension_for_mime(mime: &str, file_name: Option<&str>) -> (String, &'static str) {
    let normalized = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let ext = match normalized.as_str() {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/heic" => "heic",
        "image/heif" => "heif",
        _ => match file_name.and_then(|n| n.rsplit('.').next()) {
            Some(e) if is_safe_id(e) && e.len() <= 5 => e,
            _ => "jpg",
        },
    };
    (ext.to_string(), content_type_for_ext(ext))
}

/// `POST /receipt-pictures/upload`
///
/// Accepts `multipart/form-data` with a `picture` file part and an optional
/// `receiptId` text part. Persists the image, records the receipt row, then
/// spawns the parse job and returns `202`.
pub async fn upload_receipt_picture(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let model = match &state.ocr {
        Some(m) => m.clone(),
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "ocr not configured: set a model api key on the server",
            )
            .into_response()
        }
    };

    let mut requested_id: Option<String> = None;
    let mut picture: Option<(Vec<u8>, String, &'static str)> = None;

    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return error_response(StatusCode::BAD_REQUEST, format!("malformed body: {e}"))
                    .into_response()
            }
        };
        let name = field.name().unwrap_or("").to_string();

        if name == "receiptId" || name == "receipt_id" {
            match field.text().await {
                Ok(t) => requested_id = Some(t.trim().to_string()),
                Err(e) => {
                    return error_response(StatusCode::BAD_REQUEST, format!("receiptId: {e}"))
                        .into_response()
                }
            }
        } else if name == "picture" || name == "image" || name == "file" {
            let declared_mime = field.content_type().unwrap_or("").to_string();
            let file_name = field.file_name().map(|s| s.to_string());
            let (ext, mime) = extension_for_mime(&declared_mime, file_name.as_deref());

            let mut bytes = Vec::new();
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => bytes.extend_from_slice(&chunk),
                    Ok(None) => break,
                    Err(e) => {
                        return error_response(StatusCode::BAD_REQUEST, format!("picture: {e}"))
                            .into_response()
                    }
                }
            }
            if bytes.is_empty() {
                return error_response(StatusCode::BAD_REQUEST, "picture part was empty").into_response();
            }
            picture = Some((bytes, ext, mime));
        } else {
            // Drain unknown fields so the stream stays in sync.
            while let Ok(Some(_)) = field.chunk().await {}
        }
    }

    let Some((bytes, ext, mime)) = picture else {
        return error_response(StatusCode::BAD_REQUEST, "missing `picture` field").into_response();
    };

    // The client normally supplies its own UUID so the row id stays stable
    // across phone and server (the `?since=` pull depends on it). Fall back to a
    // server-generated v7 when absent.
    let id = match requested_id {
        Some(v) if is_safe_id(&v) => v,
        Some(_) => return error_response(StatusCode::BAD_REQUEST, "invalid receiptId").into_response(),
        None => uuid::Uuid::now_v7().to_string(),
    };

    let dir = picture_dir(&state);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("cannot create {}: {e}", dir.display()),
        )
        .into_response();
    }
    let file_name = format!("{id}.{ext}");
    if let Err(e) = write_file_atomic(&dir.join(&file_name), &bytes).await {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("cannot store image: {e}"))
            .into_response();
    }

    // Relative, not absolute: this value is shipped to clients by `pull_receipts`
    // and must not leak the server's filesystem layout.
    let relative = format!("{PICTURE_SUBDIR}/{file_name}");
    let now = db::server_time_ms();
    let result = {
        let conn = state.db.lock().unwrap();
        conn.execute(
            "INSERT INTO receipts (id, local_path, remote_path, upload_status, parsed, parsed_json, parse_error, transaction_id, created_at, updated_at, deleted_at)
             VALUES (?1, '', ?2, ?3, 0, NULL, NULL, NULL, ?4, ?5, NULL)
             ON CONFLICT(id) DO UPDATE SET remote_path = excluded.remote_path,
                                           upload_status = excluded.upload_status,
                                           parsed = 0,
                                           parsed_json = NULL,
                                           parse_error = NULL,
                                           updated_at = excluded.updated_at,
                                           deleted_at = NULL",
            rusqlite::params![
                id,
                relative,
                UPLOAD_STATUS_PARSING,
                now,
                now
            ],
        )
    };
    if let Err(e) = result {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("db: {e}")).into_response();
    }

    // Detached: the model call outlives this request. `DbPool` and `Model` are
    // both `Arc`-backed, so cloning them here is cheap and `'static`.
    let db_pool = state.db.clone();
    let job_id = id.clone();
    tokio::spawn(async move {
        run_ocr(db_pool, model, job_id, bytes, mime.to_string()).await;
    });

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "id": id,
            "status": "parsing",
            "server_time": now,
        })),
    )
        .into_response()
}

/// Runs the vision parse and records the outcome on the receipt row.
///
/// Always writes a terminal state, so a client polling `/:id/parse` can never
/// hang on `parsing` forever.
async fn run_ocr(
    db_pool: db::DbPool,
    model: std::sync::Arc<ai::Model>,
    id: String,
    bytes: Vec<u8>,
    mime: String,
) {
    tracing::info!(id = %id, bytes = bytes.len(), mime = %mime, "starting receipt parse");
    let now = db::server_time_ms();

    let outcome = match model.extract_image_bytes::<ParsedReceipt>(bytes, &mime).await {
        Ok(Some(parsed)) => {
            let json_text = serde_json::to_string(&parsed).unwrap_or_default();
            let conn = db_pool.lock().unwrap();
            let _ = conn.execute(
                "UPDATE receipts SET parsed = 1, parsed_json = ?1, parse_error = NULL, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![json_text, now, id],
            );
            Ok(())
        }
        Ok(None) => {
            tracing::warn!(id = %id, "model returned no content");
            Err("model returned no content".to_string())
        }
        Err(e) => Err(format!("{e:?}")),
    };

    if let Err(message) = outcome {
        tracing::error!(id = %id, error = %message, "receipt parse failed");
        let conn = db_pool.lock().unwrap();
        let _ = conn.execute(
            "UPDATE receipts SET parsed = 0, parse_error = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![message, now, id],
        );
    }
}

/// `GET /receipt-pictures/:id/parse`
///
/// Returns the current parse state. `status` is one of `pending`, `parsing`,
/// `parsed`, `error`; `parsed`/`error` are terminal.
pub async fn get_receipt_parse(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !is_safe_id(&id) {
        return error_response(StatusCode::BAD_REQUEST, "invalid id").into_response();
    }
    let conn = state.db.lock().unwrap();
    let row = conn
        .query_row(
            "SELECT upload_status, parsed, parsed_json, parse_error FROM receipts WHERE id = ?1 AND deleted_at IS NULL",
            rusqlite::params![id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .ok();

    let Some((upload_status, parsed, parsed_json, parse_error)) = row else {
        return (StatusCode::NOT_FOUND, Json(json!({"message":"not found"}))).into_response();
    };

    let status = if parsed != 0 {
        "parsed"
    } else if parse_error.is_some() {
        "error"
    } else if upload_status == UPLOAD_STATUS_UPLOADED || upload_status == UPLOAD_STATUS_PARSING {
        "parsing"
    } else {
        "pending"
    };

    let parsed_value = parsed_json
        .as_deref()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok());

    (
        StatusCode::OK,
        Json(json!({
            "id": id,
            "status": status,
            "parsed": parsed_value,
            "error": parse_error,
            "server_time": db::server_time_ms(),
        })),
    )
        .into_response()
}

/// `GET /receipt-pictures/:id/picture`
///
/// Serves the stored image. Resolves the extension from the recorded
/// `remote_path`, and follows `archived_path` so a receipt archived from the
/// desktop is still viewable.
pub async fn get_receipt_picture(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !is_safe_id(&id) {
        return error_response(StatusCode::BAD_REQUEST, "invalid id").into_response();
    }
    let base = db_dir(&state);
    // Scoped so the `MutexGuard` cannot live across the `await` below — it is
    // not `Send`, and axum requires handler futures to be.
    let stored = {
        let conn = state.db.lock().unwrap();
        conn.query_row(
            "SELECT remote_path, archived_path FROM receipts WHERE id = ?1 AND deleted_at IS NULL",
            rusqlite::params![id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .ok()
    };

    let Some((remote_path, archived_path)) = stored else {
        return (StatusCode::NOT_FOUND, Json(json!({"message":"not found"}))).into_response();
    };

    // Archive location first: the original file has been moved out from under us.
    let candidate = archived_path
        .or(remote_path)
        .and_then(|p| resolve_stored_path(&base, &p));

    let Some(path) = candidate else {
        return (StatusCode::NOT_FOUND, Json(json!({"message":"not found"}))).into_response();
    };
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "jpg".to_string());
    serve_file(path, content_type_for_ext(&ext)).await
}

/// Joins a stored (relative) path onto the DB directory, refusing anything that
/// would escape it.
fn resolve_stored_path(base: &std::path::Path, stored: &str) -> Option<PathBuf> {
    if stored.is_empty() {
        return None;
    }
    let relative = std::path::Path::new(stored);
    let joined = base.join(relative);
    // Reject `..` traversal and absolute paths smuggled in via the DB.
    if relative.is_absolute() || relative.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return None;
    }
    Some(joined)
}

/// Writes via a temp file + rename so a reader never observes a partial image.
async fn write_file_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = tokio::fs::File::create(&tmp).await?;
        f.write_all(bytes).await?;
        f.flush().await?;
    }
    tokio::fs::rename(&tmp, path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_prefers_declared_mime() {
        assert_eq!(extension_for_mime("image/png", None), ("png".into(), "image/png"));
        assert_eq!(extension_for_mime("image/jpeg", None), ("jpg".into(), "image/jpeg"));
        // Parameters are ignored.
        assert_eq!(extension_for_mime("image/PNG; charset=binary", None).0, "png");
        // Falls back to the filename extension when the mime is unhelpful.
        assert_eq!(extension_for_mime("", Some("receipt.webp")).0, "webp");
        // And to jpg when neither helps.
        assert_eq!(extension_for_mime("application/octet-stream", None).0, "jpg");
    }

    #[test]
    fn extension_rejects_hostile_filenames() {
        // A traversal attempt in the extension position must not escape.
        let (ext, _) = extension_for_mime("", Some("x../../evil"));
        assert!(is_safe_id(&ext), "unexpected extension {ext}");
    }

    #[test]
    fn stored_path_resolution_blocks_traversal() {
        let base = std::path::Path::new("/srv/data");
        assert_eq!(
            resolve_stored_path(base, "receipt_pictures/r1.jpg"),
            Some(PathBuf::from("/srv/data/receipt_pictures/r1.jpg"))
        );
        assert_eq!(resolve_stored_path(base, ""), None);
        assert_eq!(resolve_stored_path(base, "../escape.jpg"), None);
        assert_eq!(resolve_stored_path(base, "a/../../escape.jpg"), None);
        assert_eq!(resolve_stored_path(base, "/etc/passwd"), None);
    }
}