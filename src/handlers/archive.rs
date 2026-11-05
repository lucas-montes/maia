//! Archiving a parsed receipt: move its image out of the live picture
//! directory into an archive folder, leaving behind a `receipt.json` copy of
//! the parsed data.
//!
//! This lives on the server rather than in the desktop app because it moves
//! files and updates the row that records where they went. With the app opening
//! the database read-only, the server is the only writer, so a move that the
//! server did not perform would leave `archived_path` describing a move that
//! never happened.
//!
//! A file move cannot join a database transaction, so the two steps are ordered
//! rather than atomic: the files move first and the row second, and a failed
//! row update moves them back. That is the only ordering where a failure leaves
//! the receipt usable rather than merely recorded as archived.

use std::path::{Path, PathBuf};

use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::{
    db,
    error::error_response,
    handlers::receipt_ocr::resolve_stored_path,
    server::AppState,
};

/// `error_response` is opaque per call site, so a handler with several early
/// returns needs them all widened to the same concrete type.
fn err(status: StatusCode, message: impl Into<String>) -> Response {
    error_response(status, message).into_response()
}

/// The parsed fields worth keeping beside an archived image, so the folder is
/// still readable if the database is lost or restored from an older backup.
#[derive(Serialize)]
struct ArchiveManifest {
    id: String,
    archived_at: i64,
    source: String,
    upload_status: i64,
    parsed: bool,
    transaction_id: Option<String>,
    /// Stored as a JSON string in the column; surfaced as a real object so the
    /// manifest is usable without re-parsing, and left as a string only if it
    /// somehow does not parse.
    #[serde(rename = "parsedJson")]
    parsed_json: serde_json::Value,
}

/// Where receipt images live.
///
/// `remote_path` is stored relative to the *database* directory -- the upload
/// handlers write `"receipt_pictures/<id>.jpg"` precisely so the server's own
/// layout is not leaked to other clients -- so paths resolve against that, not
/// against the pictures directory itself. Resolving against the pictures
/// directory would look for `receipt_pictures/receipt_pictures/<id>.jpg`.
fn db_dir(state: &AppState) -> PathBuf {
    state
        .config
        .db_path
        .parent()
        .unwrap_or_else(|| Path::new("/tmp"))
        .to_path_buf()
}

fn read_row(conn: &rusqlite::Connection, id: &str) -> Result<ReceiptRow, rusqlite::Error> {
    conn.query_row(
        "SELECT remote_path, upload_status, parsed, parsed_json, transaction_id, archived_path
         FROM receipts WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(ReceiptRow {
                remote_path: row.get(0)?,
                upload_status: row.get(1)?,
                parsed: row.get(2)?,
                parsed_json: row.get(3)?,
                transaction_id: row.get(4)?,
                archived_path: row.get(5)?,
            })
        },
    )
}

struct ReceiptRow {
    remote_path: Option<String>,
    upload_status: i64,
    parsed: bool,
    parsed_json: Option<String>,
    transaction_id: Option<String>,
    archived_path: Option<String>,
}

