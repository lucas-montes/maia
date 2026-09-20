use std::fs;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sync_server::Config as SyncConfig;
use tauri::Manager;

/// Shared application state managed by Tauri.
struct AppState {
    db: Mutex<Connection>,
    notes_dir: PathBuf,
}

struct SyncServerState {
    port: u16,
    url: String,
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

#[derive(Serialize, Deserialize, Clone)]
struct Receipt {
    id: i64,
    receipt_id: Option<String>,
    original_path: String,
    current_path: String,
    archived_path: Option<String>,
    status: Option<String>,
    parsed_json_path: Option<String>,
    merchant: Option<String>,
    total: Option<String>,
    date: Option<String>,
    category: Option<String>,
    tags: Option<String>,
    checksum: Option<String>,
    created_at: String,
    processed_at: Option<String>,
}

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
    sync_server::db::create_schema(&conn).map_err(|e| format!("Failed to create sync schema: {e}"))?;
    Ok(conn)
}

fn get_database_path() -> String {
    for candidate in ["maia.json", "../../maia.json", "../maia.json"] {
        if let Ok(content) = fs::read_to_string(candidate) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(db) = config.get("database").and_then(|v| v.as_str()) {
                    let p = PathBuf::from(db);
                    if p.is_absolute() { return p.to_string_lossy().to_string(); }
                    if let Some(parent) = PathBuf::from(candidate).parent() {
                        return parent.join(p).to_string_lossy().to_string();
                    }
                    return db.to_string();
                }
            }
        }
    }
    for candidate in ["../../maia.db", "maia.db"] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            if let Ok(canon) = p.canonicalize() { return canon.to_string_lossy().to_string(); }
            return p.to_string_lossy().to_string();
        }
    }
    let proj = PathBuf::from("../../maia.db");
    if let Ok(canon) = proj.canonicalize() { return canon.to_string_lossy().to_string(); }
    "maia.db".to_string()
}

fn get_notes_dir() -> PathBuf {
    let dir = PathBuf::from("notes");
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir
}

fn get_sync_db_path() -> PathBuf {
    PathBuf::from(get_database_path())
}

fn get_sync_api_key_value() -> String {
    if let Ok(content) = fs::read_to_string("maia.json") {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(key) = config.get("sync_api_key").and_then(|v| v.as_str()) {
                return key.to_string();
            }
            if let Some(sync) = config.get("sync") {
                if let Some(key) = sync.get("api_key").and_then(|v| v.as_str()) {
                    return key.to_string();
                }
            }
        }
    }
    "fitfat-sync-key".to_string()
}

#[tauri::command]
fn get_sync_port(state: tauri::State<Mutex<SyncServerState>>) -> u16 {
    state.lock().map(|s| s.port).unwrap_or(0)
}

#[tauri::command]
fn get_sync_url(state: tauri::State<Mutex<SyncServerState>>) -> String {
    state.lock().map(|s| s.url.clone()).unwrap_or_default()
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
fn get_sync_lan_url(state: tauri::State<Mutex<SyncServerState>>) -> String {
    let port = state.lock().map(|s| s.port).unwrap_or(3030);
    format!("http://{}:{}", get_lan_ip_value(), port)
}

#[tauri::command]
fn get_server_logs(limit: Option<usize>) -> Vec<String> {
    sync_server::logs::tail_global(limit.unwrap_or(50))
}

#[tauri::command]
fn get_server_diagnostics(state: tauri::State<Mutex<SyncServerState>>) -> serde_json::Value {
    let port = state.lock().map(|s| s.port).unwrap_or(0);
    let lan_ip = get_lan_ip_value();
    let lan_url = format!("http://{}:{}", lan_ip, port);
    let bind_addr = "0.0.0.0:3030".to_string();
    let is_listening = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok();
    let api_key = get_sync_api_key_value();
    let masked = if api_key.len() > 8 {
        format!("{}...{}", &api_key[..4], &api_key[api_key.len() - 4..])
    } else {
        "***".to_string()
    };
    serde_json::json!({
        "port": port,
        "lanUrl": lan_url,
        "bindAddr": bind_addr,
        "isListening": is_listening,
        "lanIp": lan_ip,
        "apiKeyMasked": masked,
        "hasApiKey": !api_key.is_empty()
    })
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

#[tauri::command]
fn list_receipts(state: tauri::State<AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("lock {e}"))}};
    let mut stmt = match conn.prepare("SELECT id, local_path, remote_path, upload_status, parsed, parsed_json, transaction_id, created_at, updated_at FROM receipts WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 500") { Ok(s)=>s, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let rows: Vec<serde_json::Value> = stmt.query_map([], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"localPath":r.get::<_,String>(1)?,"remotePath":r.get::<_,Option<String>>(2)?,"uploadStatus":r.get::<_,i64>(3)?,"parsed":r.get::<_,i64>(4)?!=0,"parsedJson":r.get::<_,Option<String>>(5)?,"transactionId":r.get::<_,Option<String>>(6)?,"createdAt":r.get::<_,i64>(7)?,"updatedAt":r.get::<_,i64>(8)?}))).unwrap().filter_map(|r|r.ok()).collect();
    CommandResult{success:true,data:Some(rows),error:None}
}

