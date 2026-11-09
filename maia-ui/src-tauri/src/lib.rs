pub mod api;
use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::Datelike;
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use maia::Config as SyncConfig;

/// Shared application state managed by Tauri.
struct AppState {
    db: Mutex<Connection>,
}

// ---- Data types ----

/// A note, as the `notes` table actually stores it.
///
/// The body is a column rather than a file on disk, so there is no path and no
/// separate read to fail.
#[derive(Serialize, Deserialize)]
struct NoteWithContent {
    id: String,
    title: String,
    content: String,
    created_at: String,
    updated_at: String,
}

/// A receipt row, matching the `receipts` table in `maia::db`.
///
/// `id` is a UUID v7 string and timestamps are epoch milliseconds, per the v30
/// schema. The previous struct targeted a long-gone `original_path` /
/// `current_path` / `archived_path` file-triplet layout with an `INTEGER` id.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    id: String,
    local_path: String,
    remote_path: Option<String>,
    /// Mirrors the client's `ReceiptStatus`: 0 local, 1 uploading,
    /// 2 uploaded, 3 error, 4 parsing.
    upload_status: i64,
    parsed: bool,
    /// The OCR result as a JSON *string*, not a nested object.
    parsed_json: Option<String>,
    parse_error: Option<String>,
    transaction_id: Option<String>,
    archived_path: Option<String>,
    archived_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

const RECEIPT_COLUMNS: &str = "id, local_path, remote_path, upload_status, parsed, parsed_json, \
     parse_error, transaction_id, archived_path, archived_at, created_at, updated_at";

#[derive(Serialize, Deserialize, Clone)]
struct Task {
    id: String,
    goal_id: Option<i64>,
    title: String,
    description: Option<String>,
    due_date: Option<String>,
    priority: Option<String>,
    tags: String,
    order_index: i64,
    completed_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Goal {
    id: String,
    title: String,
    description: Option<String>,
    status: Option<String>,
    deadline: Option<String>,
    progress: f64,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct SavedUrl {
    /// A TEXT primary key. This was typed as an integer to go with the
    /// `last_insert_rowid()` insert that has now been replaced by a server push,
    /// which sends a client-generated uuid.
    id: String,
    title: Option<String>,
    url: String,
    source: Option<String>,
    tags: String,
    is_new: bool,
    created_at: String,
}

#[derive(Serialize)]
struct DashboardCounts {
    tasks: i64,
    goals: i64,
    receipts: i64,
    notes: i64,
    urls: i64,
}

#[derive(Serialize)]
struct CommandResult<T: Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

// ---- Database initialization ----

/// Opens the database read-only.
///
/// The desktop app does not write here. Everything it used to write goes
/// through the sync server, which makes the server the only writer and leaves
/// this process unable to migrate the schema even by accident -- two runners
/// stepping the same database at once was a real source of corruption.
///
/// Opening with SQLITE_OPEN_READ_ONLY means SQLite refuses writes at the engine
/// level, rather than relying on every caller remembering not to write.
fn init_database(db_path: &str) -> Result<Connection, String> {
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("Failed to open database read-only: {e}"))
}

/// Where the database lives, from the shared configuration.
///
/// Used to probe `maia.json` and then walk up the tree looking for `maia.db`,
/// which meant a launch from anywhere but the checkout opened a different
/// database than the service was using.
fn get_database_path() -> String {
    sync_config().db_path.display().to_string()
}

/// The API key, from the shared configuration rather than the checkout's
/// `maia.json`, so the app and the service cannot disagree about it.
fn get_sync_api_key_value() -> String {
    sync_config().api_key.clone()
}

/// Fallback used when the configuration cannot be read, so a broken file
/// degrades the sync half of the app instead of preventing it from starting.
/// The service's own `ExecStartPre` reports the real problem with the same file.
static FALLBACK_SYNC_CONFIG: OnceLock<SyncConfig> = OnceLock::new();

/// The API's address, taken from configuration.
///
/// The server runs as its own process now, so this describes where it was told to
/// listen rather than a socket this app owns. Reporting it unconditionally is the
/// point: it used to be read back from a bind that silently failed whenever the
/// service already held the port, which left the pairing QR showing port 0.
fn sync_config() -> &'static SyncConfig {
    match SyncConfig::shared(None) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("failed to load sync config: {e}");
            FALLBACK_SYNC_CONFIG.get_or_init(SyncConfig::default)
        }
    }
}

#[tauri::command]
fn get_sync_port() -> u16 {
    sync_config().port()
}

#[tauri::command]
fn get_sync_url() -> String {
    format!("http://0.0.0.0:{}", sync_config().port())
}

#[tauri::command]
fn get_sync_api_key() -> String {
    get_sync_api_key_value()
}

fn get_lan_ip_value() -> String {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip();
                if !ip.is_loopback() && ip.is_ipv4() {
                    return ip.to_string();
                }
            }
        }
    }
    "127.0.0.1".to_string()
}

#[tauri::command]
fn get_lan_ip() -> String {
    get_lan_ip_value()
}

#[tauri::command]
fn get_sync_lan_url() -> String {
    format!("http://{}:{}", get_lan_ip_value(), sync_config().port())
}

#[tauri::command]
fn get_server_diagnostics() -> serde_json::Value {
    let cfg = sync_config();
    let port = cfg.port();
    let lan_ip = get_lan_ip_value();
    let lan_url = format!("http://{}:{}", lan_ip, port);
    let is_listening = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok();
    let api_key = cfg.api_key.clone();
    let masked = mask_secret(&api_key);
    serde_json::json!({
        "port": port,
        "lanUrl": lan_url,
        "bindAddr": cfg.bind_addr,
        "isListening": is_listening,
        "lanIp": lan_ip,
        "apiKeyMasked": masked,
        "hasApiKey": !api_key.is_empty(),
        // Receipt OCR needs a vision-model key; without one the sync server
        // answers the parse endpoints with 503.
        "modelApiKeyMasked": mask_secret(&cfg.model_api_key),
        "hasModelApiKey": !cfg.model_api_key.is_empty()
    })
}

/// Everything the phone needs to pair, as one object.
///
/// The URL and key are read here rather than in the webview so the pairing
/// details never depend on the webview being able to reach the server.
#[tauri::command]
async fn sync_info() -> serde_json::Value {
    let cfg = sync_config();
    let port = cfg.port();
    let lan_ip = get_lan_ip_value();
    serde_json::json!({
        "port": port,
        // Always loopback for this machine. The configured address is a *bind*
        // address, and a bind address is not somewhere to connect to -- the
        // earlier version handed the webview "0.0.0.0:3030" to fetch from.
        "localUrl": format!("http://127.0.0.1:{port}"),
        "lanUrl": format!("http://{}:{}", lan_ip, port),
        "lanIp": lan_ip,
        "apiKey": cfg.api_key,
        "isListening": api::is_reachable().await,
    })
}

/// Shows only the first and last few characters of a secret, mirroring how the
/// sync API key is displayed.
fn mask_secret(secret: &str) -> String {
    if secret.is_empty() {
        return String::new();
    }
    if secret.len() <= 8 {
        return "***".to_string();
    }
    format!("{}...{}", &secret[..4], &secret[secret.len() - 4..])
}

// ---- Tauri commands ----

#[tauri::command]
fn list_notes(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let mut stmt = match conn.prepare("SELECT id, title, body, created_at, updated_at FROM notes WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"body":r.get::<_,String>(2)?,"createdAt":r.get::<_,i64>(3)?,"updatedAt":r.get::<_,i64>(4)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}

#[tauri::command]
fn read_note(state: tauri::State<AppState>, id: String) -> CommandResult<NoteWithContent> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Database lock error: {e}")),
            };
        }
    };

    match conn.query_row(
        "SELECT id, title, body, created_at, updated_at FROM notes WHERE id = ?1",
        params![id],
        |row| {
            Ok(NoteWithContent {
                id: row.get(0)?,
                title: row.get::<_, String>(1).unwrap_or_default(),
                content: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                created_at: row
                    .get::<_, i64>(3)
                    .map(|ms| ms.to_string())
                    .unwrap_or_default(),
                updated_at: row
                    .get::<_, i64>(4)
                    .map(|ms| ms.to_string())
                    .unwrap_or_default(),
            })
        },
    ) {
        Ok(note) => CommandResult {
            success: true,
            data: Some(note),
            error: None,
        },
        Err(rusqlite::Error::QueryReturnedNoRows) => CommandResult {
            success: false,
            data: None,
            error: Some("Note not found".to_string()),
        },
        Err(e) => CommandResult {
            success: false,
            data: None,
            error: Some(format!("Query error: {e}")),
        },
    }
}

// ---- Receipt commands ----

/// Directory holding receipt pictures, relative to the database file.
fn receipt_picture_dir() -> PathBuf {
    PathBuf::from(get_database_path())
        .parent()
        .map(|p| p.to_path_buf())
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("receipt_pictures")
}

/// Resolves a stored (relative) receipt path against the picture directory.
///
/// Stored paths come from the sync server and must not be able to escape it.
fn resolve_receipt_file(base: &Path, stored: &str) -> Option<PathBuf> {
    if stored.is_empty() {
        return None;
    }
    let relative = Path::new(stored);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(base.join(relative))
}

/// Builds a [`Receipt`] from a `receipts` row, columns in `RECEIPT_COLUMNS` order.
fn map_receipt(r: &rusqlite::Row<'_>) -> rusqlite::Result<Receipt> {
    Ok(Receipt {
        id: r.get(0)?,
        local_path: r.get(1)?,
        remote_path: r.get(2)?,
        upload_status: r.get(3)?,
        parsed: r.get::<_, i64>(4)? != 0,
        parsed_json: r.get(5)?,
        parse_error: r.get(6)?,
        transaction_id: r.get(7)?,
        archived_path: r.get(8)?,
        archived_at: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
}

#[tauri::command]
fn list_receipts(state: tauri::State<AppState>) -> CommandResult<Vec<Receipt>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let sql = format!(
        "SELECT {RECEIPT_COLUMNS} FROM receipts WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 500"
    );
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("{e}")),
            };
        }
    };
    let rows: Vec<Receipt> = stmt
        .query_map([], map_receipt)
        .unwrap_or_else(|e| panic!("query receipts: {e}"))
        .filter_map(|r| r.ok())
        .collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}

/// Loads one receipt, shared by the Tauri command and the archive/restore
/// commands that need to re-read the row after updating it.
fn load_receipt(state: &AppState, id: &str) -> Result<Receipt, String> {
    let conn = state
        .db
        .lock()
        .map_err(|e| format!("Database lock error: {e}"))?;
    let sql =
        format!("SELECT {RECEIPT_COLUMNS} FROM receipts WHERE id = ?1 AND deleted_at IS NULL");
    conn.query_row(&sql, params![id], map_receipt)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => "Receipt not found".to_string(),
            other => format!("Query error: {other}"),
        })
}

#[tauri::command]
fn read_receipt(state: tauri::State<AppState>, id: String) -> CommandResult<Receipt> {
    match load_receipt(&state, &id) {
        Ok(receipt) => CommandResult {
            success: true,
            data: Some(receipt),
            error: None,
        },
        Err(e) => CommandResult {
            success: false,
            data: None,
            error: Some(e),
        },
    }
}

/// Raw bytes for a receipt image, for the detail view's `<img>`.
///
/// Wraps [`load_receipt`] in the `CommandResult` envelope the commands return.
fn finish_receipt(state: &AppState, id: &str) -> CommandResult<Receipt> {
    match load_receipt(state, id) {
        Ok(r) => CommandResult {
            success: true,
            data: Some(r),
            error: None,
        },
        Err(e) => CommandResult {
            success: false,
            data: None,
            error: Some(e),
        },
    }
}

/// Raw bytes for a receipt image, for the detail view.s `<img>`.: that
/// endpoint sits behind Bearer auth, which a plain image tag cannot send.
#[tauri::command]
fn read_receipt_picture(
    state: tauri::State<AppState>,
    id: String,
) -> CommandResult<serde_json::Value> {
    let row =
        {
            let conn = match state.db.lock() {
                Ok(c) => c,
                Err(e) => {
                    return CommandResult {
                        success: false,
                        data: None,
                        error: Some(format!("Database lock error: {e}")),
                    };
                }
            };
            conn.query_row(
            "SELECT remote_path, archived_path FROM receipts WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        };
    let (remote_path, archived_path) = match row {
        Ok(v) => v,
        Err(_) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some("Receipt not found".to_string()),
            };
        }
    };

    // Archived folders win: archiving *moved* the file.
    let base = receipt_picture_dir();
    let path = archived_path
        .or(remote_path)
        .and_then(|p| resolve_receipt_file(&base, &p));
    let Some(path) = path else {
        return CommandResult {
            success: false,
            data: None,
            error: Some("Receipt image not found".to_string()),
        };
    };
    match fs::read(&path) {
        Ok(bytes) => CommandResult {
            success: true,
            data: Some(serde_json::json!({
                "base64": base64_encode(&bytes),
                "contentType": content_type_for_receipt(&path),
            })),
            error: None,
        },
        Err(e) => CommandResult {
            success: false,
            data: None,
            error: Some(format!("Cannot read receipt image: {e}")),
        },
    }
}

