use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sync_server::Config as SyncConfig;

/// Shared application state managed by Tauri.
struct AppState {
    db: Mutex<Connection>,
    notes_dir: PathBuf,
}

// ---- Data types ----

#[derive(Serialize, Deserialize, Clone)]
struct Note {
    id: i64,
    title: String,
    path: String,
    tags: String, // JSON array
    created_at: String,
    updated_at: String,
}

#[derive(Serialize, Deserialize)]
struct NoteWithContent {
    id: i64,
    title: String,
    path: String,
    tags: String,
    content: String,
    created_at: String,
    updated_at: String,
}

/// A receipt row, matching the `receipts` table in `sync_server::db`.
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

impl Receipt {
    /// Display status derived from the row, since there is no status column.
    fn display_status(&self) -> &'static str {
        if self.archived_at.is_some() {
            "archived"
        } else if self.parsed {
            "parsed"
        } else {
            match self.upload_status {
                1 => "uploading",
                2 | 4 => "parsing",
                3 => "error",
                _ => "local",
            }
        }
    }
}

const RECEIPT_COLUMNS: &str = "id, local_path, remote_path, upload_status, parsed, parsed_json, \
     parse_error, transaction_id, archived_path, archived_at, created_at, updated_at";

#[derive(Serialize, Deserialize, Clone)]
struct Task {
    id: i64,
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
    id: i64,
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
    id: i64,
    title: Option<String>,
    url: String,
    source: Option<String>,
    tags: String,
    is_new: i64,
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

fn init_database(db_path: &str) -> Result<Connection, String> {
    let conn = Connection::open(db_path).map_err(|e| format!("Failed to open database: {e}"))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(|e| format!("Failed to set pragmas: {e}"))?;
    sync_server::db::migrate(&conn).map_err(|e| format!("Failed to migrate sync schema: {e}"))?;
    Ok(conn)
}

/// Where the database lives, from the shared configuration.
///
/// Used to probe `maia.json` and then walk up the tree looking for `maia.db`,
/// which meant a launch from anywhere but the checkout opened a different
/// database than the service was using.
fn get_database_path() -> String {
    sync_config().db_path.display().to_string()
}

fn get_notes_dir() -> PathBuf {
    let dir = sync_config()
        .notes_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("notes"));
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir
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

/// Slugify a title for use as a filename.
fn slugify(title: &str) -> String {
    let slug: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if slug.is_empty() {
        "untitled".to_string()
    } else {
        slug
    }
}

/// Generate a unique file path for a new note.
fn generate_note_path(notes_dir: &PathBuf, title: &str) -> PathBuf {
    let base = slugify(title);
    let mut path = notes_dir.join(format!("{}.md", base));
    let mut counter = 1;
    while path.exists() {
        path = notes_dir.join(format!("{}_{}.md", base, counter));
        counter += 1;
    }
    path
}

// ---- Tauri commands ----

#[tauri::command]
fn list_notes(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let mut stmt = match conn.prepare("SELECT id, title, body, created_at, updated_at FROM notes WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"body":r.get::<_,String>(2)?,"createdAt":r.get::<_,i64>(3)?,"updatedAt":r.get::<_,i64>(4)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}

#[tauri::command]
fn read_note(state: tauri::State<AppState>, id: i64) -> CommandResult<NoteWithContent> {
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

    let note = match conn.query_row(
        "SELECT id, title, path, tags, created_at, updated_at FROM notes WHERE id = ?1",
        params![id],
        |row| {
            Ok(Note {
                id: row.get(0)?,
                title: row.get::<_, String>(1).unwrap_or_default(),
                path: row.get(2)?,
                tags: row.get::<_, String>(3).unwrap_or_else(|_| "[]".to_string()),
                created_at: row.get::<_, String>(4).unwrap_or_default(),
                updated_at: row.get::<_, String>(5).unwrap_or_default(),
            })
        },
    ) {
        Ok(n) => n,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some("Note not found".to_string()),
            };
        }
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Query error: {e}")),
            };
        }
    };

    let content = match fs::read_to_string(&note.path) {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to read file '{}': {e}", note.path)),
            };
        }
    };

    CommandResult {
        success: true,
        data: Some(NoteWithContent {
            id: note.id,
            title: note.title,
            path: note.path,
            tags: note.tags,
            content,
            created_at: note.created_at,
            updated_at: note.updated_at,
        }),
        error: None,
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
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("lock {e}")) },
    };
    let sql = format!(
        "SELECT {RECEIPT_COLUMNS} FROM receipts WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 500"
    );
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => return CommandResult { success: false, data: None, error: Some(format!("{e}")) },
    };
    let rows: Vec<Receipt> = stmt
        .query_map([], map_receipt)
        .unwrap_or_else(|e| panic!("query receipts: {e}"))
        .filter_map(|r| r.ok())
        .collect();
    CommandResult { success: true, data: Some(rows), error: None }
}

/// Loads one receipt, shared by the Tauri command and the archive/restore
/// commands that need to re-read the row after updating it.
fn load_receipt(state: &AppState, id: &str) -> Result<Receipt, String> {
    let conn = state.db.lock().map_err(|e| format!("Database lock error: {e}"))?;
    let sql = format!("SELECT {RECEIPT_COLUMNS} FROM receipts WHERE id = ?1 AND deleted_at IS NULL");
    conn.query_row(&sql, params![id], map_receipt).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => "Receipt not found".to_string(),
        other => format!("Query error: {other}"),
    })
}

#[tauri::command]
fn read_receipt(state: tauri::State<AppState>, id: String) -> CommandResult<Receipt> {
    match load_receipt(&state, &id) {
        Ok(receipt) => CommandResult { success: true, data: Some(receipt), error: None },
        Err(e) => CommandResult { success: false, data: None, error: Some(e) },
    }
}

