use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::{db, server::AppState};

#[derive(Deserialize)]
struct ReceiptsJson {
    receipts: Vec<ReceiptPush>,
}

#[derive(Deserialize, Clone)]
struct ReceiptPush {
    id: String,
    #[serde(rename = "localPath")]
    local_path: String,
    #[serde(rename = "remotePath")]
    remote_path: Option<String>,
    #[serde(rename = "uploadStatus")]
    upload_status: i64,
    parsed: bool,
    #[serde(rename = "parsedJson")]
    parsed_json: Option<String>,
    #[serde(rename = "transactionId")]
    transaction_id: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: i64,
}

pub async fn push_receipt_pictures(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let server_time = db::server_time_ms();

    if content_type.starts_with("multipart/") {
        let boundary = content_type
            .split("boundary=")
            .nth(1)
            .unwrap_or("")
            .trim_matches('"')
            .trim();
        if boundary.is_empty() {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message":"missing boundary"}))).into_response();
        }
        let (receipt_opt, picture_bytes) = parse_multipart(&body, boundary);
        let receipt: ReceiptPush = if let Some(r) = receipt_opt {
            match serde_json::from_str(&r) {
                Ok(v) => v,
                Err(e) => {
                    return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message": e.to_string()}))).into_response();
                }
            }
        } else {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message":"missing receipt field"}))).into_response();
        };

        let conn = state.db.lock().unwrap();
        if let Some(bytes) = picture_bytes {
            let base = state.config.db_path.parent().unwrap_or(std::path::Path::new("/tmp"));
            let dir = base.join("receipt_pictures");
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(format!("{}.jpg", receipt.id));
            let _ = std::fs::write(&path, &bytes);
            let remote = path.to_string_lossy().to_string();
            conn.execute(
                "INSERT INTO receipts (id, local_path, remote_path, upload_status, parsed, parsed_json, transaction_id, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL) ON CONFLICT(id) DO UPDATE SET local_path=excluded.local_path, remote_path=excluded.remote_path, upload_status=excluded.upload_status, parsed=excluded.parsed, parsed_json=excluded.parsed_json, transaction_id=excluded.transaction_id, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
                rusqlite::params![receipt.id, receipt.local_path, remote, receipt.upload_status, if receipt.parsed {1} else {0}, receipt.parsed_json, receipt.transaction_id, receipt.created_at, server_time],
            )
            .unwrap();
        } else {
            conn.execute(
                "INSERT INTO receipts (id, local_path, remote_path, upload_status, parsed, parsed_json, transaction_id, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL) ON CONFLICT(id) DO UPDATE SET local_path=excluded.local_path, remote_path=excluded.remote_path, upload_status=excluded.upload_status, parsed=excluded.parsed, parsed_json=excluded.parsed_json, transaction_id=excluded.transaction_id, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
                rusqlite::params![receipt.id, receipt.local_path, receipt.remote_path, receipt.upload_status, if receipt.parsed {1} else {0}, receipt.parsed_json, receipt.transaction_id, receipt.created_at, server_time],
            )
            .unwrap();
        }
        (StatusCode::OK, Json(serde_json::json!({"server_time": server_time}))).into_response()
    } else {
        let payload: ReceiptsJson = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"message": e.to_string()}))).into_response();
            }
        };
        let conn = state.db.lock().unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        for r in payload.receipts {
            tx.execute(
                "INSERT INTO receipts (id, local_path, remote_path, upload_status, parsed, parsed_json, transaction_id, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL) ON CONFLICT(id) DO UPDATE SET local_path=excluded.local_path, remote_path=excluded.remote_path, upload_status=excluded.upload_status, parsed=excluded.parsed, parsed_json=excluded.parsed_json, transaction_id=excluded.transaction_id, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=NULL",
                rusqlite::params![r.id, r.local_path, r.remote_path, r.upload_status, if r.parsed {1} else {0}, r.parsed_json, r.transaction_id, r.created_at, server_time],
            )
            .unwrap();
        }
        tx.commit().unwrap();
        (StatusCode::OK, Json(serde_json::json!({"server_time": server_time}))).into_response()
    }
}

fn parse_multipart(body: &[u8], boundary: &str) -> (Option<String>, Option<Vec<u8>>) {
    let boundary_bytes = format!("--{boundary}").into_bytes();
    let parts = split_bytes(body, &boundary_bytes);
    let mut receipt_opt = None;
    let mut picture_opt = None;
    for part in parts {
        if part.windows(b"receipt".len()).any(|w| w==b"receipt") && part.windows(b"Content-Type: application/json".len()).any(|_| false) {
        }
        let text = String::from_utf8_lossy(part);
        if text.contains("name=\"receipt\"") || text.contains("name=receipt") {
            if let Some(start) = find_double_crlf(part) {
                let json_bytes = &part[start..];
                let end = find_boundary_end(json_bytes);
                let json_slice = &json_bytes[..end];
                let s = String::from_utf8_lossy(json_slice).trim().trim_matches('\r').trim_matches('\n').to_string();
                if !s.is_empty() {
                    receipt_opt = Some(s);
                }
            }
        } else if text.contains("name=\"picture\"") || text.contains("name=picture") {
            if let Some(start) = find_double_crlf(part) {
                let pic_bytes = &part[start..];
                let end = find_boundary_end(pic_bytes);
                picture_opt = Some(pic_bytes[..end].to_vec());
            }
        }
    }
    (receipt_opt, picture_opt)
}

fn split_bytes<'a>(data: &'a [u8], pattern: &[u8]) -> Vec<&'a [u8]> {
    let mut res = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + pattern.len() <= data.len() {
        if &data[i..i+pattern.len()] == pattern {
            res.push(&data[start..i]);
            i += pattern.len();
            start = i;
        } else {
            i += 1;
        }
    }
    if start < data.len() {
        res.push(&data[start..]);
    }
    res
}

fn find_double_crlf(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|w| w==b"\r\n\r\n").map(|p| p+4)
        .or_else(|| data.windows(2).position(|w| w==b"\n\n").map(|p| p+2))
}

fn find_boundary_end(data: &[u8]) -> usize {
    if let Some(pos) = data.windows(4).position(|w| w==b"\r\n--") {
        pos
    } else if let Some(pos) = data.windows(2).position(|w| w==b"\n-") {
        pos
    } else {
        data.len()
    }
}