/// Moves a parsed receipt's image into the archive.
///
/// `remote_path` is deliberately left untouched: it records where the image
/// came from, which is what makes restoring possible.
pub async fn archive_receipt(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let conn = state.db.get().unwrap();
    let row = match read_row(&conn, &id) {
        Ok(r) => r,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return err(StatusCode::NOT_FOUND, "Receipt not found");
        }
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    if row.archived_path.is_some() {
        return err(StatusCode::CONFLICT, "Receipt is already archived");
    }
    if !row.parsed {
        return err(
            StatusCode::CONFLICT,
            "Receipt has not been parsed yet",
        );
    }
    let Some(source_rel) = row.remote_path.as_deref() else {
        return err(
            StatusCode::CONFLICT,
            "Receipt has no stored image to archive",
        );
    };

    let base = db_dir(&state);
    let Some(source) = resolve_stored_path(&base, source_rel) else {
        return err(StatusCode::BAD_REQUEST, "Receipt image path is invalid");
    };
    if !source.exists() {
        return err(
            StatusCode::NOT_FOUND,
            format!("Receipt image not found at {}", source.display()),
        );
    }

    // One folder per receipt id, so the destination name is unambiguous.
    let folder = state.config.archive_dir().join(&id);
    if let Err(e) = std::fs::create_dir_all(&folder) {
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot create archive folder: {e}"),
        );
    }
    let ext = source
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "jpg".to_string());
    let dest = folder.join(format!("receipt.{ext}"));
    if let Err(e) = std::fs::rename(&source, &dest) {
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot archive image: {e}"),
        );
    }

    let archived_at = db::server_time_ms();
    let manifest = ArchiveManifest {
        id: id.clone(),
        archived_at,
        source: source_rel.to_string(),
        upload_status: row.upload_status,
        parsed: row.parsed,
        transaction_id: row.transaction_id,
        parsed_json: match row.parsed_json {
            Some(text) => serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)),
            None => serde_json::Value::Null,
        },
    };
    if let Err(e) = std::fs::write(
        folder.join("receipt.json"),
        serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".into()),
    ) {
        // Put the image back so the receipt is not left without its data.
        let _ = std::fs::rename(&dest, &source);
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot write archive manifest: {e}"),
        );
    }

    // Only now is the receipt recorded as archived. If this fails the move above
    // is undone, so the row never claims a file is archived when it is not.
    if let Err(e) = conn.execute(
        "UPDATE receipts SET archived_path = ?1, archived_at = ?2, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![folder.to_string_lossy(), archived_at, id],
    ) {
        let _ = std::fs::remove_file(folder.join("receipt.json"));
        let _ = std::fs::rename(&dest, &source);
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    Json(serde_json::json!({
        "id": id,
        "archived_path": folder.to_string_lossy(),
        "archived_at": archived_at,
        "server_time": archived_at,
    })).into_response()
}

/// Inverse of [`archive_receipt`]: moves the image back to where `remote_path`
/// said it lived, and removes the sidecar.
pub async fn restore_receipt(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let conn = state.db.get().unwrap();
    let row = match read_row(&conn, &id) {
        Ok(r) => r,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return err(StatusCode::NOT_FOUND, "Receipt not found");
        }
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let Some(archived) = row.archived_path.as_deref() else {
        return err(StatusCode::CONFLICT, "Receipt is not archived");
    };
    let Some(source_rel) = row.remote_path.as_deref() else {
        return err(
            StatusCode::CONFLICT,
            "Receipt has no recorded image location to restore to",
        );
    };

    let folder = PathBuf::from(archived);
    // Read the extension from the sidecar's sibling rather than trusting the
    // folder contents, so a stray file cannot be moved into place.
    let ext = row
        .remote_path
        .as_deref()
        .and_then(|p| Path::new(p).extension().map(|e| e.to_string_lossy().to_string()))
        .unwrap_or_else(|| "jpg".to_string());
    let archived_image = folder.join(format!("receipt.{ext}"));
    if !archived_image.exists() {
        return err(
            StatusCode::NOT_FOUND,
            format!("Archived image not found at {}", archived_image.display()),
        );
    }

    let base = db_dir(&state);
    let Some(dest) = resolve_stored_path(&base, source_rel) else {
        return err(StatusCode::BAD_REQUEST, "Receipt image path is invalid");
    };
    if let Some(parent) = dest.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot create picture directory: {e}"),
        );
    }
    if let Err(e) = std::fs::rename(&archived_image, &dest) {
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot restore image: {e}"),
        );
    }

    // Clear the archive bookkeeping first: if the row is not updated the image
    // is back where it belongs but still recorded as archived, which is the
    // recoverable direction. The leftover folder is tidied up after.
    let server_time = db::server_time_ms();
    if let Err(e) = conn.execute(
        "UPDATE receipts SET archived_path = NULL, archived_at = NULL, updated_at = ?1 WHERE id = ?2",
        rusqlite::params![server_time, id],
    ) {
        let _ = std::fs::rename(&dest, &archived_image);
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    let _ = std::fs::remove_file(folder.join("receipt.json"));
    let _ = std::fs::remove_dir(&folder);

    Json(serde_json::json!({
        "id": id,
        "archived_path": serde_json::Value::Null,
        "server_time": server_time,
    })).into_response()
}