/// Raw bytes for a receipt image, for the detail view's `<img>`.
///
/// Wraps [`load_receipt`] in the `CommandResult` envelope the commands return.
fn finish_receipt(state: &AppState, id: &str) -> CommandResult<Receipt> {
    match load_receipt(state, id) {
        Ok(r) => CommandResult { success: true, data: Some(r), error: None },
        Err(e) => CommandResult { success: false, data: None, error: Some(e) },
    }
}

/// Raw bytes for a receipt image, for the detail view.s `<img>`.: that
/// endpoint sits behind Bearer auth, which a plain image tag cannot send.
#[tauri::command]
fn read_receipt_picture(state: tauri::State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let row = {
        let conn = match state.db.lock() {
            Ok(c) => c,
            Err(e) => return CommandResult { success: false, data: None, error: Some(format!("Database lock error: {e}")) },
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
            }
        }
    };

    // Archived folders win: archiving *moved* the file.
    let base = receipt_picture_dir();
    let path = archived_path
        .or(remote_path)
        .and_then(|p| resolve_receipt_file(&base, &p));
    let Some(path) = path else {
        return CommandResult { success: false, data: None, error: Some("Receipt image not found".to_string()) };
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

/// Archive directory for receipts, from `receipts.archived_dir` in maia.json.
///
/// Read the archive directory from maia.json's `receipts.archived_dir`, or default.
fn get_archive_dir() -> PathBuf {
    if let Ok(content) = fs::read_to_string("maia.json") {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(receipts) = config.get("receipts") {
                if let Some(dir) = receipts.get("archived_dir").and_then(|v| v.as_str()) {
                    let p = PathBuf::from(dir);
                    if !p.exists() {
                        let _ = fs::create_dir_all(&p);
                    }
                    return p;
                }
            }
        }
    }
    let fallback = PathBuf::from("receipts_archive");
    if !fallback.exists() {
        let _ = fs::create_dir_all(&fallback);
    }
    fallback
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

/// The `receipt.json` sidecar written beside an archived image, so the archive
/// folder stays meaningful without the database.
fn archive_manifest(receipt: &Receipt, source: &str, archived_at: i64) -> serde_json::Value {
    // `parsed_json` is stored as a text blob; surface it as a real object when
    // it parses, and fall back to the raw string if it ever doesn't.
    let parsed = match &receipt.parsed_json {
        Some(text) => serde_json::from_str::<serde_json::Value>(text)
            .unwrap_or_else(|_| serde_json::Value::String(text.clone())),
        None => serde_json::Value::Null,
    };
    serde_json::json!({
        "id": receipt.id,
        "archivedAt": archived_at,
        "source": source,
        "uploadStatus": receipt.upload_status,
        "parsed": receipt.parsed,
        "transactionId": receipt.transaction_id,
        "parsedJson": parsed,
    })
}

/// Moves a parsed receipt's image into the archive directory, next to a
/// `receipt.json` copy of its parsed data.
///
/// `remote_path` is deliberately left untouched: it records where the image
/// lived, which is what makes [`restore_receipt`] possible.
#[tauri::command]
fn archive_receipt(state: tauri::State<AppState>, id: String) -> CommandResult<Receipt> {
    let receipt = match load_receipt(&state, &id) {
        Ok(r) => r,
        Err(e) => return CommandResult { success: false, data: None, error: Some(e) },
    };

    if receipt.archived_at.is_some() {
        return CommandResult {
            success: false,
            data: None,
            error: Some("Receipt is already archived".to_string()),
        };
    }
    if !receipt.parsed {
        return CommandResult {
            success: false,
            data: None,
            error: Some("Receipt has not been parsed yet".to_string()),
        };
    }

    let base = receipt_picture_dir();
    let Some(source_rel) = receipt.remote_path.clone() else {
        return CommandResult {
            success: false,
            data: None,
            error: Some("Receipt has no stored image to archive".to_string()),
        };
    };
    let Some(source) = resolve_receipt_file(&base, &source_rel) else {
        return CommandResult {
            success: false,
            data: None,
            error: Some("Receipt image path is invalid".to_string()),
        };
    };
    if !source.exists() {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Receipt image not found at {}", source.display())),
        };
    }

    let folder = get_archive_dir().join(&id);
    if let Err(e) = fs::create_dir_all(&folder) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Cannot create archive folder: {e}")),
        };
    }
    // One folder per receipt id, so the destination name is unambiguous.
    let ext = source
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "jpg".to_string());
    let dest = folder.join(format!("receipt.{ext}"));

    if let Err(e) = fs::rename(&source, &dest) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Cannot archive image: {e}")),
        };
    }

    let archived_at = chrono::Utc::now().timestamp_millis();
    let manifest = archive_manifest(&receipt, &source_rel, archived_at);
    if let Err(e) = fs::write(folder.join("receipt.json"), manifest.to_string()) {
        // Roll the move back so the receipt is not left without its JSON.
        let _ = fs::rename(&dest, &source);
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Cannot write archive manifest: {e}")),
        };
    }

    let archived_folder = folder.to_string_lossy().to_string();
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Database lock error: {e}")),
            }
        }
    };
    if let Err(e) = conn.execute(
        "UPDATE receipts SET archived_path = ?1, archived_at = ?2, updated_at = ?2 WHERE id = ?3",
        params![archived_folder, archived_at, id],
    ) {
        return CommandResult { success: false, data: None, error: Some(format!("{e}")) };
    }
    drop(conn);

    finish_receipt(&state, &id)
}