/// Content type for a stored receipt image, from its extension.
fn content_type_for_receipt(path: &Path) -> &'static str {
    match path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .as_deref()
        .unwrap_or("")
    {
        "png" => "image/png",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "heif" => "image/heif",
        _ => "image/jpeg",
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Archives a parsed receipt: moves its image into the archive folder and writes
/// the manifest beside it.
///
/// The move belongs to the server, which is the only process allowed to write.
/// The desktop app no longer touches the filesystem for this, so the pairing of
/// "file moved" and "row updated" cannot drift apart.
#[tauri::command]
async fn archive_receipt(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<CommandResult<Receipt>, String> {
    let result = push_ok(
        &format!("/receipt-pictures/{id}/archive"),
        &serde_json::json!({}),
        serde_json::json!({}),
    )
    .await;
    Ok(reload_receipt(&state, &id, result))
}

/// The inverse of [`archive_receipt`]: puts the image back where it was.
#[tauri::command]
async fn restore_receipt(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<CommandResult<Receipt>, String> {
    let result = push_ok(
        &format!("/receipt-pictures/{id}/restore"),
        &serde_json::json!({}),
        serde_json::json!({}),
    )
    .await;
    Ok(reload_receipt(&state, &id, result))
}

/// Re-reads a receipt after a push, so the caller gets the stored row rather
/// than echoing back what it asked for.
fn reload_receipt(
    state: &tauri::State<'_, AppState>,
    id: &str,
    result: CommandResult<serde_json::Value>,
) -> CommandResult<Receipt> {
    match result.success {
        true => finish_receipt(state, id),
        false => CommandResult {
            success: false,
            data: None,
            error: result.error,
        },
    }
}

/// Lists workout templates, newest first.
///
/// The templates view used to be fed by the sync server over HTTP. It reads the
/// database like everything else now, so it needs a read command of its own.
#[tauri::command]
fn list_templates(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let mut stmt = match conn.prepare("SELECT id, name, notes, start_date, recurrence, created_at, updated_at FROM workout_templates WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("prep {e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"notes":r.get::<_,Option<String>>(2)?,"startDate":r.get::<_,i64>(3)?,"recurrence":r.get::<_,Option<String>>(4)?,"createdAt":r.get::<_,i64>(5)?,"updatedAt":r.get::<_,i64>(6)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}

/// Lists recorded workouts, newest first.
#[tauri::command]
fn list_workouts(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let mut stmt = match conn.prepare("SELECT id, name, date, started_at, completed_at, notes, routine_id, template_id, created_at, updated_at FROM workouts WHERE deleted_at IS NULL ORDER BY date DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("prep {e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"date":r.get::<_,i64>(2)?,"startedAt":r.get::<_,Option<i64>>(3)?,"completedAt":r.get::<_,Option<i64>>(4)?,"notes":r.get::<_,Option<String>>(5)?,"routineId":r.get::<_,Option<String>>(6)?,"templateId":r.get::<_,Option<String>>(7)?,"createdAt":r.get::<_,i64>(8)?,"updatedAt":r.get::<_,i64>(9)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}

/// Loads one workout with its exercises and sets (read-only detail view).
#[tauri::command]
fn get_workout(state: tauri::State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let workout = match conn.query_row(
        "SELECT id, name, date, started_at, completed_at, notes, routine_id, template_id, created_at, updated_at FROM workouts WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"date":r.get::<_,i64>(2)?,"startedAt":r.get::<_,Option<i64>>(3)?,"completedAt":r.get::<_,Option<i64>>(4)?,"notes":r.get::<_,Option<String>>(5)?,"routineId":r.get::<_,Option<String>>(6)?,"templateId":r.get::<_,Option<String>>(7)?,"createdAt":r.get::<_,i64>(8)?,"updatedAt":r.get::<_,i64>(9)?})),
    ) {
        Ok(w) => w,
        Err(rusqlite::Error::QueryReturnedNoRows) => return CommandResult{success:false,data:None,error:Some("Workout not found".to_string())},
        Err(e) => return CommandResult{success:false,data:None,error:Some(format!("query {e}"))},
    };
    let mut ex_stmt = match conn.prepare(
        "SELECT we.id, we.exercise_id, e.name, we.sort_order, we.notes FROM workout_exercises we JOIN exercises e ON e.id = we.exercise_id WHERE we.workout_id = ?1 AND we.deleted_at IS NULL ORDER BY we.sort_order",
    ) { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("prep {e}"))}};
    let exercise_ids: Vec<(String, String, String, i64, Option<String>)> = ex_stmt
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let mut set_stmt = match conn.prepare(
        "SELECT id, set_number, reps, weight_kg, rest_seconds, actual_reps, actual_weight_kg, actual_rest_seconds, duration_minutes, distance_meters, actual_duration_minutes, actual_distance_meters, notes, completed_at FROM exercise_sets WHERE workout_exercise_id = ?1 AND deleted_at IS NULL ORDER BY set_number",
    ) { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("prep {e}"))}};
    let mut exercises: Vec<serde_json::Value> = Vec::new();
    for (we_id, exercise_id, exercise_name, sort_order, notes) in exercise_ids {
        let sets: Vec<serde_json::Value> = set_stmt
            .query_map([&we_id], |r| {
                Ok(serde_json::json!({
                    "id":r.get::<_,String>(0)?,
                    "setNumber":r.get::<_,i64>(1)?,
                    "reps":r.get::<_,Option<i64>>(2)?,
                    "weightKg":r.get::<_,Option<f64>>(3)?,
                    "restSeconds":r.get::<_,Option<i64>>(4)?,
                    "actualReps":r.get::<_,Option<i64>>(5)?,
                    "actualWeightKg":r.get::<_,Option<f64>>(6)?,
                    "actualRestSeconds":r.get::<_,Option<i64>>(7)?,
                    "durationMinutes":r.get::<_,Option<i64>>(8)?,
                    "distanceMeters":r.get::<_,Option<f64>>(9)?,
                    "actualDurationMinutes":r.get::<_,Option<i64>>(10)?,
                    "actualDistanceMeters":r.get::<_,Option<f64>>(11)?,
                    "notes":r.get::<_,Option<String>>(12)?,
                    "completedAt":r.get::<_,Option<i64>>(13)?
                }))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        exercises.push(serde_json::json!({"id":we_id,"exerciseId":exercise_id,"exerciseName":exercise_name,"sortOrder":sort_order,"notes":notes,"sets":sets}));
    }
    let mut workout = workout;
    workout
        .as_object_mut()
        .map(|o| o.insert("exercises".into(), serde_json::Value::Array(exercises)));
    CommandResult {
        success: true,
        data: Some(workout),
        error: None,
    }
}
/// Lists tasks, newest update first.
#[tauri::command]
fn list_tasks(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let mut stmt = match conn.prepare("SELECT id, date, title, done, task_status, carry_over, sort_order, due_date, notes, created_at, updated_at FROM tasks WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"date":r.get::<_,i64>(1)?,"title":r.get::<_,String>(2)?,"done":r.get::<_,i64>(3)?,"taskStatus":r.get::<_,Option<i64>>(4)?,"carryOver":r.get::<_,i64>(5)?!=0,"sortOrder":r.get::<_,i64>(6)?,"dueDate":r.get::<_,Option<i64>>(7)?,"notes":r.get::<_,Option<String>>(8)?,"createdAt":r.get::<_,i64>(9)?,"updatedAt":r.get::<_,i64>(10)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}

/// Lists tasks for the desktop calendar: every task whose own day
/// (`due_date` if set, else `date`) falls within `[start, end]` (epoch ms,
/// inclusive), plus *every* recurring anchor regardless of its own date —
/// the caller expands those client-side (see `src/lib/recurrence.ts`), and
/// an anchor created long before `start` can still recur into the range.
#[tauri::command]
fn list_tasks_range(
    state: tauri::State<AppState>,
    start: i64,
    end: i64,
) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let mut stmt = match conn.prepare(
        "SELECT id, date, title, done, task_status, due_date, due_time_minutes, start_time_minutes, end_time_minutes, notes, recurrence, series_id, created_at, updated_at \
         FROM tasks \
         WHERE deleted_at IS NULL \
           AND (recurrence IS NOT NULL OR COALESCE(due_date, date) BETWEEN ?1 AND ?2) \
         ORDER BY COALESCE(due_date, date) ASC",
    ) {
        Ok(s) => s,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("{e}")) },
    };
    let rows: Vec<serde_json::Value> = stmt
        .query_map(params![start, end], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "date": r.get::<_, i64>(1)?,
                "title": r.get::<_, String>(2)?,
                "done": r.get::<_, i64>(3)?,
                "taskStatus": r.get::<_, Option<i64>>(4)?,
                "dueDate": r.get::<_, Option<i64>>(5)?,
                "dueTimeMinutes": r.get::<_, Option<i64>>(6)?,
                "startTimeMinutes": r.get::<_, Option<i64>>(7)?,
                "endTimeMinutes": r.get::<_, Option<i64>>(8)?,
                "notes": r.get::<_, Option<String>>(9)?,
                "recurrence": r.get::<_, Option<String>>(10)?,
                "seriesId": r.get::<_, Option<String>>(11)?,
                "createdAt": r.get::<_, i64>(12)?,
                "updatedAt": r.get::<_, i64>(13)?,
            }))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}

// ---- Goal commands ----

#[tauri::command]
fn list_goals(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let mut stmt = match conn.prepare("SELECT id, title, description, start_date, end_date, status, target_type, target_value, baseline_value, unit, reminder_enabled, reminder_time_minutes, created_at, updated_at FROM goals WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"description":r.get::<_,Option<String>>(2)?,"startDate":r.get::<_,i64>(3)?,"endDate":r.get::<_,Option<i64>>(4)?,"status":r.get::<_,String>(5)?,"targetType":r.get::<_,String>(6)?,"targetValue":r.get::<_,Option<f64>>(7)?,"baselineValue":r.get::<_,Option<f64>>(8)?,"unit":r.get::<_,Option<String>>(9)?,"reminderEnabled":r.get::<_,i64>(10)?!=0,"reminderTimeMinutes":r.get::<_,i64>(11)?,"createdAt":r.get::<_,i64>(12)?,"updatedAt":r.get::<_,i64>(13)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}

// ---- Dashboard commands ----

#[tauri::command]
fn get_counts(state: tauri::State<AppState>) -> CommandResult<DashboardCounts> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let tasks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let goals: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let receipts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM receipts WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let notes: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM notes WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let urls: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    CommandResult {
        success: true,
        data: Some(DashboardCounts {
            tasks,
            goals,
            receipts,
            notes,
            urls,
        }),
        error: None,
    }
}

// ---- URL commands ----

#[tauri::command]
fn list_urls(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let mut stmt = match conn.prepare("SELECT id, title, url, source, tags, is_new, created_at, updated_at FROM urls WHERE deleted_at IS NULL ORDER BY is_new DESC, created_at DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,Option<String>>(1)?,"url":r.get::<_,String>(2)?,"source":r.get::<_,Option<String>>(3)?,"tags":r.get::<_,Option<String>>(4)?,"is_new":r.get::<_,i64>(5)?!=0,"created_at":r.get::<_,i64>(6)?,"updated_at":r.get::<_,i64>(7)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}

/// A saved url as the frontend expects it back.
#[tauri::command]
async fn create_url(
    title: Option<String>,
    url: String,
    source: Option<String>,
    tags: Option<String>,
) -> CommandResult<SavedUrl> {
    let id = uuid::Uuid::now_v7().to_string();
    let saved = SavedUrl {
        id: id.clone(),
        title: title.clone(),
        url: url.clone(),
        source: source.clone(),
        tags: tags.clone().unwrap_or_else(|| "[]".to_string()),
        is_new: true,
        created_at: now_ms().to_string(),
    };
    let payload = serde_json::json!({
        "urls": [{
            "id": id, "title": title, "url": url, "source": source,
            "tags": tags.unwrap_or_else(|| "[]".to_string()),
            "isNew": true, "createdAt": now_ms(),
        }],
        "deleted": [],
    });
    match push_ok("/urls", &payload, serde_json::json!({})).await {
        CommandResult { success: true, .. } => CommandResult {
            success: true,
            data: Some(saved),
            error: None,
        },
        CommandResult { error, .. } => CommandResult {
            success: false,
            data: None,
            error,
        },
    }
}

/// Deletes a url by tombstone.
///
/// The server has no DELETE and every pull filters on `deleted_at`, so removing
/// the row outright would leave other clients holding a bookmark nothing removes.
#[tauri::command]
async fn delete_url(id: String) -> CommandResult<()> {
    match push_ok(
        "/urls",
        &serde_json::json!({ "urls": [], "deleted": [id] }),
        serde_json::json!({}),
    )
    .await
    {
        CommandResult { success: true, .. } => CommandResult {
            success: true,
            data: None,
            error: None,
        },
        CommandResult { error, .. } => CommandResult {
            success: false,
            data: None,
            error,
        },
    }
}

/// Marks a bookmark read by re-sending it with `isNew` false.
#[tauri::command]
async fn mark_url_read(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<CommandResult<SavedUrl>, String> {
    let Some(mut entry) = read_url_for_push(&state, &id) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Url not found".into()),
        });
    };
    if let Some(obj) = entry.as_object_mut() {
        obj.insert("isNew".into(), serde_json::json!(false));
    }
    let saved = saved_url_from(&entry);
    let result = push_ok(
        "/urls",
        &serde_json::json!({ "urls": [entry], "deleted": [] }),
        serde_json::json!({}),
    )
    .await;
    Ok(match result {
        CommandResult { success: true, .. } => CommandResult {
            success: true,
            data: Some(saved),
            error: None,
        },
        CommandResult { error, .. } => CommandResult {
            success: false,
            data: None,
            error,
        },
    })
}

/// Reads a url back in the shape `POST /urls` expects.
fn read_url_for_push(state: &tauri::State<'_, AppState>, id: &str) -> Option<serde_json::Value> {
    let conn = state.db.lock().ok()?;
    conn.query_row(
        "SELECT id, title, url, source, tags, is_new, created_at FROM urls WHERE id=?1 AND deleted_at IS NULL",
        params![id],
        |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "title": r.get::<_, Option<String>>(1)?,
                "url": r.get::<_, String>(2)?,
                "source": r.get::<_, Option<String>>(3)?,
                "tags": r.get::<_, Option<String>>(4)?.unwrap_or_else(|| "[]".to_string()),
                "isNew": r.get::<_, i64>(5)? != 0,
                "createdAt": r.get::<_, i64>(6)?,
            }))
        },
    ).ok()
}

fn saved_url_from(entry: &serde_json::Value) -> SavedUrl {
    SavedUrl {
        id: entry
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        title: entry
            .get("title")
            .and_then(|v| v.as_str())
            .map(String::from),
        url: entry
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        source: entry
            .get("source")
            .and_then(|v| v.as_str())
            .map(String::from),
        tags: entry
            .get("tags")
            .and_then(|v| v.as_str())
            .unwrap_or("[]")
            .to_string(),
        is_new: entry
            .get("isNew")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        created_at: entry
            .get("createdAt")
            .and_then(|v| v.as_i64())
            .unwrap_or_default()
            .to_string(),
    }
}

#[tauri::command]
fn list_exercises(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let mut stmt = match conn.prepare("SELECT id, name, exercise_type, body_part, equipment, primary_muscle, secondary_muscle, instructions, tips, faqs, keywords, image_path, video_path, created_at, updated_at FROM exercises ORDER BY name LIMIT 5000") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("prep {e}"))}};
    let rows = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"exercise_type":r.get::<_,Option<String>>(2)?,"body_part":r.get::<_,Option<String>>(3)?,"equipment":r.get::<_,Option<String>>(4)?,"primary_muscle":r.get::<_,Option<String>>(5)?,"secondary_muscle":r.get::<_,Option<String>>(6)?,"instructions":r.get::<_,Option<String>>(7)?,"tips":r.get::<_,Option<String>>(8)?,"faqs":r.get::<_,Option<String>>(9)?,"keywords":r.get::<_,Option<String>>(10)?,"image_path":r.get::<_,Option<String>>(11)?,"video_path":r.get::<_,Option<String>>(12)?,"created_at":r.get::<_,i64>(13).unwrap_or(0),"updated_at":r.get::<_,Option<i64>>(14)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}
#[tauri::command]
fn list_ingredients(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let mut stmt = match conn.prepare("SELECT id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, sodium_per100g, fiber_per100g, sugar_per100g, is_archived, brand, barcode, created_at FROM ingredients ORDER BY name LIMIT 1000") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let mut out = Vec::new();
    let rows: Vec<(
        String,
        String,
        f64,
        f64,
        f64,
        f64,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        i64,
        Option<String>,
        Option<String>,
        i64,
    )> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
                r.get(12)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    for (id, name, cal, prot, carb, fat, sod, fib, sug, arch, brand, barcode, created) in rows {
        let pics: Vec<serde_json::Value> = conn.prepare("SELECT id, image_path, sort_order FROM ingredient_pictures WHERE ingredient_id=?1 ORDER BY sort_order").unwrap().query_map([&id], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"imagePath":r.get::<_,String>(1)?,"sortOrder":r.get::<_,i64>(2)?}))).unwrap().filter_map(|r|r.ok()).collect();
        // package_grams is what turns a price into a comparable cost-per-100g;
        // source_url is the optional "where I saw this" link. Both used to be
        // missing here, so the ingredient detail view could not show a
        // cost-per-100g history at all -- see T03 in the price-tracking plan.
        let prices: Vec<serde_json::Value> = conn.prepare("SELECT id, store_id, price, currency_code, package_grams, recorded_at, source_url FROM ingredient_prices WHERE ingredient_id=?1 AND deleted_at IS NULL ORDER BY recorded_at DESC LIMIT 100").unwrap().query_map([&id], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"storeId":r.get::<_,String>(1)?,"price":r.get::<_,f64>(2)?,"currencyCode":r.get::<_,String>(3)?,"packageGrams":r.get::<_,Option<f64>>(4)?,"recordedAt":r.get::<_,i64>(5)?,"sourceUrl":r.get::<_,Option<String>>(6)?}))).unwrap().filter_map(|r|r.ok()).collect();
        out.push(serde_json::json!({"id":id,"name":name,"calories_per100g":cal,"protein_per100g":prot,"carbs_per100g":carb,"fat_per100g":fat,"sodium_per100g":sod,"fiber_per100g":fib,"sugar_per100g":sug,"is_archived":arch!=0,"brand":brand,"barcode":barcode,"createdAt":created,"pictures":pics,"prices":prices}));
    }
    CommandResult {
        success: true,
        data: Some(out),
        error: None,
    }
}
#[tauri::command]
fn list_stores(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("{e}")),
            };
        }
    };
    let mut stmt = conn
        .prepare("SELECT id, name, created_at FROM stores ORDER BY name")
        .unwrap();
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"createdAt":r.get::<_,i64>(2)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}
#[tauri::command]
fn list_meals(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("{e}")),
            };
        }
    };
    let mut stmt = conn
        .prepare(
            "SELECT id, name, eaten_at, created_at FROM meals ORDER BY eaten_at DESC LIMIT 200",
        )
        .unwrap();
    let meals: Vec<(String, String, i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let mut out = Vec::new();
    for (id, name, eaten, created) in meals {
        // Logged foods with their frozen portion snapshot, and the food name
        // joined live so a rename shows up in an old meal without its numbers
        // moving.
        let foods: Vec<serde_json::Value> = conn.prepare(
            "SELECT mf.id, mf.food_id, f.name, mf.amount, mf.calories, mf.protein, mf.carbs, mf.fat \
             FROM meal_foods mf LEFT JOIN foods f ON f.id = mf.food_id \
             WHERE mf.meal_id = ?1 AND mf.deleted_at IS NULL",
        ).unwrap().query_map([&id], |r| Ok(serde_json::json!({
            "id":r.get::<_,String>(0)?,
            "foodId":r.get::<_,String>(1)?,
            "foodName":r.get::<_,Option<String>>(2)?,
            "amount":r.get::<_,f64>(3)?,
            "calories":r.get::<_,f64>(4)?,
            "protein":r.get::<_,f64>(5)?,
            "carbs":r.get::<_,f64>(6)?,
            "fat":r.get::<_,f64>(7)?,
        }))).unwrap().filter_map(|r|r.ok()).collect();
        out.push(serde_json::json!({"id":id,"name":name,"eatenAt":eaten,"createdAt":created,"foods":foods}));
    }
    CommandResult {
        success: true,
        data: Some(out),
        error: None,
    }
}
#[tauri::command]
fn list_body_metrics(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("{e}")),
            };
        }
    };
    let mut stmt = conn.prepare("SELECT id, date, weight_kg, height_cm, created_at FROM body_metrics ORDER BY date DESC LIMIT 500").unwrap();
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"date":r.get::<_,i64>(1)?,"weightKg":r.get::<_,Option<f64>>(2)?,"heightCm":r.get::<_,Option<f64>>(3)?,"createdAt":r.get::<_,i64>(4)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}
#[tauri::command]
fn list_tags(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("{e}")),
            };
        }
    };
    let mut stmt = conn
        .prepare("SELECT id, name, color, sort_order, created_at FROM tags ORDER BY sort_order")
        .unwrap();
    let rows: Vec<(String, String, Option<i64>, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let mut out = Vec::new();
    for (id, name, color, order) in rows {
        let taskCount: i64 = conn
            .query_row(
                "SELECT count(*) FROM task_tags WHERE tag_id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let goalCount: i64 = conn
            .query_row(
                "SELECT count(*) FROM goal_tags WHERE tag_id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let noteCount: i64 = conn
            .query_row(
                "SELECT count(*) FROM note_tags WHERE tag_id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        out.push(serde_json::json!({"id":id,"name":name,"color":color,"sortOrder":order,"taskCount":taskCount,"goalCount":goalCount,"noteCount":noteCount}));
    }
    CommandResult {
        success: true,
        data: Some(out),
        error: None,
    }
}
#[tauri::command]
fn list_accounts(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("{e}")),
            };
        }
    };
    let mut stmt = conn
        .prepare(
            "SELECT id, name, type, opening_balance, note, created_at, source, external_id
             FROM accounts WHERE deleted_at IS NULL ORDER BY name",
        )
        .unwrap();
    let rows: Vec<serde_json::Value> = stmt
        .query_map([], |r| {
            let id: String = r.get(0)?;
            // The balance is computed per account with the same
            // opening-plus-net rule the Insights chart uses, so the list view
            // and the charts cannot disagree about what an account is worth.
            let balance = compute_current_balance(&conn, &id).unwrap_or(f64::NAN);
            Ok(serde_json::json!({
                "id": id,
                "name": r.get::<_, String>(1)?,
                "type": r.get::<_, String>(2)?,
                "openingBalance": r.get::<_, f64>(3)?,
                "note": r.get::<_, Option<String>>(4)?,
                "createdAt": r.get::<_, i64>(5)?,
                // Accounts created by a statement import carry their provider
                // identity; hand-created ones have neither.
                "source": r.get::<_, Option<String>>(6)?,
                "externalId": r.get::<_, Option<String>>(7)?,
                "balance": balance,
            }))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}
#[tauri::command]
fn list_transactions(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("{e}")),
            };
        }
    };
    // Both sides are named straight from the join: a transfer without its
    // account names ("transfer 500") told the reader nothing about where the
    // money moved, and a spend without the account it left is equally blind.
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.type, t.amount, t.currency_code, t.amount_base, t.rate_used,
                    t.category, t.date, t.note, t.receipt_id, t.is_draft,
                    t.account_id, t.to_account_id, a.name, b.name
             FROM transactions t
             LEFT JOIN accounts a ON t.account_id = a.id
             LEFT JOIN accounts b ON t.to_account_id = b.id
             WHERE t.deleted_at IS NULL
             ORDER BY t.date DESC LIMIT 500",
        )
        .unwrap();
    let rows: Vec<serde_json::Value> = stmt
        .query_map([], |r| {
            let account_id: Option<String> = r.get(11)?;
            let note: Option<String> = r.get(8)?;
            let category: Option<String> = r.get(6)?;
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "type": r.get::<_, String>(1)?,
                "amount": r.get::<_, f64>(2)?,
                "currencyCode": r.get::<_, String>(3)?,
                "amountBase": r.get::<_, f64>(4)?,
                "rateUsed": r.get::<_, f64>(5)?,
                "category": category,
                "date": r.get::<_, i64>(7)?,
                "note": note,
                "receiptId": r.get::<_, Option<String>>(9)?,
                "isDraft": r.get::<_, i64>(10)? != 0,
                "accountId": account_id,
                "toAccountId": r.get::<_, Option<String>>(12)?,
                "accountName": r.get::<_, Option<String>>(13)?,
                "toAccountName": r.get::<_, Option<String>>(14)?,
            }))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}
#[tauri::command]
fn list_fx_rates(
    state: tauri::State<AppState>,
    base: Option<String>,
    date: Option<String>,
) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("{e}")),
            };
        }
    };
    let b = base.unwrap_or_else(|| "USD".to_string());
    let d = date.unwrap_or_else(|| "".to_string());
    let mut sql =
        "SELECT code, base_code, rate_date, rate_to_base, updated_at, manual FROM fx_rates"
            .to_string();
    let mut wh: Vec<String> = Vec::new();
    if !d.is_empty() {
        wh.push(format!("rate_date='{}'", d.replace('\'', "''")));
    }
    if !b.is_empty() {
        wh.push(format!("base_code='{}'", b.replace('\'', "''")));
    }
    if !wh.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&wh.join(" AND "));
    }
    sql.push_str(" ORDER BY updated_at DESC LIMIT 200");
    let mut stmt = conn.prepare(&sql).unwrap();
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"code":r.get::<_,String>(0)?,"baseCode":r.get::<_,String>(1)?,"rateDate":r.get::<_,String>(2)?,"rateToBase":r.get::<_,f64>(3)?,"updatedAt":r.get::<_,i64>(4)?,"manual":r.get::<_,i64>(5)?!=0}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult {
        success: true,
        data: Some(rows),
        error: None,
    }
}