#[tauri::command]
fn read_receipt(state: tauri::State<AppState>, id: i64) -> CommandResult<Receipt> {
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

    let receipt = match conn.query_row(
        "SELECT id, receipt_id, original_path, current_path, archived_path,
                status, parsed_json_path, merchant, total, date, category,
                tags, checksum, created_at, processed_at
         FROM receipts WHERE id = ?1",
        params![id],
        |row| {
            Ok(Receipt {
                id: row.get(0)?,
                receipt_id: row.get(1)?,
                original_path: row.get(2)?,
                current_path: row.get(3)?,
                archived_path: row.get(4)?,
                status: row.get(5)?,
                parsed_json_path: row.get(6)?,
                merchant: row.get(7)?,
                total: row.get(8)?,
                date: row.get(9)?,
                category: row.get(10)?,
                tags: row.get(11)?,
                checksum: row.get(12)?,
                created_at: row.get::<_, String>(13).unwrap_or_default(),
                processed_at: row.get(14)?,
            })
        },
    ) {
        Ok(r) => r,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some("Receipt not found".to_string()),
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

    CommandResult {
        success: true,
        data: Some(receipt),
        error: None,
    }
}

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

#[tauri::command]
fn archive_receipt(state: tauri::State<AppState>, id: i64) -> CommandResult<Receipt> {
    // Read current receipt from DB
    let (current_path, _receipt_id) = {
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

        let result: Result<(String, Option<String>), _> = conn.query_row(
            "SELECT current_path, receipt_id FROM receipts WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );

        match result {
            Ok(r) => r,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return CommandResult {
                    success: false,
                    data: None,
                    error: Some("Receipt not found".to_string()),
                };
            }
            Err(e) => {
                return CommandResult {
                    success: false,
                    data: None,
                    error: Some(format!("Query error: {e}")),
                };
            }
        }
    };

    let source = PathBuf::from(&current_path);
    if !source.exists() {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Receipt file not found at '{current_path}'")),
        };
    }

    // Determine destination filename
    let archive_dir = get_archive_dir();
    let filename = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("receipt_{id}.jpg"));
    let dest = archive_dir.join(&filename);

    // If dest already exists, prepend receipt_id
    let dest = if dest.exists() {
        let stem = source
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "receipt".to_string());
        let ext = source
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_else(|| ".jpg".to_string());
        let dedup = format!("{}_{}{}", stem, id, ext);
        archive_dir.join(dedup)
    } else {
        dest
    };

    // Move the file
    if let Err(e) = fs::rename(&source, &dest) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to move file to '{}': {e}", dest.display())),
        };
    }

    let dest_str = dest.to_string_lossy().to_string();

    // Update the database
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

    if let Err(e) = conn.execute(
        "UPDATE receipts SET status = 'archived', archived_path = current_path, current_path = ?1, processed_at = datetime('now') WHERE id = ?2",
        params![dest_str, id],
    ) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to update receipt: {e}")),
        };
    }

    // Read back the updated receipt
    let receipt = match conn.query_row(
        "SELECT id, receipt_id, original_path, current_path, archived_path,
                status, parsed_json_path, merchant, total, date, category,
                tags, checksum, created_at, processed_at
         FROM receipts WHERE id = ?1",
        params![id],
        |row| {
            Ok(Receipt {
                id: row.get(0)?,
                receipt_id: row.get(1)?,
                original_path: row.get(2)?,
                current_path: row.get(3)?,
                archived_path: row.get(4)?,
                status: row.get(5)?,
                parsed_json_path: row.get(6)?,
                merchant: row.get(7)?,
                total: row.get(8)?,
                date: row.get(9)?,
                category: row.get(10)?,
                tags: row.get(11)?,
                checksum: row.get(12)?,
                created_at: row.get::<_, String>(13).unwrap_or_default(),
                processed_at: row.get(14)?,
            })
        },
    ) {
        Ok(r) => r,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to read back receipt: {e}")),
            };
        }
    };

    CommandResult {
        success: true,
        data: Some(receipt),
        error: None,
    }
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
        let ings: Vec<serde_json::Value> = conn.prepare("SELECT id, ingredient_id, grams FROM meal_ingredients WHERE meal_id=?1").unwrap().query_map([&id], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"ingredientId":r.get::<_,String>(1)?,"grams":r.get::<_,f64>(2)?}))).unwrap().filter_map(|r|r.ok()).collect();
        out.push(serde_json::json!({"id":id,"name":name,"eatenAt":eaten,"createdAt":created,"ingredients":ings}));
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
    let conn = match state.db.lock() { Ok(c)=>c, Err(e)=>return CommandResult{success:false,data:None,error:Some(format!("{e}"))}};
    let id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    if let Err(e)=conn.execute("INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, sodium_per100g, fiber_per100g, sugar_per100g, brand, barcode, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)", rusqlite::params![id, name, caloriesPer100g, proteinPer100g, carbsPer100g, fatPer100g, sodiumPer100g, fiberPer100g, sugarPer100g, brand, barcode, now, now]) { return CommandResult{success:false,data:None,error:Some(format!("{e}"))}; }
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