/// Inverse of [`archive_receipt`]: moves the image back to `remote_path` and
/// drops the sidecar. `remote_path` was never rewritten, so the original
/// location is still known.
#[tauri::command]
fn restore_receipt(state: tauri::State<AppState>, id: String) -> CommandResult<Receipt> {
    let receipt = match load_receipt(&state, &id) {
        Ok(r) => r,
        Err(e) => return CommandResult { success: false, data: None, error: Some(e) },
    };

    let Some(archived_folder) = receipt.archived_path.clone() else {
        return CommandResult {
            success: false,
            data: None,
            error: Some("Receipt is not archived".to_string()),
        };
    };
    let folder = PathBuf::from(&archived_folder);
    let image = fs::read_dir(&folder)
        .ok()
        .and_then(|entries| {
            entries.flatten().find_map(|e| {
                let p = e.path();
                let is_json = p.extension().map(|x| x == "json").unwrap_or(false);
                (!is_json && p.is_file()).then_some(p)
            })
        });
    let Some(image) = image else {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("No archived image found in {archived_folder}")),
        };
    };

    // `remote_path` is relative to the picture directory.
    let Some(source_rel) = receipt.remote_path.clone() else {
        return CommandResult {
            success: false,
            data: None,
            error: Some("Receipt has no recorded image path to restore to".to_string()),
        };
    };
    let Some(dest) = resolve_receipt_file(&receipt_picture_dir(), &source_rel) else {
        return CommandResult {
            success: false,
            data: None,
            error: Some("Receipt image path is invalid".to_string()),
        };
    };
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::rename(&image, &dest) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Cannot restore image: {e}")),
        };
    }

    // The archive folder is now redundant once the JSON sidecar is gone.
    let _ = fs::remove_file(folder.join("receipt.json"));
    let _ = fs::remove_dir(&folder);

    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Database lock error: {e}")),
            }
        }
    };
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e) = conn.execute(
        "UPDATE receipts SET archived_path = NULL, archived_at = NULL, updated_at = ?1 WHERE id = ?2",
        params![now, id],
    ) {
        return CommandResult { success: false, data: None, error: Some(format!("{e}")) };
    }
    drop(conn);

    finish_receipt(&state, &id)
}
// ---- Task commands ----

#[tauri::command]
fn list_tasks(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let mut stmt = match conn.prepare("SELECT id, date, title, done, task_status, carry_over, sort_order, due_date, notes, created_at, updated_at FROM tasks WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"date":r.get::<_,i64>(1)?,"title":r.get::<_,String>(2)?,"done":r.get::<_,i64>(3)?,"taskStatus":r.get::<_,Option<i64>>(4)?,"carryOver":r.get::<_,i64>(5)?!=0,"sortOrder":r.get::<_,i64>(6)?,"dueDate":r.get::<_,Option<i64>>(7)?,"notes":r.get::<_,Option<String>>(8)?,"createdAt":r.get::<_,i64>(9)?,"updatedAt":r.get::<_,i64>(10)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}




// ---- Goal commands ----

#[tauri::command]
fn list_goals(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let mut stmt = match conn.prepare("SELECT id, title, description, start_date, end_date, status, target_type, target_value, baseline_value, unit, reminder_enabled, reminder_time_minutes, created_at, updated_at FROM goals WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"description":r.get::<_,Option<String>>(2)?,"startDate":r.get::<_,i64>(3)?,"endDate":r.get::<_,Option<i64>>(4)?,"status":r.get::<_,String>(5)?,"targetType":r.get::<_,String>(6)?,"targetValue":r.get::<_,Option<f64>>(7)?,"baselineValue":r.get::<_,Option<f64>>(8)?,"unit":r.get::<_,Option<String>>(9)?,"reminderEnabled":r.get::<_,i64>(10)?!=0,"reminderTimeMinutes":r.get::<_,i64>(11)?,"createdAt":r.get::<_,i64>(12)?,"updatedAt":r.get::<_,i64>(13)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}




// ---- Dashboard commands ----

#[tauri::command]
fn get_counts(state: tauri::State<AppState>) -> CommandResult<DashboardCounts> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let tasks: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE deleted_at IS NULL", [], |r| r.get(0)).unwrap_or(0);
    let goals: i64 = conn.query_row("SELECT COUNT(*) FROM goals WHERE deleted_at IS NULL", [], |r| r.get(0)).unwrap_or(0);
    let receipts: i64 = conn.query_row("SELECT COUNT(*) FROM receipts WHERE deleted_at IS NULL", [], |r| r.get(0)).unwrap_or(0);
    let notes: i64 = conn.query_row("SELECT COUNT(*) FROM notes WHERE deleted_at IS NULL", [], |r| r.get(0)).unwrap_or(0);
    let urls: i64 = conn.query_row("SELECT COUNT(*) FROM urls WHERE deleted_at IS NULL", [], |r| r.get(0)).unwrap_or(0);

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
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let mut stmt = match conn.prepare("SELECT id, title, url, source, tags, is_new, created_at, updated_at FROM urls WHERE deleted_at IS NULL ORDER BY is_new DESC, created_at DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,Option<String>>(1)?,"url":r.get::<_,String>(2)?,"source":r.get::<_,Option<String>>(3)?,"tags":r.get::<_,Option<String>>(4)?,"is_new":r.get::<_,i64>(5)?,"created_at":r.get::<_,i64>(6)?,"updated_at":r.get::<_,i64>(7)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}

#[tauri::command]
fn create_url(
    state: tauri::State<AppState>,
    title: Option<String>,
    url: String,
    source: Option<String>,
    tags: Option<String>,
) -> CommandResult<SavedUrl> {
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

    let tags_str = tags.unwrap_or_else(|| "[]".to_string());

    if let Err(e) = conn.execute(
        "INSERT INTO urls (title, url, source, tags) VALUES (?1, ?2, ?3, ?4)",
        params![title, url, source, tags_str],
    ) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to create url: {e}")),
        };
    }

    let url_id = conn.last_insert_rowid();

    let saved = match conn.query_row(
        "SELECT id, title, url, source, tags, is_new, created_at
         FROM urls WHERE id = ?1",
        params![url_id],
        |row| {
            Ok(SavedUrl {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                source: row.get(3)?,
                tags: row.get::<_, String>(4).unwrap_or_else(|_| "[]".to_string()),
                is_new: row.get::<_, i64>(5).unwrap_or(1),
                created_at: row.get::<_, String>(6).unwrap_or_default(),
            })
        },
    ) {
        Ok(u) => u,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to read back url: {e}")),
            };
        }
    };

    CommandResult {
        success: true,
        data: Some(saved),
        error: None,
    }
}

#[tauri::command]
fn delete_url(state: tauri::State<AppState>, id: i64) -> CommandResult<()> {
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

    if let Err(e) = conn.execute("DELETE FROM urls WHERE id = ?1", params![id]) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to delete url: {e}")),
        };
    }

    CommandResult {
        success: true,
        data: None,
        error: None,
    }
}