// ---- Insights commands ----
//
// Every chart on the Insights screen (desktop-visualization-expansion plan)
// reads straight from `maia.db` the same way the rest of this file does --
// no new abstraction layer, just more `#[tauri::command]`s. The bucketing and
// running-balance math is factored into plain functions that take a
// `&Connection` (rather than `tauri::State`) so the tests at the bottom of
// this file can seed an in-memory database and call them directly, without
// going through Tauri's command dispatch.

/// Buckets an epoch-millisecond timestamp into a sortable period key.
///
/// Weeks start on Monday -- picked once here and reused by every chart below,
/// since mixing a Monday-start convention in one chart with a Sunday-start in
/// another would make two independently-correct charts visually disagree
/// about what "this week" means. "day" (and anything else unrecognized)
/// passes through as a plain calendar day.
fn bucket_key(epoch_ms: i64, period: &str) -> String {
    let date = chrono::DateTime::from_timestamp_millis(epoch_ms)
        .unwrap_or_default()
        .naive_utc()
        .date();
    match period {
        "month" => date.format("%Y-%m-01").to_string(),
        "week" => {
            let monday = date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64);
            monday.format("%Y-%m-%d").to_string()
        }
        _ => date.format("%Y-%m-%d").to_string(),
    }
}

/// Weight/reps progression for one exercise, chronological.
///
/// Prefers `actual_*` (what was actually done) over the planned `weight_kg`/
/// `reps`, matching how the rest of the app treats planned-vs-actual. A set
/// with neither -- bodyweight or cardio work, where weight does not apply --
/// is skipped rather than plotted as a false zero.
fn compute_exercise_progression(
    conn: &Connection,
    exercise_id: &str,
) -> rusqlite::Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT w.date, es.weight_kg, es.reps, es.actual_weight_kg, es.actual_reps \
         FROM exercise_sets es \
         JOIN workout_exercises we ON we.id = es.workout_exercise_id \
         JOIN workouts w ON w.id = we.workout_id \
         WHERE we.exercise_id = ?1 AND es.deleted_at IS NULL AND we.deleted_at IS NULL \
           AND w.deleted_at IS NULL \
         ORDER BY w.date ASC",
    )?;
    let rows: Vec<(i64, Option<f64>, Option<i64>, Option<f64>, Option<i64>)> = stmt
        .query_map(params![exercise_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows
        .into_iter()
        .filter_map(|(date, weight, reps, aweight, areps)| {
            let weight = aweight.or(weight)?;
            let reps = areps.or(reps);
            Some(serde_json::json!({"date": date, "weightKg": weight, "reps": reps}))
        })
        .collect())
}

/// Training volume (Σ weight_kg * reps, actual preferred over planned) per
/// workout, summed into day/week/month buckets.
fn compute_exercise_volume(conn: &Connection, period: &str) -> rusqlite::Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT w.id, w.date, es.weight_kg, es.reps, es.actual_weight_kg, es.actual_reps \
         FROM exercise_sets es \
         JOIN workout_exercises we ON we.id = es.workout_exercise_id \
         JOIN workouts w ON w.id = we.workout_id \
         WHERE es.deleted_at IS NULL AND we.deleted_at IS NULL AND w.deleted_at IS NULL \
         ORDER BY w.date ASC",
    )?;
    let rows: Vec<(String, i64, Option<f64>, Option<i64>, Option<f64>, Option<i64>)> = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Per-workout volume first -- the chart buckets whole workouts, not sets
    // -- then each workout's single volume number drops into its date's
    // bucket.
    let mut workout_date: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut workout_volume: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for (workout_id, date, weight, reps, aweight, areps) in rows {
        let w = aweight.or(weight).unwrap_or(0.0);
        let r = areps.or(reps).unwrap_or(0) as f64;
        workout_date.entry(workout_id.clone()).or_insert(date);
        *workout_volume.entry(workout_id).or_insert(0.0) += w * r;
    }
    let mut totals: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for (workout_id, volume) in workout_volume {
        let key = bucket_key(workout_date[&workout_id], period);
        *totals.entry(key).or_insert(0.0) += volume;
    }
    Ok(totals
        .into_iter()
        .map(|(bucket, volume_kg)| serde_json::json!({"bucket": bucket, "volumeKg": volume_kg}))
        .collect())
}

/// One averaged nutrition row per bucket: the mean of the *per-day totals*
/// over the distinct days with logged meals in that bucket.
struct NutritionAverages {
    bucket: String,
    /// Days carried into the averages (the divisor).
    days: u32,
    calories: f64,
    protein: f64,
    carbs: f64,
    fat: f64,
}

/// Calories and macros scaled per day. For a `day` bucket the average over
/// its single day is just the day's total. For week/month buckets, summing
/// every meal in the bucket would print bigger-and-bigger numbers as history
/// accumulates, so the chart gets the average daily total instead. A day
/// with no meals does not dilute the average: skipped logging days are
/// anomalous, not dietary, and averaging over 7 calendar days would make a
/// week containing 3 logged days look like a winning diet.
fn compute_nutrition_averages(conn: &Connection, period: &str) -> rusqlite::Result<Vec<NutritionAverages>> {
    let mut stmt = conn.prepare(
        "SELECT m.eaten_at, mf.calories, mf.protein, mf.carbs, mf.fat FROM meal_foods mf \
         JOIN meals m ON m.id = mf.meal_id \
         WHERE mf.deleted_at IS NULL AND m.deleted_at IS NULL \
         ORDER BY m.eaten_at ASC",
    )?;
    let rows: Vec<(i64, f64, f64, f64, f64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .filter_map(|r| r.ok())
        .collect();

    // First collapse every meal into per-day totals, so each day counts once
    // regardless of how many meals were logged on it.
    let mut by_day: std::collections::BTreeMap<String, (f64, f64, f64, f64)> = std::collections::BTreeMap::new();
    for (eaten_at, calories, protein, carbs, fat) in rows {
        let entry = by_day
            .entry(bucket_key(eaten_at, "day"))
            .or_insert((0.0, 0.0, 0.0, 0.0));
        entry.0 += calories;
        entry.1 += protein;
        entry.2 += carbs;
        entry.3 += fat;
    }

    // Then average each bucket's day totals.
    let mut by_bucket: std::collections::BTreeMap<String, (f64, f64, f64, f64, u32)> = std::collections::BTreeMap::new();
    for (day, (calories, protein, carbs, fat)) in by_day {
        // `day` is already a normalized `YYYY-MM-DD` key, so re-bucketing it
        // needs no timestamp round-trip.
        let key = bucket_key_from_date(&day, period);
        let entry = by_bucket.entry(key).or_insert((0.0, 0.0, 0.0, 0.0, 0));
        entry.0 += calories;
        entry.1 += protein;
        entry.2 += carbs;
        entry.3 += fat;
        entry.4 += 1;
    }

    Ok(by_bucket
        .into_iter()
        .map(|(bucket, (calories, protein, carbs, fat, days))| {
            let n = days as f64;
            NutritionAverages {
                bucket,
                days,
                calories: calories / n,
                protein: protein / n,
                carbs: carbs / n,
                fat: fat / n,
            }
        })
        .collect())
}

/// Buckets an already-normalized `YYYY-MM-DD` date string without re-parsing
/// it through a timestamp (`bucket_key` takes epoch millis). On malformed
/// input the day is passed through unchanged rather than inventing 1970.
fn bucket_key_from_date(day: &str, period: &str) -> String {
    match period {
        "month" => format!("{}-01", &day[..7]),
        "week" => match chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d") {
            Ok(date) => {
                (date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64))
                    .format("%Y-%m-%d")
                    .to_string()
            }
            Err(_) => day.to_string(),
        },
        _ => day.to_string(),
    }
}

fn compute_nutrition_calories(conn: &Connection, period: &str) -> rusqlite::Result<Vec<serde_json::Value>> {
    Ok(compute_nutrition_averages(conn, period)?
        .into_iter()
        .map(|a| serde_json::json!({"bucket": a.bucket, "calories": a.calories}))
        .collect())
}

/// Protein/carbs/fat logged per day, as three parallel series. Week/month
/// buckets are averaged daily values, mirroring `compute_nutrition_calories`.
fn compute_nutrition_macros(conn: &Connection, period: &str) -> rusqlite::Result<Vec<serde_json::Value>> {
    Ok(compute_nutrition_averages(conn, period)?
        .into_iter()
        .map(|a| {
            serde_json::json!({"bucket": a.bucket, "protein": a.protein, "carbs": a.carbs, "fat": a.fat})
        })
        .collect())
}

/// Expense totals (in `amount_base`, already currency-converted) grouped by
/// category per day/week/month. Income and transfers are not spending, so
/// only `type='expense'` rows count; a null/empty category becomes
/// "Uncategorized" rather than being dropped.
fn compute_category_totals(conn: &Connection, period: &str) -> rusqlite::Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT date, category, amount_base FROM transactions \
         WHERE type = 'expense' AND deleted_at IS NULL \
         ORDER BY date ASC",
    )?;
    let rows: Vec<(i64, Option<String>, f64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();
    let mut totals: std::collections::BTreeMap<(String, String), f64> = std::collections::BTreeMap::new();
    for (date, category, amount_base) in rows {
        let category = category
            .filter(|c| !c.trim().is_empty())
            .unwrap_or_else(|| "Uncategorized".to_string());
        let key = (bucket_key(date, period), category);
        *totals.entry(key).or_insert(0.0) += amount_base;
    }
    Ok(totals
        .into_iter()
        .map(|((bucket, category), total)| serde_json::json!({"bucket": bucket, "category": category, "total": total}))
        .collect())
}

/// One raw transaction row relevant to a single account's running balance:
/// `(date, type, amount_base, account_id, to_account_id)`.
type BalanceRow = (i64, String, f64, Option<String>, Option<String>);

/// The running-balance math, factored out of `compute_account_balance` so it
/// can be unit tested against seeded rows without a database.
///
/// Starts at `opening` and walks `rows` (already ordered by date) in order.
/// `amount`/`amount_base` are always stored as a positive magnitude -- see
/// `create_transaction` in `src/statements/store.rs` -- so the sign has to
/// come from `type` and from which side of a transfer this account is on:
/// `account_id` is the transfer's source (debit) and `to_account_id` is its
/// destination (credit). Get that backwards and every balance after the
/// first transfer is wrong, which is exactly the gotcha the plan calls out.
///
/// Only the last balance observed in each period bucket is kept, which is
/// what a day/week/month toggle on a running balance should show: where the
/// balance *ended up* by the end of that bucket, not a sum of changes within
/// it.
fn running_balance_series(
    opening: f64,
    rows: &[BalanceRow],
    account_id: &str,
    period: &str,
) -> Vec<serde_json::Value> {
    let mut balance = opening;
    let mut order: Vec<String> = Vec::new();
    let mut last: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for (date, type_, amount_base, from, to) in rows {
        match type_.as_str() {
            "income" if from.as_deref() == Some(account_id) => balance += amount_base,
            "expense" if from.as_deref() == Some(account_id) => balance -= amount_base,
            "transfer" => {
                if from.as_deref() == Some(account_id) {
                    balance -= amount_base;
                }
                if to.as_deref() == Some(account_id) {
                    balance += amount_base;
                }
            }
            _ => {}
        }
        let key = bucket_key(*date, period);
        if !last.contains_key(&key) {
            order.push(key.clone());
        }
        last.insert(key, balance);
    }
    order
        .into_iter()
        .map(|key| serde_json::json!({"bucket": key, "balance": last[&key]}))
        .collect()
}

/// Current balance of one account: its opening balance plus the net effect
/// of every live transaction that touches it — the same accounting used by
/// [`running_balance_series`], collapsed to the last value.
///
/// Kept as a standalone fold (rather than calling the series) so a list of
/// accounts can be priced without building every chart bucket.
fn compute_current_balance(conn: &Connection, account_id: &str) -> Result<f64, String> {
    let opening: f64 = conn
        .query_row(
            "SELECT opening_balance FROM accounts WHERE id = ?1 AND deleted_at IS NULL",
            params![account_id],
            |r| r.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => "Account not found".to_string(),
            e => format!("{e}"),
        })?;
    Ok(opening + account_net_effect(conn, account_id)?)
}

/// The net effect of every live transaction that touches one account —
/// income and inbound transfers add, expenses and outbound transfers
/// subtract, each at [amount_base]. The shared accounting between the list
/// view's prices and re-basing an account on its *current* balance: the
/// distance from opening, both directions.
fn account_net_effect(conn: &Connection, account_id: &str) -> Result<f64, String> {
    let mut stmt = conn
        .prepare(
            "SELECT type, amount_base, account_id, to_account_id FROM transactions \
             WHERE deleted_at IS NULL AND (account_id = ?1 OR to_account_id = ?1)",
        )
        .map_err(|e| format!("{e}"))?;
    let mut net = 0.0;
    let mut rows = stmt
        .query(params![account_id])
        .map_err(|e| format!("{e}"))?;
    while let Some(row) = rows.next().map_err(|e| format!("{e}"))? {
        let type_: String = row.get(0).map_err(|e| format!("{e}"))?;
        let amount_base: f64 = row.get(1).map_err(|e| format!("{e}"))?;
        let from: Option<String> = row.get(2).map_err(|e| format!("{e}"))?;
        let to: Option<String> = row.get(3).map_err(|e| format!("{e}"))?;
        match type_.as_str() {
            "income" if from.as_deref() == Some(account_id) => net += amount_base,
            "expense" if from.as_deref() == Some(account_id) => net -= amount_base,
            "transfer" => {
                if from.as_deref() == Some(account_id) {
                    net -= amount_base;
                }
                if to.as_deref() == Some(account_id) {
                    net += amount_base;
                }
            }
            _ => {}
        }
    }
    Ok(net)
}

/// Loads one account's opening balance and every transaction that touches it
/// (as either side), then hands off to `running_balance_series`.
fn compute_account_balance(
    conn: &Connection,
    account_id: &str,
    period: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let opening: f64 = conn
        .query_row(
            "SELECT opening_balance FROM accounts WHERE id = ?1 AND deleted_at IS NULL",
            params![account_id],
            |r| r.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => "Account not found".to_string(),
            e => format!("{e}"),
        })?;
    let mut stmt = conn
        .prepare(
            "SELECT date, type, amount_base, account_id, to_account_id FROM transactions \
             WHERE deleted_at IS NULL AND (account_id = ?1 OR to_account_id = ?1) \
             ORDER BY date ASC",
        )
        .map_err(|e| format!("{e}"))?;
    let rows: Vec<BalanceRow> = stmt
        .query_map(params![account_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(|e| format!("{e}"))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(running_balance_series(opening, &rows, account_id, period))
}

#[tauri::command]
fn exercise_progression(
    state: tauri::State<AppState>,
    // camelCase to match the JS invoke arg literally, per house style (see
    // `add_template_set`'s `templateExerciseId` etc.) -- this binding is not
    // auto-converted from the frontend's argument key.
    exerciseId: String,
) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("lock {e}")) },
    };
    match compute_exercise_progression(&conn, &exerciseId) {
        Ok(rows) => CommandResult { success: true, data: Some(rows), error: None },
        Err(e) => CommandResult { success: false, data: None, error: Some(format!("{e}")) },
    }
}

#[tauri::command]
fn exercise_volume(state: tauri::State<AppState>, period: String) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("lock {e}")) },
    };
    match compute_exercise_volume(&conn, &period) {
        Ok(rows) => CommandResult { success: true, data: Some(rows), error: None },
        Err(e) => CommandResult { success: false, data: None, error: Some(format!("{e}")) },
    }
}

#[tauri::command]
fn nutrition_calories(state: tauri::State<AppState>, period: String) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("lock {e}")) },
    };
    match compute_nutrition_calories(&conn, &period) {
        Ok(rows) => CommandResult { success: true, data: Some(rows), error: None },
        Err(e) => CommandResult { success: false, data: None, error: Some(format!("{e}")) },
    }
}

#[tauri::command]
fn nutrition_macros(state: tauri::State<AppState>, period: String) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("lock {e}")) },
    };
    match compute_nutrition_macros(&conn, &period) {
        Ok(rows) => CommandResult { success: true, data: Some(rows), error: None },
        Err(e) => CommandResult { success: false, data: None, error: Some(format!("{e}")) },
    }
}

#[tauri::command]
fn finance_category_totals(
    state: tauri::State<AppState>,
    period: String,
) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("lock {e}")) },
    };
    match compute_category_totals(&conn, &period) {
        Ok(rows) => CommandResult { success: true, data: Some(rows), error: None },
        Err(e) => CommandResult { success: false, data: None, error: Some(format!("{e}")) },
    }
}

#[tauri::command]
fn finance_account_balance(
    state: tauri::State<AppState>,
    accountId: String,
    period: String,
) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("lock {e}")) },
    };
    match compute_account_balance(&conn, &accountId, &period) {
        Ok(rows) => CommandResult { success: true, data: Some(rows), error: None },
        Err(e) => CommandResult { success: false, data: None, error: Some(e) },
    }
}

#[tauri::command]
async fn create_store(name: String) -> CommandResult<serde_json::Value> {
    let id = uuid::Uuid::now_v7().to_string();
    push_ok(
        "/stores",
        &serde_json::json!({
            "stores": [{ "id": id, "name": name, "createdAt": now_ms() }],
            "deleted": [],
        }),
        serde_json::json!({ "id": id, "name": name }),
    )
    .await
}

#[tauri::command]
async fn create_fx_rate(
    code: String,
    baseCode: String,
    rateDate: String,
    rateToBase: f64,
) -> CommandResult<serde_json::Value> {
    // A rate entered by hand stays manual; the flag is carried explicitly so a
    // later pull cannot quietly turn it into a fetched one.
    push_ok(
        "/fx-rates",
        &serde_json::json!({
            "rates": [{
                "code": code, "baseCode": baseCode, "rateDate": rateDate,
                "rateToBase": rateToBase, "manual": true,
            }],
            "deleted": [],
        }),
        serde_json::json!({ "code": code }),
    )
    .await
}
// --- Finance scenarios ------------------------------------------------------
//
// Same story as statements: the server is the only writer, so these are thin
// HTTP wrappers over `/finance-scenarios`.

#[tauri::command]
async fn list_finance_scenarios() -> CommandResult<serde_json::Value> {
    json_result(api::get("/finance-scenarios").await)
}

#[tauri::command]
async fn create_finance_scenario(
    name: String,
    assumptions: serde_json::Value,
) -> CommandResult<serde_json::Value> {
    json_result(
        api::post(
            "/finance-scenarios",
            &serde_json::json!({ "name": name, "assumptions": assumptions }),
        )
        .await,
    )
}

#[tauri::command]
async fn update_finance_scenario(
    id: String,
    name: String,
    assumptions: serde_json::Value,
) -> CommandResult<serde_json::Value> {
    json_result(
        api::put(
            &format!("/finance-scenarios/{id}"),
            &serde_json::json!({ "name": name, "assumptions": assumptions }),
        )
        .await,
    )
}

#[tauri::command]
async fn delete_finance_scenario(id: String) -> CommandResult<serde_json::Value> {
    json_result(api::delete(&format!("/finance-scenarios/{id}")).await)
}

#[tauri::command]
async fn get_finance_baseline(windowMonths: Option<u32>) -> CommandResult<serde_json::Value> {
    let path = match windowMonths {
        Some(months) => format!("/finance-scenarios/baseline?windowMonths={months}"),
        None => "/finance-scenarios/baseline".to_string(),
    };
    json_result(api::get(&path).await)
}