#[tauri::command]
fn read_config() -> ConfigResponse {
    let path = "maia.json";
    match fs::read_to_string(path) {
        Ok(content) => ConfigResponse {
            success: true,
            data: Some(content),
            error: None,
        },
        Err(_) => {
            // Return a default config when the file doesn't exist
            let defaults = r#"{
  "receipts_path": "",
  "nutriments_path": "",
  "bank_statements_path": "",
  "investments_statements_path": "",
  "database": "maia.db",
  "models_api": []
}"#;
            ConfigResponse {
                success: true,
                data: Some(defaults.to_string()),
                error: None,
            }
        }
    }
}

#[tauri::command]
fn save_config(json: String) -> ConfigResponse {
    // Validate that the input is valid JSON
    if let Err(e) = serde_json::from_str::<serde_json::Value>(&json) {
        return ConfigResponse {
            success: false,
            data: None,
            error: Some(format!("Invalid JSON: {e}")),
        };
    }

    let path = "maia.json";
    let backup_path = "maia.json.bak";

    // Create a backup of the current config if it exists
    if fs::metadata(path).is_ok() {
        if let Err(e) = fs::copy(path, backup_path) {
            return ConfigResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to create backup {backup_path}: {e}")),
            };
        }
    }

    // Write the new config
    match fs::write(path, &json) {
        Ok(()) => ConfigResponse {
            success: true,
            data: None,
            error: None,
        },
        Err(e) => ConfigResponse {
            success: false,
            data: None,
            error: Some(format!("Failed to write {path}: {e}")),
        },
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

    let sync_db_path = get_sync_db_path();
    let sync_api_key = get_sync_api_key_value();
    let sync_config = SyncConfig::new(sync_db_path, sync_api_key);
    let sync_state = Mutex::new(SyncServerState { port: 0, url: String::new() });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .manage(sync_state)
        .setup(move |app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match sync_server::server::start_server(sync_config).await {
                    Ok((port, _jh)) => {
                        let url = format!("http://0.0.0.0:{port}");
                        let state = handle.state::<Mutex<SyncServerState>>();
                        if let Ok(mut s) = state.lock() {
                            s.port = port;
                            s.url = url.clone();
                        }
                        eprintln!("sync-server listening on 0.0.0.0:{port}");
                    }
                    Err(e) => {
                        eprintln!("failed to start sync-server: {e}");
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_config,
            save_config,
            get_sync_port,
            get_sync_url,
            get_sync_api_key,
            get_lan_ip,
            get_sync_lan_url,
            get_server_logs,
            get_server_diagnostics,
            list_notes,
            read_note,
            list_receipts,
            read_receipt,
            archive_receipt,
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