#[tauri::command]
fn mark_url_read(state: tauri::State<AppState>, id: i64) -> CommandResult<SavedUrl> {
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

    if let Err(e) = conn.execute("UPDATE urls SET is_new = 0 WHERE id = ?1", params![id]) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to mark url read: {e}")),
        };
    }

    let saved = match conn.query_row(
        "SELECT id, title, url, source, tags, is_new, created_at
         FROM urls WHERE id = ?1",
        params![id],
        |row| {
            Ok(SavedUrl {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                source: row.get(3)?,
                tags: row.get::<_, String>(4).unwrap_or_else(|_| "[]".to_string()),
                is_new: row.get::<_, i64>(5).unwrap_or(0),
                created_at: row.get::<_, String>(6).unwrap_or_default(),
            })
        },
    ) {
        Ok(u) => u,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to read back url: {e}")),
            };
        }
    };

    CommandResult {
        success: true,
        data: Some(saved),
        error: None,
    }
}

#[tauri::command]
fn list_exercises(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let mut stmt = match conn.prepare("SELECT id, name, exercise_type, body_part, equipment, primary_muscle, secondary_muscle, instructions, tips, faqs, keywords, image_path, video_path, created_at, updated_at FROM exercises ORDER BY name LIMIT 5000") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("prep {e}"))}};
    let rows = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"exercise_type":r.get::<_,Option<String>>(2)?,"body_part":r.get::<_,Option<String>>(3)?,"equipment":r.get::<_,Option<String>>(4)?,"primary_muscle":r.get::<_,Option<String>>(5)?,"secondary_muscle":r.get::<_,Option<String>>(6)?,"instructions":r.get::<_,Option<String>>(7)?,"tips":r.get::<_,Option<String>>(8)?,"faqs":r.get::<_,Option<String>>(9)?,"keywords":r.get::<_,Option<String>>(10)?,"image_path":r.get::<_,Option<String>>(11)?,"video_path":r.get::<_,Option<String>>(12)?,"created_at":r.get::<_,i64>(13).unwrap_or(0),"updated_at":r.get::<_,Option<i64>>(14)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}