#[tauri::command]
async fn project_finance_scenario(
    scenarioId: Option<String>,
    assumptions: Option<serde_json::Value>,
    windowMonths: Option<u32>,
) -> CommandResult<serde_json::Value> {
    json_result(
        api::post(
            "/finance-scenarios/project",
            &serde_json::json!({
                "scenarioId": scenarioId,
                "assumptions": assumptions,
                "windowMonths": windowMonths,
            }),
        )
        .await,
    )
}

// --- Statements -----------------------------------------------------------
//
// Staging lives on the server: it parses files and provider feeds, and it is
// the only process that may write the database. The desktop review screen is a
// thin client over those endpoints, like the phone.
//
// The one exception below is the drop form: the webview's file input cannot
// hand over absolute paths (like every web sandbox), so the picked bytes
// travel through the command and land in the server's dump folder, where the
// watcher takes over. Nothing here parses or imports anything.

fn json_result(
    result: Result<serde_json::Value, api::ApiError>,
) -> CommandResult<serde_json::Value> {
    match result {
        Ok(value) => CommandResult {
            success: true,
            data: Some(value),
            error: None,
        },
        Err(error) => CommandResult {
            success: false,
            data: None,
            error: Some(error.to_message()),
        },
    }
}

/// Whitelists the list filters, so a value from the webview cannot turn into
/// arbitrary query syntax. These are the only statuses and sources the server
/// produces.
fn statement_list_path(status: Option<&str>, source: Option<&str>) -> String {
    let mut path = String::from("/statements");
    let mut separator = '?';
    if let Some(status) = status.filter(|value| {
        matches!(
            *value,
            "staged" | "partial" | "imported" | "ignored" | "error"
        )
    }) {
        path.push(separator);
        path.push_str("status=");
        path.push_str(status);
        separator = '&';
    }
    if let Some(source) =
        source.filter(|value| matches!(*value, "credit_agricole" | "trading212" | "firstrade"))
    {
        path.push(separator);
        path.push_str("source=");
        path.push_str(source);
    }
    path
}

#[tauri::command]
async fn list_statements(
    status: Option<String>,
    source: Option<String>,
) -> CommandResult<serde_json::Value> {
    let path = statement_list_path(status.as_deref(), source.as_deref());
    json_result(api::get(&path).await)
}

#[tauri::command]
async fn get_statement(id: String) -> CommandResult<serde_json::Value> {
    json_result(api::get(&format!("/statements/{id}")).await)
}

/// Percent-encodes a query value, keeping the unreserved set literal.
///
/// A dropped file's path carries directories and possibly non-ASCII names, so
/// it cannot go into a URL raw.
fn url_query_value(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// One dropped file's statements: every account the parser staged from it.
#[tauri::command]
async fn get_statement_file(path: String) -> CommandResult<serde_json::Value> {
    let path = format!("/statements/file?path={}", url_query_value(&path));
    json_result(api::get(&path).await)
}

/// Imports pending lines of several statements in one server transaction.
///
/// Each entry is `{ id, lines, autoLink }`, the same shape the single-statement
/// import takes; the batch form keeps a file-level selection atomic.
#[tauri::command]
async fn import_statements(statements: Vec<serde_json::Value>) -> CommandResult<serde_json::Value> {
    json_result(
        api::post(
            "/statements/import",
            &serde_json::json!({ "statements": statements }),
        )
        .await,
    )
}

/// Soft-deletes several statements in one server transaction.
#[tauri::command]
async fn delete_statements(ids: Vec<String>) -> CommandResult<serde_json::Value> {
    json_result(api::post("/statements/delete", &serde_json::json!({ "ids": ids })).await)
}

#[tauri::command]
async fn import_statement(
    id: String,
    lines: Vec<serde_json::Value>,
    auto_link: Option<bool>,
) -> CommandResult<serde_json::Value> {
    let body = serde_json::json!({
        "lines": lines,
        "autoLink": auto_link.unwrap_or(true),
    });
    json_result(api::post(&format!("/statements/{id}/import"), &body).await)
}

#[tauri::command]
async fn ignore_statement(id: String) -> CommandResult<serde_json::Value> {
    json_result(api::post(&format!("/statements/{id}/ignore"), &serde_json::json!({})).await)
}

#[tauri::command]
async fn delete_statement(id: String) -> CommandResult<serde_json::Value> {
    json_result(api::delete(&format!("/statements/{id}")).await)
}

#[tauri::command]
async fn scan_statements() -> CommandResult<serde_json::Value> {
    json_result(api::post("/statements/scan", &serde_json::json!({})).await)
}

#[tauri::command]
async fn list_statement_feeds() -> CommandResult<serde_json::Value> {
    json_result(api::get("/statement-feeds").await)
}

#[tauri::command]
async fn get_statement_feed_settings() -> CommandResult<serde_json::Value> {
    json_result(api::get("/statement-feeds/settings").await)
}

/// Writes the Trading 212 credentials to the per-user overlay on the server.
/// The command never reads them back: the form only ever learns whether a
/// secret is set, so a screen share or a screenshot cannot leak it.
#[tauri::command]
async fn save_statement_feed_settings(
    settings: serde_json::Value,
) -> CommandResult<serde_json::Value> {
    json_result(api::put("/statement-feeds/settings", &settings).await)
}

#[tauri::command]
async fn sync_statement_feed(provider: String) -> CommandResult<serde_json::Value> {
    json_result(
        api::post(
            &format!("/statement-feeds/{provider}/sync"),
            &serde_json::json!({}),
        )
        .await,
    )
}

/// One file the drop form picked, with its content (base64).
#[derive(serde::Deserialize)]
struct StatementDrop {
    name: String,
    /// Base64 of the file contents, as `FileReader.readAsDataURL` produced it.
    data: String,
}

/// `export.csv` + suffix 2 becomes `export-2.csv`, keeping the extension
/// recognizable. Mirrors the server's own rule for handled files.
fn disambiguate_dump_name(name: &str, suffix: usize) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{stem}-{suffix}.{extension}"),
        _ => format!("{name}-{suffix}"),
    }
}

/// Writes one dropped file into `<dump_dir>/<source>/<name>`.
///
/// Only the final path component of the picked name writes: a name carrying
/// directories would otherwise escape the dump tree. A name already taken
/// gets a numeric suffix rather than an overwrite -- the server may be about
/// to move that exact file out from under us.
///
/// Returns where the file landed, so the banner can show the real path.
fn place_statement_file(
    dump_root: &std::path::Path,
    source: maia::statements::SourceId,
    raw_name: &str,
    bytes: &[u8],
) -> Result<std::path::PathBuf, String> {
    let name = raw_name.rsplit(['/', '\\']).next().unwrap_or("").trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err(format!("{raw_name:?} has no usable file name"));
    }
    if !name.contains('.') {
        return Err(format!(
            "{name} has no extension; statement files are CSV exports"
        ));
    }

    let dir = dump_root.join(source.as_str());
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("creating {}: {error}", dir.display()))?;
    let mut target = dir.join(name);
    let mut suffix = 1;
    while target.exists() {
        suffix += 1;
        target = dir.join(disambiguate_dump_name(name, suffix));
    }
    std::fs::write(&target, bytes)
        .map_err(|error| format!("writing {}: {error}", target.display()))?;
    Ok(target)
}

/// Drops picked statement files into the dump folder for one source.
///
/// The server's watcher (inotify, with a slow sweep as backstop) picks them up
/// from here; this command deliberately stops at placing the bytes. It neither
/// triggers a scan nor touches the database.
#[tauri::command]
fn add_statement_files(source: String, files: Vec<StatementDrop>) -> CommandResult<Vec<String>> {
    let Some(source_id) = maia::statements::SourceId::parse(&source) else {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("unknown statement source: {source}")),
        };
    };
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::STANDARD;
    let dump_root = sync_config().statements_dump_dir();
    let mut placed = Vec::new();
    for file in files {
        let bytes = match engine.decode(file.data.as_bytes()) {
            Ok(bytes) => bytes,
            Err(error) => {
                return CommandResult {
                    success: false,
                    data: None,
                    error: Some(format!("decoding {}: {error}", file.name)),
                };
            }
        };
        match place_statement_file(&dump_root, source_id, &file.name, &bytes) {
            Ok(path) => placed.push(path.display().to_string()),
            Err(error) => {
                return CommandResult {
                    success: false,
                    data: None,
                    error: Some(error),
                };
            }
        }
    }
    CommandResult {
        success: true,
        data: Some(placed),
        error: None,
    }
}

/// One receipt file the import form picked, base64 as `FileReader` produced it.
#[derive(serde::Deserialize)]
struct ReceiptDrop {
    name: String,
    data: String,
}

/// Uploads one or many receipt pictures to the server's OCR endpoint.
///
/// One HTTP request per file — the endpoint keeps a single file part, so
/// "many receipts" cannot ride one body. Results are per-name: a failing
/// upload reports there (with the decode error, or the server's own message
/// such as "ocr not configured") instead of failing the whole batch.
#[tauri::command]
async fn upload_receipt_files(files: Vec<ReceiptDrop>) -> CommandResult<Vec<serde_json::Value>> {
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut results: Vec<serde_json::Value> = Vec::new();
    for file in files {
        let upload = match engine.decode(api::strip_data_url(&file.data).as_bytes()) {
            Ok(bytes) => api::UploadFile {
                name: file.name.clone(),
                bytes,
            },
            Err(e) => {
                results.push(serde_json::json!({
                    "name": file.name, "id": null,
                    "error": format!("could not decode the image: {e}"),
                }));
                continue;
            }
        };
        match api::post_multipart::<serde_json::Value>("/receipt-pictures/upload", &[], &upload)
            .await
        {
            Ok(response) => {
                let id = response
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if id.is_some() {
                    results.push(serde_json::json!({"name": file.name, "id": id, "error": null}));
                } else {
                    results.push(serde_json::json!({
                        "name": file.name, "id": null,
                        "error": "upload succeeded but the response carried no receipt id",
                    }));
                }
            }
            Err(error) => results.push(
                serde_json::json!({"name": file.name, "id": null, "error": error.to_message()}),
            ),
        }
    }
    CommandResult {
        success: true,
        data: Some(results),
        error: None,
    }
}

/// Saves edits to one account — name, type, note, and **the current balance
/// the account holds today**, which is what a person actually knows; the
/// opening balance is not asked for anywhere.
///
/// The stored opening balance is derived: it becomes the typed current
/// balance minus the net effect of the account's live transactions, so the
/// ledger is untouched and every figure computed from opening + net — the
/// list badge, the detail page, the balance chart — lands exactly on the
/// number the user typed.
///
/// The desktop database is read-only, so the edit travels server-ward as a
/// client-wins push of the whole row (the shape `handlers::push::AccountPush`
/// expects); the server's upsert ignores `source`/`external_id` on conflict,
/// keeping statement-linked accounts wired to their provider.
#[tauri::command]
async fn save_account(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
    account_type: String,
    current_balance: f64,
    note: String,
) -> Result<CommandResult<serde_json::Value>, String> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Account name cannot be empty".to_string()),
        });
    }
    if !current_balance.is_finite() {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Current balance must be a number".to_string()),
        });
    }

    // Short-lived read lock: row facts plus the net that re-bases the
    // opening balance; nothing held across the awaits below.
    let (old_type, created_at, deleted_at, opening_balance, net) = {
        let conn = state.db.lock().map_err(|e| format!("{e}"))?;
        let row: Option<(String, i64, Option<i64>)> = conn
            .query_row(
                "SELECT type, created_at, deleted_at FROM accounts WHERE id = ?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();
        let Some((old_type, created_at, deleted_at)) = row else {
            return Ok(CommandResult {
                success: false,
                data: None,
                error: Some(format!("Account {id} not found")),
            });
        };
        if deleted_at.is_some() {
            return Ok(CommandResult {
                success: false,
                data: None,
                error: Some("Account is deleted".to_string()),
            });
        }
        let net = account_net_effect(&conn, &id)?;
        (old_type, created_at, deleted_at, current_balance - net, net)
    };

    let body = serde_json::json!({
        "accounts": [{
            "id": id,
            "name": trimmed,
            "type": if account_type.trim().is_empty() { old_type.as_str() } else { account_type.trim() },
            "openingBalance": opening_balance,
            "note": if note.trim().is_empty() { serde_json::Value::Null } else { serde_json::Value::String(note.trim().to_string()) },
            "createdAt": created_at,
            "deletedAt": deleted_at,
        }],
        "deleted": [],
    });
    let _ = api::post::<_, serde_json::Value>("/budget-accounts", &body)
        .await
        .map_err(|e| e.to_message())?;

    Ok(CommandResult {
        success: true,
        data: Some(serde_json::json!({"saved": true, "openingBalance": opening_balance, "net": net})),
        error: None,
    })
}

/// Epoch-milliseconds from the OCR date text, whatever shape it came in
/// (RFC 3339, both naive splits, or a bare date), falling back to
/// [fallback_ms]. Noon on a bare date — a receipt is a daytime artifact.
fn parse_receipt_date(text: &str, fallback_ms: i64) -> i64 {
    let text = text.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(text) {
        return dt.timestamp_millis();
    }
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(text, format) {
            return dt.and_utc().timestamp_millis();
        }
        if let Ok(date) = chrono::NaiveDate::parse_from_str(text, format) {
            if let Some(dt) = date.and_hms_opt(12, 0, 0) {
                return dt.and_utc().timestamp_millis();
            }
        }
    }
    fallback_ms
}

/// `YYYY-MM-DD` for the FX-rate lookup, on UTC days.
fn rate_date_of(timestamp_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(timestamp_ms)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// The newest rate of [code] against [base] on or before the receipt's day —
/// the same rule the server's statement import applies. 1.0 when the currency
/// is already the base, or when none is recorded: a missing rate must not
/// block the expense from existing (matching mobile's fallback).
fn rate_to_base(conn: &rusqlite::Connection, code: &str, base: &str, date: &str) -> f64 {
    if code.eq_ignore_ascii_case(base) {
        return 1.0;
    }
    conn.query_row(
        "SELECT rate_to_base FROM fx_rates
         WHERE code=?1 AND base_code=?2 AND rate_date<=?3 AND deleted_at IS NULL
         ORDER BY rate_date DESC LIMIT 1",
        rusqlite::params![code, base, date],
        |r| r.get::<_, f64>(0),
    )
    .unwrap_or(1.0)
}

/// Creates the draft expense a parsed receipt stands for, and links the
/// receipt to it — the same capture flow the phone performs locally
/// (upload → OCR → draft), except the transaction is pushed through the
/// server, the only writer.
///
/// Idempotent: an already-linked receipt is reported untouched, and one whose
/// linking push failed is re-linked to the transaction the first attempt
/// founded (found by `receipt_id`) rather than a second one being made.
#[tauri::command]
async fn create_receipt_transaction(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<CommandResult<serde_json::Value>, String> {
    let receipt = match load_receipt(&state, &id) {
        Ok(r) => r,
        Err(e) => {
            return Ok(CommandResult {
                success: false,
                data: None,
                error: Some(e),
            });
        }
    };

    let parsed: serde_json::Value = match receipt
        .parsed_json
        .as_deref()
        .filter(|_| receipt.parsed)
        .map(serde_json::from_str)
    {
        Some(Ok(v)) => v,
        Some(Err(e)) => {
            return Ok(CommandResult {
                success: false,
                data: None,
                error: Some(format!("receipt is parsed but unreadable: {e}")),
            });
        }
        _ => {
            return Ok(CommandResult {
                success: false,
                data: None,
                error: Some("receipt has not been parsed yet".to_string()),
            });
        }
    };
    let total = parsed.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0);
    // A total at or below zero is an OCR miss, not an expense; leave the
    // linking for the user rather than book money that was never spent.
    if total <= 0.0 {
        return Ok(CommandResult {
            success: true,
            data: Some(serde_json::json!({"created": false, "reason": "no positive total"})),
            error: None,
        });
    }
    if let Some(existing) = receipt.transaction_id.as_deref() {
        return Ok(CommandResult {
            success: true,
            data: Some(serde_json::json!({"created": false, "reason": "already linked", "transactionId": existing})),
            error: None,
        });
    }

    let currency = parsed
        .get("currency")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    let merchant = parsed
        .get("merchant")
        .and_then(|v| v.as_str())
        .unwrap_or("Receipt")
        .trim()
        .to_string();
    let date_ms = parse_receipt_date(
        parsed.get("date").and_then(|v| v.as_str()).unwrap_or(""),
        receipt.created_at,
    );
    let now = chrono::Utc::now().timestamp_millis();

    // Short-lived read locks only — no guard may live across the awaits.
    let (rate_used, base, already_created_id) = {
        let conn = state.db.lock().map_err(|e| format!("{e}"))?;
        let base = base_currency();
        let code = if currency.is_empty() { base.as_str() } else { currency.as_str() };
        let rate = rate_to_base(&conn, code, &base, &rate_date_of(date_ms));
        // Linking that failed on the previous attempt leaves the transaction
        // already in the ledger; re-adopt it instead of making a twin.
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM transactions WHERE receipt_id = ?1 AND deleted_at IS NULL",
                rusqlite::params![receipt.id],
                |r| r.get(0),
            )
            .ok();
        (rate, base, existing)
    };

    let transaction_id = match already_created_id {
        Some(found) => found,
        None => {
            let transaction_id = uuid::Uuid::now_v7().to_string();
            let amount_base = total * rate_used;
            let body = serde_json::json!({
                "transactions": [{
                    "id": transaction_id,
                    "type": "expense",
                    "amount": total,
                    "currencyCode": if currency.is_empty() { base.as_str() } else { currency.as_str() },
                    "amountBase": amount_base,
                    "rateUsed": rate_used,
                    "accountId": null,
                    "toAccountId": null,
                    "category": merchant,
                    "date": date_ms,
                    "note": "Parsed from receipt",
                    "receiptId": receipt.id,
                    "isDraft": true,
                    "createdAt": now,
                    "deletedAt": null,
                }],
                "deleted": [],
            });
            if let Err(e) = api::post::<_, serde_json::Value>("/transactions", &body).await {
                return Ok(CommandResult {
                    success: false,
                    data: None,
                    error: Some(e.to_message()),
                });
            }
            transaction_id
        }
    };

    // Client-wins receipt push, carrying every field it set so linking does
    // not clobber the parse result: the shape is `handlers::receipts::ReceiptPush`.
    let receipt_body = serde_json::json!({
        "receipts": [{
            "id": receipt.id,
            "localPath": receipt.local_path,
            "remotePath": receipt.remote_path,
            "uploadStatus": receipt.upload_status,
            "parsed": receipt.parsed,
            "parsedJson": receipt.parsed_json,
            "transactionId": transaction_id,
            "createdAt": receipt.created_at,
        }],
        "deleted": [],
    });
    if let Err(e) = api::post::<_, serde_json::Value>("/receipt-pictures", &receipt_body).await {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some(e.to_message()),
        });
    }

    Ok(CommandResult {
        success: true,
        data: Some(serde_json::json!({"created": true, "transactionId": transaction_id})),
        error: None,
    })
}

#[tauri::command]
async fn update_ingredient(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
    caloriesPer100g: f64,
    proteinPer100g: f64,
    carbsPer100g: f64,
    fatPer100g: f64,
) -> Result<CommandResult<serde_json::Value>, String> {
    // POST /ingredients replaces the whole row, so an edit has to start from
    // what is stored rather than send only the fields that changed.
    let Some(mut ingredient) = read_ingredient_for_push(&state, &id) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some(format!("Ingredient {id} not found")),
        });
    };
    if let Some(obj) = ingredient.as_object_mut() {
        obj.insert("name".into(), serde_json::json!(name));
        obj.insert("caloriesPer100g".into(), serde_json::json!(caloriesPer100g));
        obj.insert("proteinPer100g".into(), serde_json::json!(proteinPer100g));
        obj.insert("carbsPer100g".into(), serde_json::json!(carbsPer100g));
        obj.insert("fatPer100g".into(), serde_json::json!(fatPer100g));
    }
    Ok(push_ok("/ingredients", &ingredient, serde_json::json!({ "id": id })).await)
}

#[tauri::command]
async fn create_ingredient(
    name: String,
    caloriesPer100g: f64,
    proteinPer100g: f64,
    carbsPer100g: f64,
    fatPer100g: f64,
    sodiumPer100g: Option<f64>,
    fiberPer100g: Option<f64>,
    sugarPer100g: Option<f64>,
    brand: Option<String>,
    barcode: Option<String>,
) -> CommandResult<serde_json::Value> {
    // Option only so an older front-end that omits these still calls in cleanly;
    // an omitted field means zero.
    let id = uuid::Uuid::now_v7().to_string();
    push_ok(
        "/ingredients",
        &serde_json::json!({
            "id": id,
            "name": name,
            "caloriesPer100g": caloriesPer100g,
            "proteinPer100g": proteinPer100g,
            "carbsPer100g": carbsPer100g,
            "fatPer100g": fatPer100g,
            "sodiumPer100g": sodiumPer100g,
            "fiberPer100g": fiberPer100g,
            "sugarPer100g": sugarPer100g,
            "isArchived": false,
            "brand": brand,
            "barcode": barcode,
            "createdAt": now_ms(),
            "pictures": [],
            "prices": [],
            "deleted": [],
        }),
        serde_json::json!({ "id": id }),
    )
    .await
}

/// Reads an ingredient back in the shape `POST /ingredients` expects.
///
/// Includes the pictures and prices sub-tables, because the endpoint upserts all
/// three: sending empty lists would wipe them.
fn read_ingredient_for_push(state: &tauri::State<AppState>, id: &str) -> Option<serde_json::Value> {
    let conn = state.db.lock().ok()?;
    let base: serde_json::Value = conn.query_row(
        "SELECT id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g,
                sodium_per100g, fiber_per100g, sugar_per100g, is_archived, brand, barcode, created_at
         FROM ingredients WHERE id = ?1 AND deleted_at IS NULL",
        params![id],
        |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "name": r.get::<_, String>(1)?,
                "caloriesPer100g": r.get::<_, f64>(2)?,
                "proteinPer100g": r.get::<_, f64>(3)?,
                "carbsPer100g": r.get::<_, f64>(4)?,
                "fatPer100g": r.get::<_, f64>(5)?,
                "sodiumPer100g": r.get::<_, Option<f64>>(6)?,
                "fiberPer100g": r.get::<_, Option<f64>>(7)?,
                "sugarPer100g": r.get::<_, Option<f64>>(8)?,
                "isArchived": r.get::<_, i64>(9)? != 0,
                "brand": r.get::<_, Option<String>>(10)?,
                "barcode": r.get::<_, Option<String>>(11)?,
                "createdAt": r.get::<_, i64>(12)?,
            }))
        },
    ).ok()?;

    let mut out = base;
    let obj = out.as_object_mut()?;
    // `sub_rows` returns raw snake_case column names; every other caller in
    // this file remaps them to the camelCase shape the push endpoint expects
    // (see `read_food_for_push`, `template_payload`) but this one never did,
    // so re-pushing an ingredient that already had pictures or prices would
    // fail to deserialize server-side (`IngredientPicturePush`/
    // `IngredientPricePush` require `imagePath`/`storeId`/etc, not
    // `image_path`/`store_id`). Fixed here rather than carried forward, since
    // T02's receipt-match flow re-pushes exactly this payload.
    obj.insert(
        "pictures".into(),
        serde_json::Value::Array(
            sub_rows(
                &conn,
                "ingredient_pictures",
                "ingredient_id",
                id,
                &["id", "image_path", "sort_order", "created_at"],
                "ingredientId",
            )
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.get("id").cloned().unwrap_or(serde_json::Value::Null),
                    "ingredientId": p.get("ingredientId").cloned().unwrap_or(serde_json::Value::Null),
                    "imagePath": p.get("image_path").cloned().unwrap_or(serde_json::Value::Null),
                    "sortOrder": p.get("sort_order").cloned().unwrap_or(serde_json::Value::Null),
                    "createdAt": p.get("created_at").cloned().unwrap_or(serde_json::Value::Null),
                })
            })
            .collect(),
        ),
    );
    obj.insert(
        "prices".into(),
        serde_json::Value::Array(
            sub_rows(
                &conn,
                "ingredient_prices",
                "ingredient_id",
                id,
                &[
                    "id",
                    "store_id",
                    "price",
                    "currency_code",
                    "package_grams",
                    "recorded_at",
                    "source_url",
                ],
                "ingredientId",
            )
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.get("id").cloned().unwrap_or(serde_json::Value::Null),
                    "ingredientId": p.get("ingredientId").cloned().unwrap_or(serde_json::Value::Null),
                    "storeId": p.get("store_id").cloned().unwrap_or(serde_json::Value::Null),
                    "price": p.get("price").cloned().unwrap_or(serde_json::Value::Null),
                    "currencyCode": p.get("currency_code").cloned().unwrap_or(serde_json::Value::Null),
                    "packageGrams": p.get("package_grams").cloned().unwrap_or(serde_json::Value::Null),
                    "recordedAt": p.get("recorded_at").cloned().unwrap_or(serde_json::Value::Null),
                    "sourceUrl": p.get("source_url").cloned().unwrap_or(serde_json::Value::Null),
                })
            })
            .collect(),
        ),
    );
    Some(out)
}

/// The deployment's base currency, for the price-tracking aggregates below.
/// Defaults to EUR -- same fallback `Config::base_currency` itself uses --
/// rather than failing a read command outright when the config cannot load.
fn base_currency() -> String {
    maia::Config::shared(None)
        .map(|c| c.base_currency.clone())
        .unwrap_or_else(|_| "EUR".to_string())
}

/// Exposes the deployment's base currency to the finance views: a transaction
/// already in base currency must not be shown as `X base (rate 1)` noise, and
/// only converted rows earn that line.
#[tauri::command]
fn get_base_currency() -> CommandResult<String> {
    CommandResult {
        success: true,
        data: Some(base_currency()),
        error: None,
    }
}

/// Records a price observation for an existing ingredient (T03's "Add price",
/// parity with mobile's price sheet). Like `update_ingredient`, this starts
/// from the ingredient's current push payload and appends to it rather than
/// sending only the new row, because `POST /ingredients` upserts everything
/// it is given.
#[tauri::command]
async fn add_ingredient_price(
    state: tauri::State<'_, AppState>,
    ingredientId: String,
    storeId: String,
    price: f64,
    currencyCode: String,
    packageGrams: Option<f64>,
    recordedAt: i64,
    sourceUrl: Option<String>,
) -> Result<CommandResult<serde_json::Value>, String> {
    let Some(mut ingredient) = read_ingredient_for_push(&state, &ingredientId) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some(format!("Ingredient {ingredientId} not found")),
        });
    };
    let id = uuid::Uuid::now_v7().to_string();
    let new_price = serde_json::json!({
        "id": id,
        "ingredientId": ingredientId,
        "storeId": storeId,
        "price": price,
        "currencyCode": currencyCode,
        "packageGrams": packageGrams,
        "recordedAt": recordedAt,
        "sourceUrl": sourceUrl,
    });
    if let Some(obj) = ingredient.as_object_mut() {
        match obj.get_mut("prices").and_then(|p| p.as_array_mut()) {
            Some(prices) => prices.push(new_price),
            None => {
                obj.insert("prices".into(), serde_json::Value::Array(vec![new_price]));
            }
        }
    }
    Ok(push_ok("/ingredients", &ingredient, serde_json::json!({ "id": id })).await)
}

/// Matches one parsed receipt line item to an existing ingredient (T02),
/// recording a price observation at the receipt's own store and date. The
/// store is found-or-created by the receipt's merchant name server-side (see
/// `POST /ingredient-prices/match`); this command only forwards the inputs
/// the match dialog collects.
#[tauri::command]
async fn match_receipt_item(
    ingredientId: String,
    merchant: String,
    price: f64,
    currencyCode: String,
    packageGrams: Option<f64>,
    recordedAt: i64,
    sourceUrl: Option<String>,
) -> CommandResult<serde_json::Value> {
    json_result(
        api::post(
            "/ingredient-prices/match",
            &serde_json::json!({
                "ingredientId": ingredientId,
                "merchant": merchant,
                "price": price,
                "currencyCode": currencyCode,
                "packageGrams": packageGrams,
                "recordedAt": recordedAt,
                "sourceUrl": sourceUrl,
            }),
        )
        .await,
    )
}

/// Every ingredient's price-per-100g for the given period (one of "week",
/// "month", "year") containing `atMs`, plus the derived price-per-gram of
/// `nutrient` ("protein" | "carbs" | "fat" | "sodium" | "fiber" | "sugar").
/// Rows with no price data for the period, or no figure for the chosen
/// nutrient, are omitted rather than shown as a misleading zero. Sorted
/// cheapest-per-nutrient-gram first.
#[tauri::command]
fn nutrient_price_comparison(
    state: tauri::State<AppState>,
    period: String,
    nutrient: String,
    atMs: i64,
) -> CommandResult<Vec<serde_json::Value>> {
    let Some(period) = parse_period(&period) else {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("unknown period '{period}'")),
        };
    };
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("lock {e}")) },
    };
    let base = base_currency();
    let mut stmt = match conn.prepare(
        "SELECT id, name, protein_per100g, carbs_per100g, fat_per100g, sodium_per100g, fiber_per100g, sugar_per100g
         FROM ingredients WHERE deleted_at IS NULL AND is_archived = 0 ORDER BY name",
    ) {
        Ok(s) => s,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("prep {e}")) },
    };
    let rows: Vec<(String, String, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>)> =
        match stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
            })
            .and_then(|m| m.collect())
        {
            Ok(v) => v,
            Err(e) => return CommandResult { success: false, data: None, error: Some(format!("{e}")) },
        };
    drop(stmt);

    let mut out = Vec::new();
    for (id, name, protein, carbs, fat, sodium, fiber, sugar) in rows {
        let Ok(Some(price_per_100g)) = maia::pricing::price_per_100g_for_period(&conn, &id, &base, period, atMs) else {
            continue;
        };
        let nutrient_per_100g = match nutrient.as_str() {
            "protein" => protein,
            "carbs" => carbs,
            "fat" => fat,
            "sodium" => sodium,
            "fiber" => fiber,
            "sugar" => sugar,
            _ => None,
        };
        let Some(per_gram) = maia::pricing::price_per_nutrient_gram(price_per_100g, nutrient_per_100g) else {
            continue;
        };
        out.push(serde_json::json!({
            "ingredientId": id,
            "name": name,
            "pricePer100g": price_per_100g,
            "pricePerNutrientGram": per_gram,
            "currencyCode": base,
        }));
    }
    out.sort_by(|a, b| {
        let av = a["pricePerNutrientGram"].as_f64().unwrap_or(f64::MAX);
        let bv = b["pricePerNutrientGram"].as_f64().unwrap_or(f64::MAX);
        av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
    });
    CommandResult { success: true, data: Some(out), error: None }
}

/// A logged meal's estimated cost (see `maia::pricing::meal_cost_estimate`):
/// `null` data when nothing in it has ever been priced, otherwise the total
/// in the deployment's base currency plus whether every ingredient
/// contributed (`complete: false` means a partial, best-effort sum).
#[tauri::command]
fn meal_cost_estimate(state: tauri::State<AppState>, mealId: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("lock {e}")) },
    };
    match maia::pricing::meal_cost_estimate(&conn, &mealId, &base_currency()) {
        Ok(Some(estimate)) => CommandResult {
            success: true,
            data: Some(serde_json::json!({
                "totalBase": estimate.total_base,
                "complete": estimate.complete,
                "currencyCode": base_currency(),
            })),
            error: None,
        },
        Ok(None) => CommandResult { success: true, data: None, error: None },
        Err(e) => CommandResult { success: false, data: None, error: Some(format!("{e}")) },
    }
}

/// The day/week/month "estimated food spend" rollup: every logged meal's cost
/// estimate, bucketed and summed. `bucket` is one of "day", "week", "month".
#[tauri::command]
fn food_spend_rollup(
    state: tauri::State<AppState>,
    bucket: String,
    sinceMs: i64,
    untilMs: i64,
) -> CommandResult<Vec<serde_json::Value>> {
    let Some(bucket) = parse_period(&bucket) else {
        return CommandResult { success: false, data: None, error: Some(format!("unknown bucket '{bucket}'")) };
    };
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("lock {e}")) },
    };
    match maia::pricing::food_spend_rollup(&conn, &base_currency(), bucket, sinceMs, untilMs) {
        Ok(buckets) => CommandResult {
            success: true,
            data: Some(
                buckets
                    .into_iter()
                    .map(|b| {
                        serde_json::json!({
                            "bucketStartMs": b.bucket_start_ms,
                            "totalBase": b.total_base,
                            "complete": b.complete,
                        })
                    })
                    .collect(),
            ),
            error: None,
        },
        Err(e) => CommandResult { success: false, data: None, error: Some(format!("{e}")) },
    }
}

fn parse_period(s: &str) -> Option<maia::pricing::Period> {
    match s {
        "day" => Some(maia::pricing::Period::Day),
        "week" => Some(maia::pricing::Period::Week),
        "month" => Some(maia::pricing::Period::Month),
        "year" => Some(maia::pricing::Period::Year),
        _ => None,
    }
}

/// Rows of a child table belonging to one parent, shaped for a push payload.
///
/// `parent_column` is the local column holding the parent's id and `parent_field`
/// is the name the payload uses for it, since the two need not match. The filter
/// matters: sending the whole table would attach every other row's pictures to
/// this ingredient.
fn sub_rows(
    conn: &Connection,
    table: &str,
    parent_column: &str,
    parent_id: &str,
    columns: &[&str],
    parent_field: &str,
) -> Vec<serde_json::Value> {
    let selected = columns.join(", ");
    let sql =
        format!("SELECT {selected} FROM {table} WHERE {parent_column} = ?1 AND deleted_at IS NULL");
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    let mapped = stmt.query_map(params![parent_id], |r| {
        let mut obj = serde_json::Map::new();
        for (i, name) in columns.iter().enumerate() {
            obj.insert((*name).to_string(), column_value(r, i));
        }
        obj.insert(
            parent_field.to_string(),
            serde_json::Value::String(parent_id.to_string()),
        );
        Ok(serde_json::Value::Object(obj))
    });
    match mapped {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// A column as JSON, trying the types that actually occur in these tables.
fn column_value(row: &rusqlite::Row<'_>, index: usize) -> serde_json::Value {
    match row.get_ref(index) {
        Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
        Ok(rusqlite::types::ValueRef::Integer(i)) => serde_json::json!(i),
        Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::json!(f),
        Ok(rusqlite::types::ValueRef::Text(t)) => {
            serde_json::Value::String(String::from_utf8_lossy(t).into_owned())
        }
        // A blob has no sensible JSON form, and none of these tables use one.
        Ok(rusqlite::types::ValueRef::Blob(_)) | Err(_) => serde_json::Value::Null,
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// POSTs to the server and maps the result onto the app's command envelope.
///
/// Every write goes through here, so an unreachable server produces the same
/// clear message everywhere instead of a different failure per command.
async fn push_ok(
    path: &str,
    body: &serde_json::Value,
    data: serde_json::Value,
) -> CommandResult<serde_json::Value> {
    match api::post::<_, serde_json::Value>(path, body).await {
        Ok(_) => CommandResult {
            success: true,
            data: Some(data),
            error: None,
        },
        Err(e) => CommandResult {
            success: false,
            data: None,
            error: Some(e.to_message()),
        },
    }
}

// --- Foods (recipes) ------------------------------------------------------
//
// A food is a named collection of ingredients with amounts, and carries **no
// nutrition of its own**. Every number is derived from
// foods -> food_ingredients -> ingredients on read, so an ingredient edit is
// visible in every food built from it with nothing to recompute or invalidate.

/// The live per-100g profile of a food, derived from its composition.
///
/// `None` when the food cannot be resolved: no composition, no weight in it, or
/// a part whose ingredient row is missing. That last case matters — summing the
/// parts that happen to be present would report a confident number for a recipe
/// that is really missing an ingredient, and would drop those parts from the
/// weight denominator, inflating the rest.
fn resolve_food_nutrition(
    conn: &rusqlite::Connection,
    food_id: &str,
) -> Result<Option<serde_json::Value>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT fi.amount, i.calories_per100g, i.protein_per100g, i.carbs_per100g, \
             i.fat_per100g, i.sodium_per100g, i.fiber_per100g, i.sugar_per100g \
             FROM food_ingredients fi JOIN ingredients i ON i.id = fi.ingredient_id \
             WHERE fi.food_id = ?1 AND fi.deleted_at IS NULL",
        )
        .map_err(|e| format!("prep {e}"))?;
    let rows: Vec<(
        f64,
        f64,
        f64,
        f64,
        f64,
        Option<f64>,
        Option<f64>,
        Option<f64>,
    )> = stmt
        .query_map([food_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .map_err(|e| format!("query {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("collect {e}"))?;

    // Every part must be present. An inner join would silently shorten the list.
    let parts: i64 = conn
        .query_row(
            "SELECT count(*) FROM food_ingredients WHERE food_id = ?1 AND deleted_at IS NULL",
            [food_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("count {e}"))?;
    if rows.is_empty() || rows.len() as i64 != parts {
        return Ok(None);
    }

    // Cooking changes the batch's mass, not its calories, so the final weight
    // is the denominator. NULL (legacy recipes) means the raw sum.
    let cooked_weight: Option<f64> = conn
        .query_row(
            "SELECT cooked_weight FROM foods WHERE id = ?1",
            [food_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("cooked weight {e}"))?;

    let mut total_amount = 0.0;
    let (mut cal, mut prot, mut carb, mut fat) = (0.0, 0.0, 0.0, 0.0);
    let (mut sod, mut fib, mut sug) = (0.0, 0.0, 0.0);
    // No "did any component provide this" flags any more: the columns are
    // NOT NULL and zero-defaulted, so a component that omits one contributes a
    // real zero rather than leaving the total undefined.
    for (amount, c, p, cb, f, s, fb, sg) in rows {
        total_amount += amount;
        cal += c * amount;
        prot += p * amount;
        carb += cb * amount;
        fat += f * amount;
        sod += s.unwrap_or(0.0) * amount;
        fib += fb.unwrap_or(0.0) * amount;
        sug += sg.unwrap_or(0.0) * amount;
    }
    if total_amount <= 0.0 {
        return Ok(None);
    }

    // Each term is already an absolute amount (per-100g x amount), so dividing
    // by the denominator lands directly on a per-100g figure. A non-positive
    // cooked weight is treated as unset, matching the phone's resolver.
    let denominator = match cooked_weight {
        Some(w) if w > 0.0 => w,
        _ => total_amount,
    };
    let f = 1.0 / denominator;
    Ok(Some(serde_json::json!({
        "caloriesPer100g": cal * f,
        "proteinPer100g": prot * f,
        "carbsPer100g": carb * f,
        "fatPer100g": fat * f,
        "sodiumPer100g": sod * f,
        "fiberPer100g": fib * f,
        "sugarPer100g": sug * f,
        "totalAmount": denominator,
    })))
}

/// One food with its composition and its resolved nutrition.
#[tauri::command]
fn list_foods(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let mut stmt = match conn.prepare("SELECT id, name, created_at, archived_at, cooked_weight FROM foods WHERE deleted_at IS NULL ORDER BY name") {
        Ok(s)=>s,
        Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))},
    };
    let rows: Vec<(String, String, i64, Option<i64>, Option<f64>)> = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let mut out = Vec::new();
    for (id, name, created_at, archived_at, cooked_weight) in rows {
        // Parts come back ordered by ingredient name: there is no stored sort
        // order, because the order carries no meaning.
        let mut cs = match conn.prepare(
            "SELECT fi.ingredient_id, fi.amount, i.name FROM food_ingredients fi \
             JOIN ingredients i ON i.id = fi.ingredient_id \
             WHERE fi.food_id = ?1 AND fi.deleted_at IS NULL ORDER BY i.name",
        ) {
            Ok(s) => s,
            Err(e) => {
                return CommandResult {
                    success: false,
                    data: None,
                    error: Some(format!("prep {e}")),
                };
            }
        };
        let components: Vec<serde_json::Value> = cs
            .query_map([&id], |r| {
                Ok(serde_json::json!({
                    "ingredientId": r.get::<_,String>(0)?,
                    "amount": r.get::<_,f64>(1)?,
                    "name": r.get::<_,String>(2)?,
                }))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        let nutrition = match resolve_food_nutrition(&conn, &id) {
            Ok(n) => n,
            Err(e) => {
                return CommandResult {
                    success: false,
                    data: None,
                    error: Some(e),
                };
            }
        };
        out.push(serde_json::json!({
            "id": id, "name": name, "createdAt": created_at,
            "archivedAt": archived_at, "cookedWeight": cooked_weight,
            "components": components,
            "nutrition": nutrition,
        }));
    }
    CommandResult {
        success: true,
        data: Some(out),
        error: None,
    }
}

/// Creates a food with its whole composition.
///
/// The composition is written wholesale rather than appended to, so a corrected
/// recipe cannot leave a stale part behind -- and `POST /foods` takes the
/// incoming list as the whole recipe for the same reason.
#[tauri::command]
async fn create_food(
    state: tauri::State<'_, AppState>,
    name: String,
    components: Vec<serde_json::Value>,
    cooked_weight: Option<f64>,
) -> Result<CommandResult<serde_json::Value>, String> {
    if let Err(e) = check_food_name(&name) {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some(e),
        });
    }
    if let Err(e) = check_components(&state, &components) {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some(e),
        });
    }
    let cooked_weight = match check_cooked_weight(cooked_weight) {
        Ok(w) => w,
        Err(e) => {
            return Ok(CommandResult {
                success: false,
                data: None,
                error: Some(e),
            });
        }
    };
    let id = uuid::Uuid::now_v7().to_string();
    let payload = serde_json::json!({
        "items": [{
            "id": id,
            "name": name.trim(),
            "createdAt": now_ms(),
            "archivedAt": serde_json::Value::Null,
            "cookedWeight": cooked_weight,
            "components": components,
        }],
        "deleted": [],
    });
    Ok(push_ok("/foods", &payload, serde_json::json!({ "id": id })).await)
}

#[tauri::command]
async fn update_food(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
    components: Vec<serde_json::Value>,
    cooked_weight: Option<f64>,
) -> Result<CommandResult<serde_json::Value>, String> {
    if let Err(e) = check_food_name(&name) {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some(e),
        });
    }
    if let Err(e) = check_components(&state, &components) {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some(e),
        });
    }
    let cooked_weight = match check_cooked_weight(cooked_weight) {
        Ok(w) => w,
        Err(e) => {
            return Ok(CommandResult {
                success: false,
                data: None,
                error: Some(e),
            });
        }
    };
    // The push replaces the whole row, so the fields the form does not carry --
    // when it was created, whether it is archived -- have to be read back first.
    let Some(mut food) = read_food_for_push(&state, &id) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Food not found".into()),
        });
    };
    if let Some(obj) = food.as_object_mut() {
        obj.insert("name".into(), serde_json::json!(name.trim()));
        obj.insert("components".into(), serde_json::Value::Array(components));
        obj.insert("cookedWeight".into(), serde_json::json!(cooked_weight));
    }
    Ok(push_ok(
        "/foods",
        &serde_json::json!({ "items": [food], "deleted": [] }),
        serde_json::json!({ "id": id }),
    )
    .await)
}