#[tauri::command]
fn list_ingredients(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let mut stmt = match conn.prepare("SELECT id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, sodium_per100g, fiber_per100g, sugar_per100g, is_archived, brand, barcode, created_at FROM ingredients ORDER BY name LIMIT 1000") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let mut out = Vec::new();
    let rows: Vec<(String,String,f64,f64,f64,f64,Option<f64>,Option<f64>,Option<f64>,i64,Option<String>,Option<String>,i64)> = stmt.query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?,r.get(11)?,r.get(12)?))).unwrap().filter_map(|r|r.ok()).collect();
    for (id,name,cal,prot,carb,fat,sod,fib,sug,arch,brand,barcode,created) in rows {
        let pics: Vec<serde_json::Value> = conn.prepare("SELECT id, image_path, sort_order FROM ingredient_pictures WHERE ingredient_id=?1 ORDER BY sort_order").unwrap().query_map([&id], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"imagePath":r.get::<_,String>(1)?,"sortOrder":r.get::<_,i64>(2)?}))).unwrap().filter_map(|r|r.ok()).collect();
        let prices: Vec<serde_json::Value> = conn.prepare("SELECT id, store_id, price, currency_code, recorded_at FROM ingredient_prices WHERE ingredient_id=?1 ORDER BY recorded_at DESC LIMIT 20").unwrap().query_map([&id], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"storeId":r.get::<_,String>(1)?,"price":r.get::<_,f64>(2)?,"currencyCode":r.get::<_,String>(3)?,"recordedAt":r.get::<_,i64>(4)?}))).unwrap().filter_map(|r|r.ok()).collect();
        out.push(serde_json::json!({"id":id,"name":name,"calories_per100g":cal,"protein_per100g":prot,"carbs_per100g":carb,"fat_per100g":fat,"sodium_per100g":sod,"fiber_per100g":fib,"sugar_per100g":sug,"is_archived":arch!=0,"brand":brand,"barcode":barcode,"createdAt":created,"pictures":pics,"prices":prices}));
    }
    CommandResult{success:true,data:Some(out),error:None}
}
#[tauri::command]
fn list_stores(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let mut stmt = conn.prepare("SELECT id, name, created_at FROM stores ORDER BY name").unwrap();
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"createdAt":r.get::<_,i64>(2)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}
#[tauri::command]
fn list_meals(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let mut stmt = conn.prepare("SELECT id, name, eaten_at, created_at FROM meals ORDER BY eaten_at DESC LIMIT 200").unwrap();
    let meals: Vec<(String,String,i64,i64)> = stmt.query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap().filter_map(|r|r.ok()).collect();
    let mut out=Vec::new();
    for (id,name,eaten,created) in meals {
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
    CommandResult{success:true,data:Some(out),error:None}
}
#[tauri::command]
fn list_body_metrics(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let mut stmt = conn.prepare("SELECT id, date, weight_kg, height_cm, created_at FROM body_metrics ORDER BY date DESC LIMIT 500").unwrap();
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"date":r.get::<_,i64>(1)?,"weightKg":r.get::<_,Option<f64>>(2)?,"heightCm":r.get::<_,Option<f64>>(3)?,"createdAt":r.get::<_,i64>(4)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}
#[tauri::command]
fn list_experiments(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let mut stmt = conn.prepare("SELECT id, name, purpose, start_date, end_date, status, categories, reminder_enabled, reminder_time_minutes, created_at FROM experiments ORDER BY start_date DESC LIMIT 200").unwrap();
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"purpose":r.get::<_,Option<String>>(2)?,"startDate":r.get::<_,i64>(3)?,"endDate":r.get::<_,Option<i64>>(4)?,"status":r.get::<_,String>(5)?,"categories":r.get::<_,Option<String>>(6)?,"reminderEnabled":r.get::<_,i64>(7)?,"reminderTimeMinutes":r.get::<_,i64>(8)?,"createdAt":r.get::<_,i64>(9)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}
#[tauri::command]
fn list_tags(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let mut stmt = conn.prepare("SELECT id, name, color, sort_order, created_at FROM tags ORDER BY sort_order").unwrap();
    let rows: Vec<(String,String,Option<i64>,i64)> = stmt.query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap().filter_map(|r|r.ok()).collect();
    let mut out=Vec::new();
    for (id,name,color,order) in rows {
        let taskCount: i64 = conn.query_row("SELECT count(*) FROM task_tags WHERE tag_id=?1", [&id], |r| r.get(0)).unwrap_or(0);
        let expCount: i64 = conn.query_row("SELECT count(*) FROM experiment_tags WHERE tag_id=?1", [&id], |r| r.get(0)).unwrap_or(0);
        let goalCount: i64 = conn.query_row("SELECT count(*) FROM goal_tags WHERE tag_id=?1", [&id], |r| r.get(0)).unwrap_or(0);
        let noteCount: i64 = conn.query_row("SELECT count(*) FROM note_tags WHERE tag_id=?1", [&id], |r| r.get(0)).unwrap_or(0);
        out.push(serde_json::json!({"id":id,"name":name,"color":color,"sortOrder":order,"taskCount":taskCount,"experimentCount":expCount,"goalCount":goalCount,"noteCount":noteCount}));
    }
    CommandResult{success:true,data:Some(out),error:None}
}
#[tauri::command]
fn list_accounts(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let mut stmt = conn.prepare("SELECT id, name, type, opening_balance, note, created_at FROM accounts ORDER BY name").unwrap();
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"type":r.get::<_,String>(2)?,"openingBalance":r.get::<_,f64>(3)?,"note":r.get::<_,Option<String>>(4)?,"createdAt":r.get::<_,i64>(5)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}
#[tauri::command]
fn list_transactions(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let mut stmt = conn.prepare("SELECT id, type, amount, currency_code, amount_base, rate_used, category, date, note, receipt_id, is_draft FROM transactions ORDER BY date DESC LIMIT 500").unwrap();
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"type":r.get::<_,String>(1)?,"amount":r.get::<_,f64>(2)?,"currencyCode":r.get::<_,String>(3)?,"amountBase":r.get::<_,f64>(4)?,"rateUsed":r.get::<_,f64>(5)?,"category":r.get::<_,Option<String>>(6)?,"date":r.get::<_,i64>(7)?,"note":r.get::<_,Option<String>>(8)?,"receiptId":r.get::<_,Option<String>>(9)?,"isDraft":r.get::<_,i64>(10)? !=0}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}
#[tauri::command]
fn list_fx_rates(state: tauri::State<AppState>, base: Option<String>, date: Option<String>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let b = base.unwrap_or_else(|| "USD".to_string());
    let d = date.unwrap_or_else(|| "".to_string());
    let mut sql = "SELECT code, base_code, rate_date, rate_to_base, updated_at, manual FROM fx_rates".to_string();
    let mut wh: Vec<String>=Vec::new();
    if !d.is_empty() { wh.push(format!("rate_date='{}'", d.replace('\'',"''"))); }
    if !b.is_empty() { wh.push(format!("base_code='{}'", b.replace('\'',"''"))); }
    if !wh.is_empty() { sql.push_str(" WHERE "); sql.push_str(&wh.join(" AND ")); }
    sql.push_str(" ORDER BY updated_at DESC LIMIT 200");
    let mut stmt = conn.prepare(&sql).unwrap();
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"code":r.get::<_,String>(0)?,"baseCode":r.get::<_,String>(1)?,"rateDate":r.get::<_,String>(2)?,"rateToBase":r.get::<_,f64>(3)?,"updatedAt":r.get::<_,i64>(4)?,"manual":r.get::<_,i64>(5)?!=0}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}
#[tauri::command]
fn create_store(state: tauri::State<AppState>, name: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e)=conn.execute("INSERT INTO stores (id, name, created_at, updated_at) VALUES (?1,?2,?3,?4)", rusqlite::params![id, name, now, now]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id,"name":name})),error:None}
}
#[tauri::command]
fn create_fx_rate(state: tauri::State<AppState>, code: String, baseCode: String, rateDate: String, rateToBase: f64) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e)=conn.execute("INSERT INTO fx_rates (code, base_code, rate_date, rate_to_base, updated_at, manual) VALUES (?1,?2,?3,?4,?5,1) ON CONFLICT(code, base_code, rate_date) DO UPDATE SET rate_to_base=excluded.rate_to_base, updated_at=excluded.updated_at, manual=1", rusqlite::params![code, baseCode, rateDate, rateToBase, now]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"code":code})),error:None}
}
#[tauri::command]
fn update_exercise(state: tauri::State<AppState>, id: String, name: String, bodyPart: Option<String>, equipment: Option<String>, primaryMuscle: Option<String>, secondaryMuscle: Option<String>) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e)=conn.execute("UPDATE exercises SET name=?1, body_part=?2, equipment=?3, primary_muscle=?4, secondary_muscle=?5, updated_at=?6 WHERE id=?7", rusqlite::params![name, bodyPart, equipment, primaryMuscle, secondaryMuscle, now, id]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}
#[tauri::command]
fn update_ingredient(state: tauri::State<AppState>, id: String, name: String, caloriesPer100g: f64, proteinPer100g: f64, carbsPer100g: f64, fatPer100g: f64) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e)=conn.execute("UPDATE ingredients SET name=?1, calories_per100g=?2, protein_per100g=?3, carbs_per100g=?4, fat_per100g=?5, updated_at=?6 WHERE id=?7", rusqlite::params![name, caloriesPer100g, proteinPer100g, carbsPer100g, fatPer100g, now, id]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}
#[tauri::command]
fn create_ingredient(state: tauri::State<AppState>, name: String, caloriesPer100g: f64, proteinPer100g: f64, carbsPer100g: f64, fatPer100g: f64, sodiumPer100g: Option<f64>, fiberPer100g: Option<f64>, sugarPer100g: Option<f64>, brand: Option<String>, barcode: Option<String>) -> CommandResult<serde_json::Value> {
    // Option only so an older front-end that omits these still calls in cleanly;
    // an omitted field means zero.
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e)=conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, sodium_per100g, fiber_per100g, sugar_per100g, brand, barcode, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)", rusqlite::params![id, name, caloriesPer100g, proteinPer100g, carbsPer100g, fatPer100g, sodiumPer100g.unwrap_or(0.0), fiberPer100g.unwrap_or(0.0), sugarPer100g.unwrap_or(0.0), brand, barcode, now, now]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
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
    let rows: Vec<(f64, f64, f64, f64, f64, Option<f64>, Option<f64>, Option<f64>)> = stmt
        .query_map([food_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
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
    // by the total weight lands directly on a per-100g figure.
    let f = 1.0 / total_amount;
    Ok(Some(serde_json::json!({
        "caloriesPer100g": cal * f,
        "proteinPer100g": prot * f,
        "carbsPer100g": carb * f,
        "fatPer100g": fat * f,
        "sodiumPer100g": sod * f,
        "fiberPer100g": fib * f,
        "sugarPer100g": sug * f,
        "totalAmount": total_amount,
    })))
}

/// One food with its composition and its resolved nutrition.
#[tauri::command]
fn list_foods(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let mut stmt = match conn.prepare("SELECT id, name, created_at, archived_at FROM foods WHERE deleted_at IS NULL ORDER BY name") {
        Ok(s)=>s,
        Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))},
    };
    let rows: Vec<(String, String, i64, Option<i64>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let mut out = Vec::new();
    for (id, name, created_at, archived_at) in rows {
        // Parts come back ordered by ingredient name: there is no stored sort
        // order, because the order carries no meaning.
        let mut cs = match conn.prepare(
            "SELECT fi.ingredient_id, fi.amount, i.name FROM food_ingredients fi \
             JOIN ingredients i ON i.id = fi.ingredient_id \
             WHERE fi.food_id = ?1 AND fi.deleted_at IS NULL ORDER BY i.name",
        ) { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("prep {e}"))} };
        let components: Vec<serde_json::Value> = cs
            .query_map([&id], |r| Ok(serde_json::json!({
                "ingredientId": r.get::<_,String>(0)?,
                "amount": r.get::<_,f64>(1)?,
                "name": r.get::<_,String>(2)?,
            })))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        let nutrition = match resolve_food_nutrition(&conn, &id) {
            Ok(n) => n,
            Err(e) => return CommandResult{success:false,data:None,error:Some(e)},
        };
        out.push(serde_json::json!({
            "id": id, "name": name, "createdAt": created_at,
            "archivedAt": archived_at, "components": components,
            "nutrition": nutrition,
        }));
    }
    CommandResult{success:true,data:Some(out),error:None}
}