#[tauri::command]
async fn archive_food(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<CommandResult<serde_json::Value>, String> {
    push_food_archived(&state, id, true).await
}

#[tauri::command]
async fn restore_food(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<CommandResult<serde_json::Value>, String> {
    push_food_archived(&state, id, false).await
}

/// Archives or restores a food by re-sending it with `archivedAt` set or null.
///
/// The server has no archive endpoint for foods; `archivedAt` rides along on the
/// ordinary push, so the rest of the row has to be read back and resent intact.
async fn push_food_archived(
    state: &tauri::State<'_, AppState>,
    id: String,
    archived: bool,
) -> Result<CommandResult<serde_json::Value>, String> {
    let Some(mut food) = read_food_for_push(state, &id) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Food not found".into()),
        });
    };
    if let Some(obj) = food.as_object_mut() {
        obj.insert(
            "archivedAt".into(),
            if archived {
                serde_json::json!(now_ms())
            } else {
                serde_json::Value::Null
            },
        );
    }
    Ok(push_ok(
        "/foods",
        &serde_json::json!({ "items": [food], "deleted": [] }),
        serde_json::json!({ "id": id }),
    )
    .await)
}

fn check_food_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Name is required".into());
    }
    Ok(())
}

/// Validates the recipe's final weight after cooking.
///
/// It is the per-100g denominator, so zero or negative would make the recipe
/// unloggable. The desktop editor defaults an empty field to the raw total, so
/// a missing value here means a caller bypassed that default.
fn check_cooked_weight(cooked_weight: Option<f64>) -> Result<f64, String> {
    match cooked_weight {
        Some(w) if w > 0.0 => Ok(w),
        _ => Err("Enter the final weight after cooking (greater than zero)".into()),
    }
}

/// Validates a composition before it is sent.
///
/// The server does none of this: `POST /foods` silently drops a part naming an
/// ingredient it has never seen, and accepts negative amounts. Validating here
/// is what turns those into a clear refusal instead of a recipe that quietly
/// loses an ingredient after it was reported as saved.
fn check_components(
    state: &tauri::State<'_, AppState>,
    components: &[serde_json::Value],
) -> Result<(), String> {
    if components.is_empty() {
        return Err("A food needs at least one ingredient".into());
    }
    let conn = state.db.lock().map_err(|e| format!("lock {e}"))?;
    let mut seen: Vec<String> = Vec::new();
    for (i, c) in components.iter().enumerate() {
        let ingredient_id = c
            .get("ingredientId")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if ingredient_id.is_empty() {
            return Err(format!("Part {} has no ingredient", i + 1));
        }
        let amount = c.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if amount <= 0.0 {
            return Err(format!("Part {} needs an amount greater than zero", i + 1));
        }
        if seen.iter().any(|s| s == ingredient_id) {
            return Err(format!("Ingredient {ingredient_id} appears twice"));
        }
        seen.push(ingredient_id.to_string());

        let known: i64 = conn
            .query_row(
                "SELECT count(*) FROM ingredients WHERE id=?1 AND is_archived=0 AND deleted_at IS NULL",
                params![ingredient_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if known == 0 {
            return Err(format!("Unknown ingredient {ingredient_id}"));
        }
    }
    Ok(())
}

/// A food in the shape `POST /foods` expects, with its composition included.
fn read_food_for_push(state: &tauri::State<'_, AppState>, id: &str) -> Option<serde_json::Value> {
    let conn = state.db.lock().ok()?;
    let mut stmt = conn
        .prepare("SELECT id, name, created_at, archived_at, cooked_weight FROM foods WHERE id=?1 AND deleted_at IS NULL")
        .ok()?;
    let food = stmt.query_row(params![id], |r| {
        Ok(serde_json::json!({
            "id": r.get::<_, String>(0)?,
            "name": r.get::<_, String>(1)?,
            "createdAt": r.get::<_, i64>(2)?,
            "archivedAt": r.get::<_, Option<i64>>(3)?,
            "cookedWeight": r.get::<_, Option<f64>>(4)?,
        }))
    });
    let food = food.ok()?;
    drop(stmt);

    let parts: Vec<serde_json::Value> = sub_rows(
        &conn,
        "food_ingredients",
        "food_id",
        id,
        &["ingredient_id", "amount"],
        "food_id",
    );
    let mut food = food;
    if let Some(obj) = food.as_object_mut() {
        // `food_id` is not part of a component payload; the list is scoped by it.
        obj.insert("components".into(), serde_json::Value::Array(
            parts.into_iter().map(|p| {
                serde_json::json!({
                    "ingredientId": p.get("ingredient_id").cloned().unwrap_or(serde_json::Value::Null),
                    "amount": p.get("amount").cloned().unwrap_or(serde_json::Value::Null),
                })
            }).collect(),
        ));
    }
    Some(food)
}

/// Reads a whole template tree in the shape `POST /templates` expects.
///
/// The endpoint upserts all three tables from a single payload, so every
/// template edit has to start from the current state of the tree rather than
/// sending only the changed row -- otherwise the parts left out of the payload
/// would be recreated as-is and nothing would ever be removable.
fn template_payload(
    state: &tauri::State<'_, AppState>,
    template_id: &str,
) -> Option<serde_json::Value> {
    let conn = state.db.lock().ok()?;

    let mut stmt = conn
        .prepare(
            "SELECT id, name, notes, start_date, recurrence, excluded_dates, source_routine_id, created_at, updated_at
             FROM workout_templates WHERE id=?1 AND deleted_at IS NULL",
        )
        .ok()?;
    let template = stmt.query_row(params![template_id], |r| {
        Ok(serde_json::json!({
            "id": r.get::<_, String>(0)?,
            "name": r.get::<_, String>(1)?,
            "notes": r.get::<_, Option<String>>(2)?,
            "startDate": r.get::<_, i64>(3)?,
            "recurrence": r.get::<_, Option<String>>(4)?,
            "excludedDates": r.get::<_, Option<String>>(5)?,
            "sourceRoutineId": r.get::<_, Option<String>>(6)?,
            "createdAt": r.get::<_, i64>(7)?,
            "updatedAt": r.get::<_, i64>(8)?,
        }))
    });
    let template = template.ok()?;
    drop(stmt);

    let exercises: Vec<serde_json::Value> = sub_rows(
        &conn,
        "workout_template_exercises",
        "template_id",
        template_id,
        &["id", "exercise_id", "sort_order", "notes"],
        "template_id",
    )
    .into_iter()
    .map(|r| {
        serde_json::json!({
            "id": r.get("id").cloned().unwrap_or(serde_json::Value::Null),
            "templateId": r.get("template_id").cloned().unwrap_or(serde_json::Value::Null),
            "exerciseId": r.get("exercise_id").cloned().unwrap_or(serde_json::Value::Null),
            "sortOrder": r.get("sort_order").cloned().unwrap_or(serde_json::Value::Null),
            "notes": r.get("notes").cloned().unwrap_or(serde_json::Value::Null),
        })
    })
    .collect();

    // Sets are addressed by their exercise, so every exercise in the tree has
    // to be walked, not just the one being edited.
    let mut sets: Vec<serde_json::Value> = Vec::new();
    for ex in &exercises {
        let Some(ex_id) = ex.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        for s in sub_rows(
            &conn,
            "workout_template_sets",
            "template_exercise_id",
            ex_id,
            &[
                "id",
                "set_number",
                "reps",
                "weight_kg",
                "rest_seconds",
                "duration_minutes",
                "distance_meters",
            ],
            "template_exercise_id",
        ) {
            sets.push(serde_json::json!({
                "id": s.get("id").cloned().unwrap_or(serde_json::Value::Null),
                "templateExerciseId": s.get("template_exercise_id").cloned().unwrap_or(serde_json::Value::Null),
                "setNumber": s.get("set_number").cloned().unwrap_or(serde_json::Value::Null),
                "reps": s.get("reps").cloned().unwrap_or(serde_json::Value::Null),
                "weightKg": s.get("weight_kg").cloned().unwrap_or(serde_json::Value::Null),
                "restSeconds": s.get("rest_seconds").cloned().unwrap_or(serde_json::Value::Null),
                "durationMinutes": s.get("duration_minutes").cloned().unwrap_or(serde_json::Value::Null),
                "distanceMeters": s.get("distance_meters").cloned().unwrap_or(serde_json::Value::Null),
            }));
        }
    }

    Some(serde_json::json!({
        "workoutTemplates": [template],
        "workoutTemplateExercises": exercises,
        "workoutTemplateSets": sets,
        "deleted": [],
    }))
}

#[tauri::command]
async fn create_template(id: String, name: String) -> CommandResult<serde_json::Value> {
    let now = now_ms();
    push_ok("/templates", &serde_json::json!({
        "workoutTemplates": [{
            "id": id, "name": name, "notes": serde_json::Value::Null,
            "startDate": now, "recurrence": serde_json::Value::Null,
            "excludedDates": serde_json::Value::Null, "sourceRoutineId": serde_json::Value::Null,
            "createdAt": now, "updatedAt": now,
        }],
        "workoutTemplateExercises": [],
        "workoutTemplateSets": [],
        "deleted": [],
    }), serde_json::json!({ "id": id }))
    .await
}

#[tauri::command]
async fn update_template(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
    notes: Option<String>,
    recurrence: Option<String>,
) -> Result<CommandResult<serde_json::Value>, String> {
    let Some(mut payload) = template_payload(&state, &id) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Template not found".into()),
        });
    };
    if let Some(list) = payload
        .get_mut("workoutTemplates")
        .and_then(|v| v.as_array_mut())
    {
        if let Some(first) = list.first_mut().and_then(|v| v.as_object_mut()) {
            first.insert("name".into(), serde_json::json!(name));
            first.insert("notes".into(), serde_json::json!(notes));
            first.insert("recurrence".into(), serde_json::json!(recurrence));
        }
    }
    Ok(push_ok("/templates", &payload, serde_json::json!({ "id": id })).await)
}

/// Deletes a template by tombstoning it.
///
/// The server has no DELETE; a tombstone is what every pull filters on, and the
/// exercises and sets have to be named too or they would be orphaned pointing at
/// a template nothing can see.
#[tauri::command]
async fn delete_template(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<CommandResult<serde_json::Value>, String> {
    let Some(mut payload) = template_payload(&state, &id) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Template not found".into()),
        });
    };
    let mut deleted: Vec<String> = vec![id.clone()];
    if let Some(list) = payload
        .get("workoutTemplateSets")
        .and_then(|v| v.as_array())
    {
        deleted.extend(
            list.iter()
                .filter_map(|s| s.get("id").and_then(|v| v.as_str()).map(String::from)),
        );
    }
    if let Some(list) = payload
        .get("workoutTemplateExercises")
        .and_then(|v| v.as_array())
    {
        deleted.extend(
            list.iter()
                .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(String::from)),
        );
    }
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("deleted".into(), serde_json::json!(deleted));
    }
    Ok(push_ok("/templates", &payload, serde_json::json!({ "id": id })).await)
}

#[tauri::command]
async fn add_template_exercise(
    state: tauri::State<'_, AppState>,
    templateId: String,
    exerciseId: String,
) -> Result<CommandResult<serde_json::Value>, String> {
    let Some(mut payload) = template_payload(&state, &templateId) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Template not found".into()),
        });
    };
    let next_sort = payload
        .get("workoutTemplateExercises")
        .and_then(|v| v.as_array())
        .and_then(|list| {
            list.iter()
                .filter_map(|e| e.get("sortOrder").and_then(|v| v.as_i64()))
                .max()
        })
        .map(|m| m + 1)
        .unwrap_or(0);
    let id = uuid::Uuid::now_v7().to_string();
    if let Some(list) = payload
        .get_mut("workoutTemplateExercises")
        .and_then(|v| v.as_array_mut())
    {
        list.push(serde_json::json!({
            "id": id, "templateId": templateId, "exerciseId": exerciseId,
            "sortOrder": next_sort, "notes": serde_json::Value::Null,
        }));
    }
    Ok(push_ok("/templates", &payload, serde_json::json!({ "id": id })).await)
}

#[tauri::command]
async fn update_template_exercise(
    state: tauri::State<'_, AppState>,
    id: String,
    notes: Option<String>,
) -> Result<CommandResult<serde_json::Value>, String> {
    let Some((template_id, payload)) = find_template_of_exercise(&state, &id) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Template exercise not found".into()),
        });
    };
    let mut payload = payload;
    if let Some(list) = payload
        .get_mut("workoutTemplateExercises")
        .and_then(|v| v.as_array_mut())
    {
        for e in list.iter_mut() {
            if e.get("id").and_then(|v| v.as_str()) == Some(id.as_str()) {
                e.as_object_mut()
                    .map(|o| o.insert("notes".into(), serde_json::json!(notes)));
            }
        }
    }
    Ok(push_ok(
        "/templates",
        &payload,
        serde_json::json!({ "id": id, "templateId": template_id }),
    )
    .await)
}

#[tauri::command]
async fn delete_template_exercise(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<CommandResult<serde_json::Value>, String> {
    let Some((template_id, mut payload)) = find_template_of_exercise(&state, &id) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Template exercise not found".into()),
        });
    };
    // Its sets go too, or they would be left pointing at a tombstoned exercise.
    let mut deleted = vec![id.clone()];
    if let Some(list) = payload
        .get("workoutTemplateSets")
        .and_then(|v| v.as_array())
    {
        deleted.extend(
            list.iter()
                .filter(|s| {
                    s.get("templateExerciseId").and_then(|v| v.as_str()) == Some(id.as_str())
                })
                .filter_map(|s| s.get("id").and_then(|v| v.as_str()).map(String::from)),
        );
    }
    if let Some(list) = payload
        .get_mut("workoutTemplateExercises")
        .and_then(|v| v.as_array_mut())
    {
        list.retain(|e| e.get("id").and_then(|v| v.as_str()) != Some(id.as_str()));
    }
    if let Some(list) = payload
        .get_mut("workoutTemplateSets")
        .and_then(|v| v.as_array_mut())
    {
        list.retain(|s| s.get("templateExerciseId").and_then(|v| v.as_str()) != Some(id.as_str()));
    }
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("deleted".into(), serde_json::json!(deleted));
    }
    Ok(push_ok(
        "/templates",
        &payload,
        serde_json::json!({ "id": id, "templateId": template_id }),
    )
    .await)
}

/// The template an exercise belongs to, with that template's whole payload.
fn find_template_of_exercise(
    state: &tauri::State<'_, AppState>,
    exercise_id: &str,
) -> Option<(String, serde_json::Value)> {
    let conn = state.db.lock().ok()?;
    let template_id: String = conn
        .query_row(
            "SELECT template_id FROM workout_template_exercises WHERE id=?1 AND deleted_at IS NULL",
            params![exercise_id],
            |r| r.get(0),
        )
        .ok()?;
    drop(conn);
    template_payload(state, &template_id).map(|p| (template_id, p))
}