/// Creates a food with its whole composition.
///
/// The composition is written wholesale rather than appended to, so a corrected
/// recipe cannot leave a stale part behind.
#[tauri::command]
fn create_food(
    state: tauri::State<AppState>,
    name: String,
    components: Vec<serde_json::Value>,
) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    if name.trim().is_empty() { return CommandResult{success:false,data:None,error:Some("Name is required".into())}; }
    if components.is_empty() { return CommandResult{success:false,data:None,error:Some("A food needs at least one ingredient".into())}; }
    let id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e) = conn.execute(
        "INSERT INTO foods (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        rusqlite::params![id, name.trim(), now],
    ) { return CommandResult{success:false,data:None,error:Some(format!("insert {e}"))}; }

    match write_food_components(&conn, &id, &components) {
        Ok(()) => {
            let nutrition = resolve_food_nutrition(&conn, &id).ok().flatten();
            CommandResult{success:true,data:Some(serde_json::json!({"id":id,"nutrition":nutrition})),error:None}
        }
        Err(e) => CommandResult{success:false,data:None,error:Some(e)},
    }
}

#[tauri::command]
fn update_food(
    state: tauri::State<AppState>,
    id: String,
    name: String,
    components: Vec<serde_json::Value>,
) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    if name.trim().is_empty() { return CommandResult{success:false,data:None,error:Some("Name is required".into())}; }
    if components.is_empty() { return CommandResult{success:false,data:None,error:Some("A food needs at least one ingredient".into())}; }
    let exists: i64 = conn
        .query_row("SELECT count(*) FROM foods WHERE id=?1", [&id], |r| r.get(0))
        .unwrap_or(0);
    if exists == 0 { return CommandResult{success:false,data:None,error:Some("Food not found".into())}; }

    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e) = conn.execute(
        "UPDATE foods SET name=?1, updated_at=?2 WHERE id=?3",
        rusqlite::params![name.trim(), now, id],
    ) { return CommandResult{success:false,data:None,error:Some(format!("update {e}"))}; }

    match write_food_components(&conn, &id, &components) {
        Ok(()) => {
            let nutrition = resolve_food_nutrition(&conn, &id).ok().flatten();
            CommandResult{success:true,data:Some(serde_json::json!({"nutrition":nutrition})),error:None}
        }
        Err(e) => CommandResult{success:false,data:None,error:Some(e)},
    }
}