/// Reads an exercise image or video as a data URL.
///
/// The webview cannot be relied on to fetch `GET /media/*` from the server, so
/// the bytes come through the bridge as base64 and become a `data:` URL. A data
/// URL also avoids the `asset:` conversion a local file path would need.
#[tauri::command]
fn exercise_media(
    state: tauri::State<AppState>,
    id: String,
    video: bool,
) -> CommandResult<serde_json::Value> {
    use base64::Engine as _;

    // The id comes from the database but still reaches the filesystem, so it
    // gets the same check the server applies to its own media routes.
    if id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return CommandResult {
            success: false,
            data: None,
            error: Some("Invalid media id".into()),
        };
    }

    let dir = sync_config().media_dir();
    let (file, content_type) = if video {
        (dir.join(format!("{id}.mp4")), "video/mp4")
    } else {
        (dir.join(format!("{id}.jpg")), "image/jpeg")
    };
    let _ = &state;

    match std::fs::read(&file) {
        Ok(bytes) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            CommandResult {
                success: true,
                data: Some(serde_json::json!({
                    "contentType": content_type,
                    "dataUrl": format!("data:{content_type};base64,{encoded}"),
                })),
                error: None,
            }
        }
        // Absent media is normal -- most exercises have no image -- so the
        // caller gets a null rather than an error it has to special-case.
        Err(_) => CommandResult {
            success: true,
            data: Some(serde_json::Value::Null),
            error: None,
        },
    }
}
/// Reads a whole template with its exercises and sets, for the detail pane.

#[tauri::command]
fn get_template(state: tauri::State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("lock {e}")),
            };
        }
    };
    let tpl: serde_json::Value = match conn.query_row("SELECT id, name, notes, start_date, recurrence, created_at, updated_at FROM workout_templates WHERE id=?1", rusqlite::params![id], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"notes":r.get::<_,Option<String>>(2)?,"startDate":r.get::<_,i64>(3)?,"recurrence":r.get::<_,Option<String>>(4)?,"createdAt":r.get::<_,i64>(5)?,"updatedAt":r.get::<_,i64>(6)?}))) { Ok(v)=>v, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("template not found: {e}"))} };
    let mut ex_stmt = match conn.prepare("SELECT te.id, te.exercise_id, e.name, te.sort_order, te.notes FROM workout_template_exercises te JOIN exercises e ON e.id = te.exercise_id WHERE te.template_id = ?1 AND te.deleted_at IS NULL ORDER BY te.sort_order") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("prep {e}"))} };
    let ex_rows: Vec<(String, String, String, i64, Option<String>)> = ex_stmt
        .query_map(rusqlite::params![id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let mut exercises_out = Vec::new();
    for (ex_id, exercise_id, exercise_name, sort_order, notes) in ex_rows {
        let sets: Vec<serde_json::Value> = conn.prepare("SELECT id, set_number, reps, weight_kg, rest_seconds, duration_minutes, distance_meters FROM workout_template_sets WHERE template_exercise_id = ?1 AND deleted_at IS NULL ORDER BY set_number").unwrap().query_map([&ex_id], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"setNumber":r.get::<_,i64>(1)?,"reps":r.get::<_,Option<i64>>(2)?,"weightKg":r.get::<_,Option<f64>>(3)?,"restSeconds":r.get::<_,Option<i64>>(4)?,"durationMinutes":r.get::<_,Option<i64>>(5)?,"distanceMeters":r.get::<_,Option<f64>>(6)?}))).unwrap().filter_map(|r|r.ok()).collect();
        exercises_out.push(serde_json::json!({"id":ex_id,"exerciseId":exercise_id,"exerciseName":exercise_name,"sortOrder":sort_order,"notes":notes,"sets":sets}));
    }
    CommandResult {
        success: true,
        data: Some(serde_json::json!({"template":tpl,"exercises":exercises_out})),
        error: None,
    }
}

/// The template a set belongs to, with that template's whole payload.
fn find_template_of_set(
    state: &tauri::State<'_, AppState>,
    set_id: &str,
) -> Option<(String, serde_json::Value)> {
    let conn = state.db.lock().ok()?;
    let exercise_id: String = conn
        .query_row(
            "SELECT template_exercise_id FROM workout_template_sets WHERE id=?1 AND deleted_at IS NULL",
            params![set_id],
            |r| r.get(0),
        )
        .ok()?;
    drop(conn);
    find_template_of_exercise(state, &exercise_id)
}

#[tauri::command]
async fn add_template_set(
    state: tauri::State<'_, AppState>,
    templateExerciseId: String,
    reps: Option<i64>,
    weightKg: Option<f64>,
    restSeconds: Option<i64>,
    durationMinutes: Option<i64>,
    distanceMeters: Option<f64>,
) -> Result<CommandResult<serde_json::Value>, String> {
    let Some((template_id, mut payload)) = find_template_of_exercise(&state, &templateExerciseId)
    else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Template exercise not found".into()),
        });
    };
    let next_number = payload
        .get("workoutTemplateSets")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter(|s| {
                    s.get("templateExerciseId").and_then(|v| v.as_str())
                        == Some(templateExerciseId.as_str())
                })
                .filter_map(|s| s.get("setNumber").and_then(|v| v.as_i64()))
                .max()
                .map(|m| m + 1)
                .unwrap_or(1)
        })
        .unwrap_or(1);
    let id = uuid::Uuid::now_v7().to_string();
    if let Some(list) = payload
        .get_mut("workoutTemplateSets")
        .and_then(|v| v.as_array_mut())
    {
        list.push(serde_json::json!({
            "id": id, "templateExerciseId": templateExerciseId, "setNumber": next_number,
            "reps": reps, "weightKg": weightKg, "restSeconds": restSeconds,
            "durationMinutes": durationMinutes, "distanceMeters": distanceMeters,
        }));
    }
    Ok(push_ok(
        "/templates",
        &payload,
        serde_json::json!({ "id": id, "templateId": template_id }),
    )
    .await)
}

#[tauri::command]
async fn update_template_set(
    state: tauri::State<'_, AppState>,
    id: String,
    reps: Option<i64>,
    weightKg: Option<f64>,
    restSeconds: Option<i64>,
    durationMinutes: Option<i64>,
    distanceMeters: Option<f64>,
) -> Result<CommandResult<serde_json::Value>, String> {
    let Some((template_id, mut payload)) = find_template_of_set(&state, &id) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Set not found".into()),
        });
    };
    if let Some(list) = payload
        .get_mut("workoutTemplateSets")
        .and_then(|v| v.as_array_mut())
    {
        for s in list.iter_mut() {
            if s.get("id").and_then(|v| v.as_str()) == Some(id.as_str()) {
                if let Some(obj) = s.as_object_mut() {
                    obj.insert("reps".into(), serde_json::json!(reps));
                    obj.insert("weightKg".into(), serde_json::json!(weightKg));
                    obj.insert("restSeconds".into(), serde_json::json!(restSeconds));
                    obj.insert("durationMinutes".into(), serde_json::json!(durationMinutes));
                    obj.insert("distanceMeters".into(), serde_json::json!(distanceMeters));
                }
            }
        }
    }
    Ok(push_ok(
        "/templates",
        &payload,
        serde_json::json!({ "id": id, "templateId": template_id }),
    )
    .await)
}

#[tauri::command]
async fn delete_template_set(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<CommandResult<serde_json::Value>, String> {
    let Some((template_id, mut payload)) = find_template_of_set(&state, &id) else {
        return Ok(CommandResult {
            success: false,
            data: None,
            error: Some("Set not found".into()),
        });
    };
    if let Some(list) = payload
        .get_mut("workoutTemplateSets")
        .and_then(|v| v.as_array_mut())
    {
        list.retain(|s| s.get("id").and_then(|v| v.as_str()) != Some(id.as_str()));
    }
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("deleted".into(), serde_json::json!([id]));
    }
    Ok(push_ok(
        "/templates",
        &payload,
        serde_json::json!({ "id": id, "templateId": template_id }),
    )
    .await)
}
// ---- Config commands (from T04) ----

#[derive(Serialize)]
struct ConfigResponse {
    success: bool,
    data: Option<String>,
    error: Option<String>,
}

/// Keys the deployment config owns, which the Settings form may not write.
///
/// These decide where the database is, which port the API answers on, and which
/// key it requires. A GUI editing them would desynchronise the app from the
/// service, which reads the same file.
const DEPLOYMENT_OWNED: &[&str] = &[
    "database",
    "bind_addr",
    "media_dir",
    "seed_dir",
    "notes_dir",
    "api_key",
    "model_api",
    "model_name",
];

/// The per-user overlay, or an empty object when absent.
fn read_overlay() -> serde_json::Map<String, serde_json::Value> {
    let path = maia::config::overlay_config_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

/// The settings view: deployment values, with the overlay filling in the desktop
/// paths the form is allowed to change.
///
/// Deployment keys are inserted first so they win, and the overlay's copy of
/// them is dropped rather than shown as if editing it would do anything.
#[tauri::command]
fn read_config() -> ConfigResponse {
    let cfg = sync_config();
    let mut merged = serde_json::Map::new();
    merged.insert(
        "database".into(),
        serde_json::json!(cfg.db_path.display().to_string()),
    );
    merged.insert("model_api".into(), serde_json::json!(cfg.model_api_key));
    merged.insert("model_name".into(), serde_json::json!(cfg.model_name));
    for (key, value) in read_overlay() {
        if DEPLOYMENT_OWNED.contains(&key.as_str()) {
            continue;
        }
        merged.entry(key).or_insert(value);
    }
    ConfigResponse {
        success: true,
        data: Some(serde_json::Value::Object(merged).to_string()),
        error: None,
    }
}

/// Checks an incoming settings object against [`DEPLOYMENT_OWNED`].
///
/// Returns the names it refused rather than filtering them out, so the form can
/// be told why a save did nothing instead of appearing to have worked.
fn reject_deployment_keys(
    incoming: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), Vec<&str>> {
    let rejected: Vec<&str> = incoming
        .keys()
        .filter(|key| DEPLOYMENT_OWNED.contains(&key.as_str()))
        .map(String::as_str)
        .collect();
    if rejected.is_empty() {
        Ok(())
    } else {
        Err(rejected)
    }
}

/// Merges [incoming] into [base] instead of replacing it.
///
/// The form only knows the keys it renders, but the same overlay holds the
/// Trading 212 feed settings written by the statement endpoints. Replacing the
/// file wholesale would silently erase them on the next Settings save, so only
/// the keys the form actually sent are overwritten. Objects merge recursively
/// for the same reason one level down: `receipts.archived_dir` must not take
/// the rest of `receipts` with it.
fn merge_json_object(
    mut base: serde_json::Map<String, serde_json::Value>,
    incoming: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    for (key, value) in incoming {
        let merged = match (base.get(&key), &value) {
            (Some(serde_json::Value::Object(existing)), serde_json::Value::Object(new)) => {
                serde_json::Value::Object(merge_json_object(existing.clone(), new.clone()))
            }
            _ => value,
        };
        base.insert(key, merged);
    }
    base
}

/// Writes the desktop paths to the per-user overlay.
///
/// Refuses deployment keys outright rather than silently dropping them: a form
/// that quietly discards what the user typed is worse than one that explains
/// why it cannot.
#[tauri::command]
fn save_config(json: String) -> ConfigResponse {
    let incoming: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(e) => {
            return ConfigResponse {
                success: false,
                data: None,
                error: Some(format!("Invalid JSON: {e}")),
            };
        }
    };
    let serde_json::Value::Object(incoming) = incoming else {
        return ConfigResponse {
            success: false,
            data: None,
            error: Some("Configuration must be a JSON object".to_string()),
        };
    };

    if let Err(rejected) = reject_deployment_keys(&incoming) {
        return ConfigResponse {
            success: false,
            data: None,
            error: Some(format!(
                "Managed by the deployment config, not editable here: {}",
                rejected.join(", ")
            )),
        };
    }

    let path = maia::config::overlay_config_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return ConfigResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to create {}: {e}", parent.display())),
            };
        }
    }
    let merged = merge_json_object(read_overlay(), incoming);
    if let Err(e) = fs::write(&path, serde_json::Value::Object(merged).to_string()) {
        return ConfigResponse {
            success: false,
            data: None,
            error: Some(format!("Failed to write {}: {e}", path.display())),
        };
    }
    // The overlay can hold provider credentials written by the server, so keep
    // it owner-only even though this path only writes desktop settings.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&path, fs::Permissions::from_mode(0o600)) {
            return ConfigResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to secure {}: {e}", path.display())),
            };
        }
    }
    ConfigResponse {
        success: true,
        data: Some(path.display().to_string()),
        error: None,
    }
}