/// Replaces a food's composition.
///
/// Part deletion travels by omission, which is why the whole list is rewritten
/// instead of diffed: there is no way to express "remove this part" otherwise.
fn write_food_components(
    conn: &rusqlite::Connection,
    food_id: &str,
    components: &[serde_json::Value],
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "DELETE FROM food_ingredients WHERE food_id=?1",
        rusqlite::params![food_id],
    )
    .map_err(|e| format!("clear {e}"))?;

    let mut seen: Vec<String> = Vec::new();
    for component in components {
        let ingredient_id = component
            .get("ingredientId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "component is missing ingredientId".to_string())?;
        let amount = component
            .get("amount")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| "component is missing amount".to_string())?;
        // The amounts are the denominator of the per-100g derivation, so a zero
        // one leaves the food unresolvable.
        if amount <= 0.0 {
            return Err("Every amount must be greater than 0".into());
        }
        if seen.iter().any(|s| s == ingredient_id) {
            return Err("The same ingredient cannot appear twice in one food".into());
        }
        seen.push(ingredient_id.to_string());
        // An unknown or archived part is rejected rather than inserted: foreign
        // keys are enforced, so it would otherwise abort the whole write.
        let known: i64 = conn
            .query_row(
                "SELECT count(*) FROM ingredients WHERE id=?1 AND is_archived=0",
                [ingredient_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if known == 0 {
            return Err("An ingredient in this food is unknown or archived".into());
        }
        conn.execute(
            "INSERT INTO food_ingredients (food_id, ingredient_id, amount, updated_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![food_id, ingredient_id, amount, now],
        )
        .map_err(|e| format!("insert {e}"))?;
    }
    Ok(())
}

/// Archives a food rather than deleting it, so meals that logged it keep
/// rendering from their own snapshot.
#[tauri::command]
fn archive_food(state: tauri::State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e) = conn.execute(
        "UPDATE foods SET archived_at=?1, updated_at=?1 WHERE id=?2",
        rusqlite::params![now, id],
    ) { return CommandResult{success:false,data:None,error:Some(format!("archive {e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}

#[tauri::command]
fn restore_food(state: tauri::State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e) = conn.execute(
        "UPDATE foods SET archived_at=NULL, updated_at=?1 WHERE id=?2",
        rusqlite::params![now, id],
    ) { return CommandResult{success:false,data:None,error:Some(format!("restore {e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}

#[tauri::command]
fn create_template(state: tauri::State<AppState>, id: String, name: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e)=conn.execute("INSERT INTO workout_templates (id, name, start_date, created_at, updated_at) VALUES (?1,?2,?3,?4,?5)", rusqlite::params![id, name, now, now, now]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}
#[tauri::command]
fn update_template(state: tauri::State<AppState>, id: String, name: String, notes: Option<String>, recurrence: Option<String>) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e)=conn.execute("UPDATE workout_templates SET name=?1, notes=?2, recurrence=?3, updated_at=?4 WHERE id=?5", rusqlite::params![name, notes, recurrence, now, id]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}
#[tauri::command]
fn delete_template(state: tauri::State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    if let Err(e)=conn.execute("DELETE FROM workout_templates WHERE id=?1", rusqlite::params![id]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}
#[tauri::command]
fn get_template(state: tauri::State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let tpl: serde_json::Value = match conn.query_row("SELECT id, name, notes, start_date, recurrence, created_at, updated_at FROM workout_templates WHERE id=?1", rusqlite::params![id], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"notes":r.get::<_,Option<String>>(2)?,"startDate":r.get::<_,i64>(3)?,"recurrence":r.get::<_,Option<String>>(4)?,"createdAt":r.get::<_,i64>(5)?,"updatedAt":r.get::<_,i64>(6)?}))) { Ok(v)=>v, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("template not found: {e}"))} };
    let mut ex_stmt = match conn.prepare("SELECT te.id, te.exercise_id, e.name, te.sort_order, te.notes FROM workout_template_exercises te JOIN exercises e ON e.id = te.exercise_id WHERE te.template_id = ?1 AND te.deleted_at IS NULL ORDER BY te.sort_order") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("prep {e}"))} };
    let ex_rows: Vec<(String,String,String,i64,Option<String>)> = ex_stmt.query_map(rusqlite::params![id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap().filter_map(|r|r.ok()).collect();
    let mut exercises_out = Vec::new();
    for (ex_id, exercise_id, exercise_name, sort_order, notes) in ex_rows {
        let sets: Vec<serde_json::Value> = conn.prepare("SELECT id, set_number, reps, weight_kg, rest_seconds, duration_minutes, distance_meters FROM workout_template_sets WHERE template_exercise_id = ?1 AND deleted_at IS NULL ORDER BY set_number").unwrap().query_map([&ex_id], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"setNumber":r.get::<_,i64>(1)?,"reps":r.get::<_,Option<i64>>(2)?,"weightKg":r.get::<_,Option<f64>>(3)?,"restSeconds":r.get::<_,Option<i64>>(4)?,"durationMinutes":r.get::<_,Option<i64>>(5)?,"distanceMeters":r.get::<_,Option<f64>>(6)?}))).unwrap().filter_map(|r|r.ok()).collect();
        exercises_out.push(serde_json::json!({"id":ex_id,"exerciseId":exercise_id,"exerciseName":exercise_name,"sortOrder":sort_order,"notes":notes,"sets":sets}));
    }
    CommandResult{success:true,data:Some(serde_json::json!({"template":tpl,"exercises":exercises_out})),error:None}
}
#[tauri::command]
fn add_template_exercise(state: tauri::State<AppState>, templateId: String, exerciseId: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::now_v7().to_string();
    let max_sort: Option<i64> = conn.query_row("SELECT MAX(sort_order) FROM workout_template_exercises WHERE template_id = ?1 AND deleted_at IS NULL", rusqlite::params![templateId], |r| r.get(0)).unwrap_or(None);
    let sort_order = max_sort.map(|m| m + 1).unwrap_or(0);
    if let Err(e)=conn.execute("INSERT INTO workout_template_exercises (id, template_id, exercise_id, sort_order, updated_at) VALUES (?1,?2,?3,?4,?5)", rusqlite::params![id, templateId, exerciseId, sort_order, now]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}
#[tauri::command]
fn update_template_exercise(state: tauri::State<AppState>, id: String, notes: Option<String>) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e)=conn.execute("UPDATE workout_template_exercises SET notes=?1, updated_at=?2 WHERE id=?3", rusqlite::params![notes, now, id]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}
#[tauri::command]
fn delete_template_exercise(state: tauri::State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    if let Err(e)=conn.execute("DELETE FROM workout_template_exercises WHERE id=?1", rusqlite::params![id]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}
#[tauri::command]
fn add_template_set(state: tauri::State<AppState>, templateExerciseId: String, reps: Option<i64>, weightKg: Option<f64>, restSeconds: Option<i64>, durationMinutes: Option<i64>, distanceMeters: Option<f64>) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::now_v7().to_string();
    let max_num: Option<i64> = conn.query_row("SELECT MAX(set_number) FROM workout_template_sets WHERE template_exercise_id = ?1 AND deleted_at IS NULL", rusqlite::params![templateExerciseId], |r| r.get(0)).unwrap_or(None);
    let set_number = max_num.map(|m| m + 1).unwrap_or(1);
    if let Err(e)=conn.execute("INSERT INTO workout_template_sets (id, template_exercise_id, set_number, reps, weight_kg, rest_seconds, duration_minutes, distance_meters, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", rusqlite::params![id, templateExerciseId, set_number, reps, weightKg, restSeconds, durationMinutes, distanceMeters, now]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}
#[tauri::command]
fn update_template_set(state: tauri::State<AppState>, id: String, reps: Option<i64>, weightKg: Option<f64>, restSeconds: Option<i64>, durationMinutes: Option<i64>, distanceMeters: Option<f64>) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e)=conn.execute("UPDATE workout_template_sets SET reps=?1, weight_kg=?2, rest_seconds=?3, duration_minutes=?4, distance_meters=?5, updated_at=?6 WHERE id=?7", rusqlite::params![reps, weightKg, restSeconds, durationMinutes, distanceMeters, now, id]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
}
#[tauri::command]
fn delete_template_set(state: tauri::State<AppState>, id: String) -> CommandResult<serde_json::Value> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    if let Err(e)=conn.execute("DELETE FROM workout_template_sets WHERE id=?1", rusqlite::params![id]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
    CommandResult{success:true,data:Some(serde_json::json!({"id":id})),error:None}
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
    let path = sync_server::config::overlay_config_path();
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
    merged.insert("database".into(), serde_json::json!(cfg.db_path.display().to_string()));
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

    let path = sync_server::config::overlay_config_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return ConfigResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to create {}: {e}", parent.display())),
            };
        }
    }
    if let Err(e) = fs::write(&path, serde_json::Value::Object(incoming).to_string()) {
        return ConfigResponse {
            success: false,
            data: None,
            error: Some(format!("Failed to write {}: {e}", path.display())),
        };
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
    let notes_dir = get_notes_dir();

    let app_state = AppState {
        db: Mutex::new(conn),
        notes_dir,
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
            list_notes,
            read_note,
            list_receipts,
            read_receipt,
            read_receipt_picture,
            archive_receipt,
            restore_receipt,
            list_tasks,
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
            list_experiments,
            list_tags,
            list_accounts,
            list_transactions,
            list_fx_rates,
            create_store,
            create_fx_rate,
            update_exercise,
            update_ingredient,
            create_ingredient,
            list_foods,
            create_food,
            update_food,
            archive_food,
            restore_food,
            create_template,
            update_template,
            delete_template,
            get_template,
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
    use super::*;
    use serde_json::json;

    fn keys_of(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn desktop_paths_are_accepted() {
        let incoming = keys_of(json!({
            "receipts_path": "/home/lucas/receipts",
            "nutriments_path": "/home/lucas/nutriments",
            "bank_statements_path": "/home/lucas/bank",
            "investments_statements_path": "/home/lucas/investments",
            "receipts": { "archived_dir": "/home/lucas/archive" },
        }));
        assert_eq!(reject_deployment_keys(&incoming), Ok(()));
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
        assert!(!err.contains(&"receipts_path"), "flagged a permitted key: {err:?}");
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
}