// ---- App entry point ----

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_path = get_database_path();
    let conn = init_database(&db_path).expect("Failed to initialize database");
    let app_state = AppState {
        db: Mutex::new(conn),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            read_config,
            save_config,
            get_sync_port,
            get_sync_url,
            get_sync_api_key,
            get_lan_ip,
            get_sync_lan_url,
            get_server_diagnostics,
            sync_info,
            list_notes,
            read_note,
            list_receipts,
            read_receipt,
            read_receipt_picture,
            archive_receipt,
            restore_receipt,
            list_tasks,
            list_tasks_range,
            list_templates,
            list_workouts,
            get_workout,
            list_goals,
            get_counts,
            list_urls,
            create_url,
            delete_url,
            mark_url_read,
            list_exercises,
            list_ingredients,
            list_stores,
            list_meals,
            list_body_metrics,
            list_tags,
            list_accounts,
            list_transactions,
            get_base_currency,
            upload_receipt_files,
            create_receipt_transaction,
            save_account,
            list_fx_rates,
            exercise_progression,
            exercise_volume,
            nutrition_calories,
            nutrition_macros,
            finance_category_totals,
            finance_account_balance,
            list_statements,
            get_statement,
            get_statement_file,
            import_statement,
            import_statements,
            delete_statements,
            ignore_statement,
            delete_statement,
            add_statement_files,
            scan_statements,
            list_statement_feeds,
            get_statement_feed_settings,
            save_statement_feed_settings,
            sync_statement_feed,
            create_store,
            create_fx_rate,
            list_finance_scenarios,
            create_finance_scenario,
            update_finance_scenario,
            delete_finance_scenario,
            get_finance_baseline,
            project_finance_scenario,
            update_ingredient,
            create_ingredient,
            add_ingredient_price,
            match_receipt_item,
            nutrient_price_comparison,
            meal_cost_estimate,
            food_spend_rollup,
            list_foods,
            create_food,
            update_food,
            archive_food,
            restore_food,
            create_template,
            update_template,
            delete_template,
            get_template,
            exercise_media,
            add_template_exercise,
            update_template_exercise,
            delete_template_exercise,
            add_template_set,
            update_template_set,
            delete_template_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    /// Every command the frontend invokes must be registered.
    ///
    /// The two halves are separate files that only meet through strings, so
    /// nothing else catches a rename or a removal on one side. A missing command
    /// is silent at build time and at runtime: the webview rejects with
    /// "Command <name> not found" and the panel that wanted it just stops
    /// working.
    #[test]
    fn every_invoked_command_is_registered() {
        // Both halves of the frontend invoke commands: the shell warms two
        // directly, and everything else goes through the typed API layer.
        let frontend = concat!(
            include_str!("../../src/main.ts"),
            include_str!("../../src/lib/api.ts"),
        );
        let backend = include_str!("lib.rs");

        let handler_block = backend
            .split("invoke_handler")
            .nth(1)
            .and_then(|rest| rest.split("])").next())
            .expect("generate_handler! block should be present");

        let registered: Vec<&str> = handler_block
            .lines()
            .filter_map(|l| l.trim().strip_suffix(',').map(str::trim))
            .filter(|name| {
                !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            })
            .collect();

        let mut invoked: Vec<String> = Vec::new();
        let mut rest = frontend;
        while let Some(start) = rest.find("invoke") {
            rest = &rest[start + "invoke".len()..];
            let bytes = rest.as_bytes();
            let mut idx = 0;
            while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
                idx += 1;
            }
            // Skip a generic argument, balancing nested angle brackets:
            // `invoke<Envelope<StatementDetail>>(...)` closes twice.
            if idx < bytes.len() && bytes[idx] == b'<' {
                let mut depth = 1usize;
                idx += 1;
                while idx < bytes.len() && depth > 0 {
                    match bytes[idx] {
                        b'<' => depth += 1,
                        b'>' => depth -= 1,
                        _ => {}
                    }
                    idx += 1;
                }
            }
            let candidate = rest[idx..].trim_start();
            if let Some(args) = candidate.strip_prefix('(') {
                if let Some(open) = args.find('"') {
                    let literal = &args[open + 1..];
                    if let Some(close) = literal.find('"') {
                        let name = &literal[..close];
                        if !name.is_empty()
                            && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                        {
                            invoked.push(name.to_string());
                        }
                    }
                }
            }
        }
        invoked.sort();
        invoked.dedup();

        let missing: Vec<&String> = invoked
            .iter()
            .filter(|c| !registered.contains(&c.as_str()))
            .collect();
        assert!(
            !invoked.is_empty(),
            "no commands parsed; the matcher has drifted"
        );
        assert!(
            missing.is_empty(),
            "the frontend invokes commands that are not registered: {missing:?}"
        );
    }

    /// The packaged app must load its embedded assets, not a dev server.
    ///
    /// `tauri::is_dev()` is `!cfg!(feature = "custom-protocol")`, and in dev mode
    /// the webview is pointed at build.devUrl -- http://localhost:1420 -- which
    /// nothing serves outside a `tauri dev` session. That failure is a blank
    /// window reading "Could not connect to localhost", with no error anywhere
    /// else, so it is worth a test that fails loudly instead.
    #[test]
    fn the_packaged_app_is_not_a_dev_build() {
        // The feature is declared in Cargo.toml and switched on by the Nix build;
        // this asserts the declaration survived, since a stray default-feature
        // change or a rewrite of that file is the way this silently regresses.
        let manifest = include_str!("../Cargo.toml");
        assert!(
            manifest.contains(r#"custom-protocol = ["tauri/custom-protocol"]"#),
            "the custom-protocol feature declaration is missing from Cargo.toml"
        );
        assert!(
            !manifest.contains("default = [\"custom-protocol\"]"),
            "custom-protocol must not be a default feature: `tauri dev` needs it off"
        );
    }

    /// The desktop app must not be able to write the database.
    ///
    /// That is the whole basis for routing every write through the server: it
    /// makes this process not just a convention away from writing but
    /// structurally unable to, and unable to migrate the schema even by
    /// accident. If the flags are ever loosened this fails.
    #[test]
    fn the_app_connection_cannot_write() {
        let dir = std::env::temp_dir().join(format!("maia-test-ro-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("maia.db");

        // Create the file through the server's migrator first, since the app
        // opens read-only and cannot create anything itself.
        let pool = maia::db::init_db(&path).unwrap();
        drop(pool);

        let conn = init_database(path.to_str().unwrap()).expect("read-only open should succeed");
        conn.query_row("SELECT count(*) FROM stores", [], |r| r.get::<_, i64>(0))
            .expect("reads should work");

        let write = conn.execute(
            "INSERT INTO stores (id, name, created_at, updated_at) VALUES ('s1','S',1,1)",
            [],
        );
        assert!(
            write.is_err(),
            "the app's connection must refuse writes; got {:?}",
            write.ok()
        );
    }
    use super::*;
    use serde_json::json;

    fn keys_of(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        value.as_object().unwrap().clone()
    }

    // ---- Insights: bucketing and running balance ----

    fn ms(year: i32, month: u32, day: u32) -> i64 {
        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis()
    }

    #[test]
    fn bucket_key_uses_monday_start_weeks() {
        // 2024-01-10 is a Wednesday; its week starts Monday 2024-01-08.
        assert_eq!(bucket_key(ms(2024, 1, 10), "day"), "2024-01-10");
        assert_eq!(bucket_key(ms(2024, 1, 10), "week"), "2024-01-08");
        assert_eq!(bucket_key(ms(2024, 1, 10), "month"), "2024-01-01");
        // A Monday is its own week-start.
        assert_eq!(bucket_key(ms(2024, 1, 8), "week"), "2024-01-08");
        // A Sunday belongs to the week that started the preceding Monday.
        assert_eq!(bucket_key(ms(2024, 1, 14), "week"), "2024-01-08");
    }

    /// The one piece of real accounting logic in this plan: a transfer's
    /// `account_id` is the source and `to_account_id` is the destination, and
    /// `amount_base` is always a positive magnitude, so the sign has to come
    /// from which side of the transfer this account is on. Get that backwards
    /// and every balance after the first transfer is wrong.
    #[test]
    fn running_balance_series_handles_transfers() {
        let day1 = ms(2024, 1, 8);
        let day2 = ms(2024, 1, 9);
        let rows: Vec<BalanceRow> = vec![
            // Day 1: +50 income, then -30 out via transfer to a2.
            (day1, "income".to_string(), 50.0, Some("a1".to_string()), None),
            (day1, "transfer".to_string(), 30.0, Some("a1".to_string()), Some("a2".to_string())),
            // Day 2: -20 expense, then +40 in via transfer from a3.
            (day2, "expense".to_string(), 20.0, Some("a1".to_string()), None),
            (day2, "transfer".to_string(), 40.0, Some("a3".to_string()), Some("a1".to_string())),
        ];
        let series = running_balance_series(100.0, &rows, "a1", "day");
        assert_eq!(series.len(), 2, "{series:?}");
        assert_eq!(series[0]["bucket"], "2024-01-08");
        assert_eq!(series[0]["balance"], 120.0, "100 + 50 - 30");
        assert_eq!(series[1]["bucket"], "2024-01-09");
        assert_eq!(series[1]["balance"], 140.0, "120 - 20 + 40");
    }

    #[test]
    fn running_balance_series_keeps_only_the_last_balance_per_bucket() {
        // Two changes land in the same day/week/month bucket; the chart
        // should show where the balance ended up, not every intermediate step.
        let day = ms(2024, 1, 8);
        let rows: Vec<BalanceRow> = vec![
            (day, "income".to_string(), 10.0, Some("a1".to_string()), None),
            (day, "expense".to_string(), 4.0, Some("a1".to_string()), None),
        ];
        let series = running_balance_series(0.0, &rows, "a1", "month");
        assert_eq!(series.len(), 1);
        assert_eq!(series[0]["balance"], 6.0);
    }

    #[test]
    fn compute_account_balance_matches_hand_computed_running_balance() {
        let pool = maia::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO accounts (id, name, type, opening_balance, created_at, updated_at) VALUES ('a1','Checking','bank',100,1000,1000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO accounts (id, name, type, opening_balance, created_at, updated_at) VALUES ('a2','Savings','bank',0,1000,1000)",
            [],
        ).unwrap();
        let day1 = ms(2024, 1, 8);
        let day2 = ms(2024, 1, 9);
        conn.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, account_id, to_account_id, date, created_at, updated_at) VALUES ('t1','income',50,'EUR',50,1,'a1',NULL,?1,1000,1000)",
            params![day1],
        ).unwrap();
        conn.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, account_id, to_account_id, date, created_at, updated_at) VALUES ('t2','transfer',30,'EUR',30,1,'a1','a2',?1,1000,1000)",
            params![day2],
        ).unwrap();
        conn.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, account_id, to_account_id, date, created_at, updated_at) VALUES ('t3','expense',20,'EUR',20,1,'a1',NULL,?1,1000,1000)",
            params![day2],
        ).unwrap();
        // Hand-computed: 100 opening + 50 income - 30 transferred out - 20 expense = 100.
        let series = compute_account_balance(&conn, "a1", "day").unwrap();
        assert_eq!(series.last().unwrap()["balance"], 100.0, "{series:?}");
        // The receiving side of the transfer must see its own +30.
        let series_a2 = compute_account_balance(&conn, "a2", "day").unwrap();
        assert_eq!(series_a2.last().unwrap()["balance"], 30.0, "{series_a2:?}");
    }

    #[test]
    fn compute_category_totals_matches_hand_summed_check() {
        let pool = maia::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        let day = ms(2024, 1, 8);
        conn.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, category, date, created_at, updated_at) VALUES ('t1','expense',10,'EUR',10,1,'Groceries',?1,1000,1000)",
            params![day],
        ).unwrap();
        conn.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, category, date, created_at, updated_at) VALUES ('t2','expense',5,'EUR',5,1,'Groceries',?1,1000,1000)",
            params![day],
        ).unwrap();
        conn.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, category, date, created_at, updated_at) VALUES ('t3','expense',7,'EUR',7,1,NULL,?1,1000,1000)",
            params![day],
        ).unwrap();
        // Income must not count as spending.
        conn.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, category, date, created_at, updated_at) VALUES ('t4','income',1000,'EUR',1000,1,'Salary',?1,1000,1000)",
            params![day],
        ).unwrap();
        let totals = compute_category_totals(&conn, "day").unwrap();
        let groceries = totals.iter().find(|r| r["category"] == "Groceries").unwrap();
        assert_eq!(groceries["total"], 15.0, "{totals:?}");
        let uncategorized = totals.iter().find(|r| r["category"] == "Uncategorized").unwrap();
        assert_eq!(uncategorized["total"], 7.0, "{totals:?}");
        assert!(
            totals.iter().all(|r| r["category"] != "Salary"),
            "income leaked into spending-by-category: {totals:?}"
        );
    }

    #[test]
    fn compute_exercise_progression_skips_sets_without_weight() {
        let pool = maia::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e1','Bench','weightlifting',1000,1000)", []).unwrap();
        conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e2','Plank','bodyweight',1000,1000)", []).unwrap();
        conn.execute("INSERT INTO workouts (id, name, date, created_at, updated_at) VALUES ('w1','Push',1000,1000,1000)", []).unwrap();
        conn.execute("INSERT INTO workout_exercises (id, workout_id, exercise_id, sort_order, updated_at) VALUES ('we1','w1','e1',0,1000)", []).unwrap();
        conn.execute("INSERT INTO workout_exercises (id, workout_id, exercise_id, sort_order, updated_at) VALUES ('we2','w1','e2',1,1000)", []).unwrap();
        // Planned 80kg/8, but actually did 85kg/6 -- actual must win.
        conn.execute("INSERT INTO exercise_sets (id, workout_exercise_id, set_number, reps, weight_kg, actual_reps, actual_weight_kg, updated_at) VALUES ('s1','we1',1,8,80,6,85,1000)", []).unwrap();
        // A plank set: no weight planned or done -- must be skipped, not zeroed.
        conn.execute("INSERT INTO exercise_sets (id, workout_exercise_id, set_number, updated_at) VALUES ('s2','we2',1,1000)", []).unwrap();
        let bench = compute_exercise_progression(&conn, "e1").unwrap();
        assert_eq!(bench.len(), 1);
        assert_eq!(bench[0]["weightKg"], 85.0);
        assert_eq!(bench[0]["reps"], 6);
        let plank = compute_exercise_progression(&conn, "e2").unwrap();
        assert!(plank.is_empty(), "bodyweight set should be skipped: {plank:?}");
    }

    #[test]
    fn compute_exercise_volume_rebuckets_by_month() {
        let pool = maia::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e1','Bench','weightlifting',1000,1000)", []).unwrap();
        let week1 = ms(2024, 1, 1);
        let week3 = ms(2024, 1, 15);
        conn.execute("INSERT INTO workouts (id, name, date, created_at, updated_at) VALUES ('w1','Push',?1,1000,1000)", params![week1]).unwrap();
        conn.execute("INSERT INTO workouts (id, name, date, created_at, updated_at) VALUES ('w2','Push',?1,1000,1000)", params![week3]).unwrap();
        conn.execute("INSERT INTO workout_exercises (id, workout_id, exercise_id, sort_order, updated_at) VALUES ('we1','w1','e1',0,1000)", []).unwrap();
        conn.execute("INSERT INTO workout_exercises (id, workout_id, exercise_id, sort_order, updated_at) VALUES ('we2','w2','e1',0,1000)", []).unwrap();
        // 100kg x 10 = 1000 each workout.
        conn.execute("INSERT INTO exercise_sets (id, workout_exercise_id, set_number, reps, weight_kg, updated_at) VALUES ('s1','we1',1,10,100,1000)", []).unwrap();
        conn.execute("INSERT INTO exercise_sets (id, workout_exercise_id, set_number, reps, weight_kg, updated_at) VALUES ('s2','we2',1,10,100,1000)", []).unwrap();

        let by_week = compute_exercise_volume(&conn, "week").unwrap();
        assert_eq!(by_week.len(), 2, "two different weeks: {by_week:?}");

        let by_month = compute_exercise_volume(&conn, "month").unwrap();
        assert_eq!(by_month.len(), 1, "same month should collapse to one bar: {by_month:?}");
        assert_eq!(by_month[0]["volumeKg"], 2000.0);
    }

    #[test]
    fn compute_nutrition_calories_day_bucket_is_daily_total() {
        let pool = maia::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        conn.execute("INSERT INTO foods (id, name, created_at, updated_at) VALUES ('f1','Oats',1000,1000)", []).unwrap();
        let day = ms(2024, 1, 8);
        conn.execute("INSERT INTO meals (id, name, eaten_at, created_at, updated_at) VALUES ('m1','Breakfast',?1,1000,1000)", params![day]).unwrap();
        conn.execute("INSERT INTO meals (id, name, eaten_at, created_at, updated_at) VALUES ('m2','Lunch',?1,1000,1000)", params![day]).unwrap();
        conn.execute("INSERT INTO meal_foods (id, meal_id, food_id, amount, calories, protein, carbs, fat, updated_at) VALUES ('mf1','m1','f1',100,300,10,40,8,1000)", []).unwrap();
        conn.execute("INSERT INTO meal_foods (id, meal_id, food_id, amount, calories, protein, carbs, fat, updated_at) VALUES ('mf2','m2','f1',100,500,20,60,15,1000)", []).unwrap();
        let totals = compute_nutrition_calories(&conn, "day").unwrap();
        assert_eq!(totals.len(), 1);
        // A day bucket averages over exactly that day, so it is the sum.
        assert_eq!(totals[0]["calories"], 800.0, "{totals:?}");
    }

    #[test]
    fn compute_nutrition_calories_averages_logged_days_per_bucket() {
        let pool = maia::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        conn.execute("INSERT INTO foods (id, name, created_at, updated_at) VALUES ('f1','Oats',1000,1000)", []).unwrap();
        // 2024-01-08 is a Monday (see `bucket_key_uses_monday_start_weeks`).
        let meal = |id: &str, day: u32, calories: f64| {
            conn.execute(
                "INSERT INTO meals (id, name, eaten_at, created_at, updated_at) VALUES (?1,'Meal',?2,1000,1000)",
                params![format!("meal{id}"), ms(2024, 1, day)],
            ).unwrap();
            conn.execute(
                "INSERT INTO meal_foods (id, meal_id, food_id, amount, calories, protein, carbs, fat, updated_at) VALUES (?1,?2,'f1',100,?3,0,0,0,1000)",
                params![id, format!("meal{id}"), calories],
            ).unwrap();
        };
        meal("mf1", 8, 800.0);
        meal("mf2", 9, 400.0);
        meal("mf3", 10, 200.0);

        let week = compute_nutrition_calories(&conn, "week").unwrap();
        assert_eq!(week.len(), 1, "same week: {week:?}");
        // Mean of the three logged days -- NOT sum/7 and NOT sum over the week.
        assert_eq!(week[0]["calories"], (800.0 + 400.0 + 200.0) / 3.0, "{week:?}");

        let month = compute_nutrition_calories(&conn, "month").unwrap();
        assert_eq!(month.len(), 1, "same month: {month:?}");
        assert_eq!(month[0]["calories"], (800.0 + 400.0 + 200.0) / 3.0, "{month:?}");
    }

    #[test]
    fn compute_nutrition_macros_average_per_bucket_too() {
        let pool = maia::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        conn.execute("INSERT INTO foods (id, name, created_at, updated_at) VALUES ('f1','Oats',1000,1000)", []).unwrap();
        let insert = |id: &str, day: u32, protein: f64, carbs: f64, fat: f64| {
            conn.execute(
                "INSERT INTO meals (id, name, eaten_at, created_at, updated_at) VALUES (?1,'Meal',?2,1000,1000)",
                params![format!("meal{id}"), ms(2024, 1, day)],
            ).unwrap();
            conn.execute(
                "INSERT INTO meal_foods (id, meal_id, food_id, amount, calories, protein, carbs, fat, updated_at) VALUES (?1,?2,'f1',100,0,?3,?4,?5,1000)",
                params![id, format!("meal{id}"), protein, carbs, fat],
            ).unwrap();
        };
        insert("mf1", 8, 100.0, 200.0, 50.0);
        insert("mf2", 9, 50.0, 100.0, 25.0);

        let macros = compute_nutrition_macros(&conn, "week").unwrap();
        assert_eq!(macros.len(), 1, "{macros:?}");
        assert_eq!(macros[0]["protein"], 75.0, "{macros:?}");
        assert_eq!(macros[0]["carbs"], 150.0, "{macros:?}");
        assert_eq!(macros[0]["fat"], 37.5, "{macros:?}");
    }

    #[test]
    fn desktop_paths_are_accepted() {
        let incoming = keys_of(json!({
            "receipts_path": "/home/lucas/receipts",
            "nutriments_path": "/home/lucas/nutriments",
            "receipts": { "archived_dir": "/home/lucas/archive" },
        }));
        assert_eq!(reject_deployment_keys(&incoming), Ok(()));
    }

    #[test]
    fn saving_desktop_paths_keeps_the_feed_settings() {
        // The overlay is shared with the statement endpoints, which write
        // `statements.feeds`. A Settings save must not erase what it does not
        // render, or the next save would silently disable Trading 212.
        let existing = json!({
            "receipts_path": "/old",
            "statements": { "feeds": { "trading212": { "enabled": true, "api_key": "secret" } } },
            "receipts": { "archived_dir": "/old-archive", "other": "kept" },
        })
        .as_object()
        .unwrap()
        .clone();
        let incoming = json!({
            "receipts_path": "/new",
            "receipts": { "archived_dir": "/new-archive" },
        })
        .as_object()
        .unwrap()
        .clone();

        let merged = serde_json::Value::Object(merge_json_object(existing, incoming));
        assert_eq!(merged["receipts_path"], "/new");
        assert_eq!(merged["statements"]["feeds"]["trading212"]["enabled"], true);
        assert_eq!(
            merged["statements"]["feeds"]["trading212"]["api_key"],
            "secret"
        );
        assert_eq!(merged["receipts"]["archived_dir"], "/new-archive");
        assert_eq!(merged["receipts"]["other"], "kept");
    }

    #[test]
    fn every_deployment_key_is_refused() {
        for key in DEPLOYMENT_OWNED {
            let incoming = keys_of(json!({ *key: "value" }));
            let err = reject_deployment_keys(&incoming)
                .unwrap_err_or_else(|| panic!("{key} should be refused"));
            assert_eq!(err, vec![*key], "wrong key reported for {key}");
        }
    }

    #[test]
    fn a_refusal_names_every_offending_key() {
        let incoming = keys_of(json!({
            "receipts_path": "/ok",
            "database": "/hijack.db",
            "api_key": "stolen",
        }));
        let err = reject_deployment_keys(&incoming).unwrap_err();
        assert!(err.contains(&"database"), "missing database: {err:?}");
        assert!(err.contains(&"api_key"), "missing api_key: {err:?}");
        assert!(
            !err.contains(&"receipts_path"),
            "flagged a permitted key: {err:?}"
        );
    }

    /// Unique scratch directory, removed on drop.
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "maia-drop-{tag}-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4().simple(),
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Helper so the loop above reads as an assertion rather than a panic.
    trait UnwrapErrOrElse<T, E> {
        fn unwrap_err_or_else(self, f: impl FnOnce() -> E) -> E;
    }

    impl<T, E> UnwrapErrOrElse<T, E> for Result<T, E> {
        fn unwrap_err_or_else(self, f: impl FnOnce() -> E) -> E {
            match self {
                Ok(_) => f(),
                Err(e) => e,
            }
        }
    }

    use maia::statements::SourceId;

    #[test]
    fn a_dropped_file_lands_in_the_source_folder() {
        let scratch = Scratch::new("basic");
        let placed =
            place_statement_file(&scratch.0, SourceId::CreditAgricole, "export.csv", b"rows")
                .unwrap();
        assert_eq!(placed, scratch.0.join("credit_agricole").join("export.csv"));
        assert_eq!(std::fs::read(&placed).unwrap(), b"rows");
    }

    #[test]
    fn a_taken_name_is_suffixed_not_overwritten() {
        let scratch = Scratch::new("collision");
        let first =
            place_statement_file(&scratch.0, SourceId::Trading212, "export.csv", b"one").unwrap();
        let second =
            place_statement_file(&scratch.0, SourceId::Trading212, "export.csv", b"two").unwrap();
        assert_eq!(second, scratch.0.join("trading212").join("export-2.csv"));
        assert_eq!(std::fs::read(&first).unwrap(), b"one");
        assert_eq!(std::fs::read(&second).unwrap(), b"two");

        let third =
            place_statement_file(&scratch.0, SourceId::Trading212, "export.csv", b"three").unwrap();
        assert_eq!(third, scratch.0.join("trading212").join("export-3.csv"));
    }

    #[test]
    fn a_name_carrying_directories_cannot_escape_the_dump_tree() {
        // Only the final path component is written, so a picked name carrying
        // directories collapses into the source folder instead of escaping.
        let scratch = Scratch::new("traversal");
        let outside = scratch.0.join("outside.csv");
        std::fs::write(&outside, b"bait").unwrap();

        for raw in [
            "../outside.csv",
            "..\\outside.csv",
            "/some/where/outside.csv",
        ] {
            let placed = place_statement_file(&scratch.0, SourceId::Firstrade, raw, b"payload")
                .unwrap_or_else(|_| panic!("{raw:?} should have been placed inside"));
            let source_dir = scratch.0.join("firstrade");
            assert_eq!(
                placed.parent(),
                Some(source_dir.as_path()),
                "{raw:?} must stay inside the source folder (got {placed:?})"
            );
        }
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            b"bait",
            "the bait file was untouched"
        );

        let placed = place_statement_file(
            &scratch.0,
            SourceId::Firstrade,
            "/some/where/orders.csv",
            b"orders",
        )
        .unwrap();
        assert_eq!(placed, scratch.0.join("firstrade").join("orders.csv"));
    }

    #[test]
    fn a_extensionless_or_dot_only_name_is_refused() {
        let scratch = Scratch::new("extension");
        for bad in ["no-extension", "", ".", "..", "   "] {
            let err = place_statement_file(&scratch.0, SourceId::Firstrade, bad, b"x")
                .unwrap_err_or_else(|| panic!("{bad:?} should have been refused"));
            assert!(!err.is_empty());
        }
        assert!(
            !scratch.0.join("firstrade").exists(),
            "every name was refused; nothing should have been written"
        );
    }

    #[test]
    fn a_file_the_server_parked_does_not_confuse_the_suffix_counter() {
        // The server writes `<name>.log` beside failed files; an existing log
        // must not make the next drop of the same name collide with it.
        let scratch = Scratch::new("log-beside");
        let dir = scratch.0.join("credit_agricole");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("export.csv.log"), "old failure").unwrap();

        let placed =
            place_statement_file(&scratch.0, SourceId::CreditAgricole, "export.csv", b"new")
                .unwrap();
        assert_eq!(placed, dir.join("export.csv"), "export.csv itself was free");
        assert_eq!(std::fs::read(&placed).unwrap(), b"new");
    }
}
