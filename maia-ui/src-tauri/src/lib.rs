use std::fs;
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

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT,
            path TEXT NOT NULL UNIQUE,
            metadata TEXT,
            tags TEXT DEFAULT '[]',
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_notes_title ON notes(title);

        CREATE TABLE IF NOT EXISTS receipts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            receipt_id TEXT,
            original_path TEXT NOT NULL,
            current_path TEXT NOT NULL,
            archived_path TEXT,
            status TEXT DEFAULT 'pending',
            parsed_json_path TEXT,
            merchant TEXT,
            total TEXT,
            date TEXT,
            category TEXT,
            tags TEXT,
            checksum TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            processed_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_receipts_receipt_id ON receipts(receipt_id);

        CREATE TABLE IF NOT EXISTS goals (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            description TEXT,
            status TEXT DEFAULT 'active',
            deadline TEXT,
            progress REAL DEFAULT 0.0,
            metadata TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            goal_id INTEGER REFERENCES goals(id) ON DELETE SET NULL,
            title TEXT NOT NULL,
            description TEXT,
            due_date TEXT,
            priority TEXT DEFAULT 'medium',
            tags TEXT DEFAULT '[]',
            order_index INTEGER DEFAULT 0,
            completed_at TEXT,
            metadata TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_tasks_goal ON tasks(goal_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_due ON tasks(due_date);

        CREATE TABLE IF NOT EXISTS urls (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT,
            url TEXT NOT NULL,
            source TEXT,
            tags TEXT DEFAULT '[]',
            is_new INTEGER DEFAULT 1,
            created_at TEXT DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_urls_url ON urls(url);
        CREATE INDEX IF NOT EXISTS idx_urls_is_new ON urls(is_new);",
    )
    .map_err(|e| format!("Failed to create schema: {e}"))?;

    Ok(conn)
}

fn get_database_path() -> String {
    // Try to read database path from maia.json, fall back to "maia.db"
    if let Ok(content) = fs::read_to_string("maia.json") {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(db) = config.get("database").and_then(|v| v.as_str()) {
                return db.to_string();
            }
        }
    }
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
    let db_path = get_database_path();
    let p = PathBuf::from(&db_path);
    let parent = p.parent().unwrap_or_else(|| std::path::Path::new("."));
    parent.join("fitfat_sync.db")
}

fn get_sync_api_key() -> String {
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
fn list_notes(state: tauri::State<AppState>) -> CommandResult<Vec<Note>> {
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

    let mut stmt = match conn.prepare(
        "SELECT id, title, path, tags, created_at, updated_at FROM notes ORDER BY updated_at DESC",
    ) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Query error: {e}")),
            };
        }
    };

    let notes: Vec<Note> = match stmt.query_map([], |row| {
        Ok(Note {
            id: row.get(0)?,
            title: row.get::<_, String>(1).unwrap_or_default(),
            path: row.get(2)?,
            tags: row.get::<_, String>(3).unwrap_or_else(|_| "[]".to_string()),
            created_at: row.get::<_, String>(4).unwrap_or_default(),
            updated_at: row.get::<_, String>(5).unwrap_or_default(),
        })
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
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
        data: Some(notes),
        error: None,
    }
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

#[tauri::command]
fn save_note(
    state: tauri::State<AppState>,
    id: i64,
    title: String,
    content: String,
    tags: String,
) -> CommandResult<()> {
    // First, get the current note path from the DB
    let file_path = {
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

        let path: String =
            match conn.query_row("SELECT path FROM notes WHERE id = ?1", params![id], |row| {
                row.get(0)
            }) {
                Ok(p) => p,
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

        // Update SQLite metadata
        if let Err(e) = conn.execute(
            "UPDATE notes SET title = ?1, tags = ?2, updated_at = datetime('now') WHERE id = ?3",
            params![title, tags, id],
        ) {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to update note: {e}")),
            };
        }

        path
    };

    // Write the Markdown file
    if let Err(e) = fs::write(&file_path, &content) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to write file '{}': {e}", file_path)),
        };
    }

    CommandResult {
        success: true,
        data: None,
        error: None,
    }
}

#[tauri::command]
fn create_note(state: tauri::State<AppState>, title: String) -> CommandResult<Note> {
    let notes_dir = &state.notes_dir;
    let file_path = generate_note_path(notes_dir, &title);
    let content = format!("# {}\n\n", title);

    // Write the initial Markdown file
    if let Err(e) = fs::write(&file_path, &content) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to create file: {e}")),
        };
    }

    let path_str = file_path.to_string_lossy().to_string();

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

    match conn.execute(
        "INSERT INTO notes (title, path, tags) VALUES (?1, ?2, '[]')",
        params![title, path_str],
    ) {
        Ok(_) => {}
        Err(e) => {
            // Clean up the file if DB insert fails
            let _ = fs::remove_file(&file_path);
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to insert note: {e}")),
            };
        }
    };

    let note_id = conn.last_insert_rowid();

    let note = match conn.query_row(
        "SELECT id, title, path, tags, created_at, updated_at FROM notes WHERE id = ?1",
        params![note_id],
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
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to read back note: {e}")),
            };
        }
    };

    CommandResult {
        success: true,
        data: Some(note),
        error: None,
    }
}

// ---- Receipt commands ----

#[tauri::command]
fn list_receipts(state: tauri::State<AppState>) -> CommandResult<Vec<Receipt>> {
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

    let mut stmt = match conn.prepare(
        "SELECT id, receipt_id, original_path, current_path, archived_path,
                status, parsed_json_path, merchant, total, date, category,
                tags, checksum, created_at, processed_at
         FROM receipts ORDER BY created_at DESC",
    ) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Query error: {e}")),
            };
        }
    };

    let receipts: Vec<Receipt> = match stmt.query_map([], |row| {
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
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
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
        data: Some(receipts),
        error: None,
    }
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
fn list_tasks(state: tauri::State<AppState>) -> CommandResult<Vec<Task>> {
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

    let mut stmt = match conn.prepare(
        "SELECT id, goal_id, title, description, due_date, priority, tags, order_index, completed_at, created_at, updated_at
         FROM tasks ORDER BY order_index ASC, created_at DESC",
    ) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Query error: {e}")),
            };
        }
    };

    let tasks: Vec<Task> = match stmt.query_map([], |row| {
        Ok(Task {
            id: row.get(0)?,
            goal_id: row.get(1)?,
            title: row.get(2)?,
            description: row.get(3)?,
            due_date: row.get(4)?,
            priority: row
                .get::<_, Option<String>>(5)
                .unwrap_or(Some("medium".to_string())),
            tags: row.get::<_, String>(6).unwrap_or_else(|_| "[]".to_string()),
            order_index: row.get::<_, i64>(7).unwrap_or(0),
            completed_at: row.get(8)?,
            created_at: row.get::<_, String>(9).unwrap_or_default(),
            updated_at: row.get::<_, String>(10).unwrap_or_default(),
        })
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
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
        data: Some(tasks),
        error: None,
    }
}

#[tauri::command]
fn create_task(
    state: tauri::State<AppState>,
    title: String,
    description: Option<String>,
    due_date: Option<String>,
    priority: Option<String>,
    tags: Option<String>,
    goal_id: Option<i64>,
) -> CommandResult<Task> {
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
    let priority_str = priority.unwrap_or_else(|| "medium".to_string());

    if let Err(e) = conn.execute(
        "INSERT INTO tasks (title, description, due_date, priority, tags, goal_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![title, description, due_date, priority_str, tags_str, goal_id],
    ) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to create task: {e}")),
        };
    }

    let task_id = conn.last_insert_rowid();

    let task = match conn.query_row(
        "SELECT id, goal_id, title, description, due_date, priority, tags, order_index, completed_at, created_at, updated_at
         FROM tasks WHERE id = ?1",
        params![task_id],
        |row| {
            Ok(Task {
                id: row.get(0)?,
                goal_id: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                due_date: row.get(4)?,
                priority: row.get::<_, Option<String>>(5).unwrap_or(Some("medium".to_string())),
                tags: row.get::<_, String>(6).unwrap_or_else(|_| "[]".to_string()),
                order_index: row.get::<_, i64>(7).unwrap_or(0),
                completed_at: row.get(8)?,
                created_at: row.get::<_, String>(9).unwrap_or_default(),
                updated_at: row.get::<_, String>(10).unwrap_or_default(),
            })
        },
    ) {
        Ok(t) => t,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to read back task: {e}")),
            };
        }
    };

    CommandResult {
        success: true,
        data: Some(task),
        error: None,
    }
}

#[tauri::command]
fn update_task(
    state: tauri::State<AppState>,
    id: i64,
    title: String,
    description: Option<String>,
    due_date: Option<String>,
    priority: Option<String>,
    tags: Option<String>,
    completed: bool,
    goal_id: Option<i64>,
) -> CommandResult<Task> {
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
    let priority_str = priority.unwrap_or_else(|| "medium".to_string());

    // Update fields and set completed_at based on completed toggle
    let result = if completed {
        conn.execute(
            "UPDATE tasks SET title = ?1, description = ?2, due_date = ?3, priority = ?4, tags = ?5, goal_id = ?6,
             completed_at = CASE WHEN completed_at IS NULL THEN datetime('now') ELSE completed_at END,
             updated_at = datetime('now')
             WHERE id = ?7",
            params![title, description, due_date, priority_str, tags_str, goal_id, id],
        )
    } else {
        conn.execute(
            "UPDATE tasks SET title = ?1, description = ?2, due_date = ?3, priority = ?4, tags = ?5, goal_id = ?6,
             completed_at = NULL,
             updated_at = datetime('now')
             WHERE id = ?7",
            params![title, description, due_date, priority_str, tags_str, goal_id, id],
        )
    };

    if let Err(e) = result {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to update task: {e}")),
        };
    }

    let task = match conn.query_row(
        "SELECT id, goal_id, title, description, due_date, priority, tags, order_index, completed_at, created_at, updated_at
         FROM tasks WHERE id = ?1",
        params![id],
        |row| {
            Ok(Task {
                id: row.get(0)?,
                goal_id: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                due_date: row.get(4)?,
                priority: row.get::<_, Option<String>>(5).unwrap_or(Some("medium".to_string())),
                tags: row.get::<_, String>(6).unwrap_or_else(|_| "[]".to_string()),
                order_index: row.get::<_, i64>(7).unwrap_or(0),
                completed_at: row.get(8)?,
                created_at: row.get::<_, String>(9).unwrap_or_default(),
                updated_at: row.get::<_, String>(10).unwrap_or_default(),
            })
        },
    ) {
        Ok(t) => t,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to read back task: {e}")),
            };
        }
    };

    CommandResult {
        success: true,
        data: Some(task),
        error: None,
    }
}

#[tauri::command]
fn delete_task(state: tauri::State<AppState>, id: i64) -> CommandResult<()> {
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

    if let Err(e) = conn.execute("DELETE FROM tasks WHERE id = ?1", params![id]) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to delete task: {e}")),
        };
    }

    CommandResult {
        success: true,
        data: None,
        error: None,
    }
}

// ---- Goal commands ----

#[tauri::command]
fn list_goals(state: tauri::State<AppState>) -> CommandResult<Vec<Goal>> {
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

    let mut stmt = match conn.prepare(
        "SELECT id, title, description, status, deadline, progress, created_at, updated_at
         FROM goals ORDER BY created_at DESC",
    ) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Query error: {e}")),
            };
        }
    };

    let goals: Vec<Goal> = match stmt.query_map([], |row| {
        Ok(Goal {
            id: row.get(0)?,
            title: row.get(1)?,
            description: row.get(2)?,
            status: row
                .get::<_, Option<String>>(3)
                .unwrap_or(Some("active".to_string())),
            deadline: row.get(4)?,
            progress: row.get::<_, f64>(5).unwrap_or(0.0),
            created_at: row.get::<_, String>(6).unwrap_or_default(),
            updated_at: row.get::<_, String>(7).unwrap_or_default(),
        })
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
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
        data: Some(goals),
        error: None,
    }
}

#[tauri::command]
fn create_goal(
    state: tauri::State<AppState>,
    title: String,
    description: Option<String>,
    status: Option<String>,
    deadline: Option<String>,
    progress: Option<f64>,
) -> CommandResult<Goal> {
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

    let status_str = status.unwrap_or_else(|| "active".to_string());
    let progress_val = progress.unwrap_or(0.0);

    if let Err(e) = conn.execute(
        "INSERT INTO goals (title, description, status, deadline, progress) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![title, description, status_str, deadline, progress_val],
    ) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to create goal: {e}")),
        };
    }

    let goal_id = conn.last_insert_rowid();

    let goal = match conn.query_row(
        "SELECT id, title, description, status, deadline, progress, created_at, updated_at
         FROM goals WHERE id = ?1",
        params![goal_id],
        |row| {
            Ok(Goal {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                status: row.get(3)?,
                deadline: row.get(4)?,
                progress: row.get::<_, f64>(5).unwrap_or(0.0),
                created_at: row.get::<_, String>(6).unwrap_or_default(),
                updated_at: row.get::<_, String>(7).unwrap_or_default(),
            })
        },
    ) {
        Ok(g) => g,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to read back goal: {e}")),
            };
        }
    };

    CommandResult {
        success: true,
        data: Some(goal),
        error: None,
    }
}

#[tauri::command]
fn update_goal(
    state: tauri::State<AppState>,
    id: i64,
    title: String,
    description: Option<String>,
    status: Option<String>,
    deadline: Option<String>,
    progress: Option<f64>,
) -> CommandResult<Goal> {
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

    let status_str = status.unwrap_or_else(|| "active".to_string());
    let progress_val = progress.unwrap_or(0.0);

    if let Err(e) = conn.execute(
        "UPDATE goals SET title = ?1, description = ?2, status = ?3, deadline = ?4, progress = ?5, updated_at = datetime('now') WHERE id = ?6",
        params![title, description, status_str, deadline, progress_val, id],
    ) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to update goal: {e}")),
        };
    }

    let goal = match conn.query_row(
        "SELECT id, title, description, status, deadline, progress, created_at, updated_at
         FROM goals WHERE id = ?1",
        params![id],
        |row| {
            Ok(Goal {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                status: row
                    .get::<_, Option<String>>(3)
                    .unwrap_or(Some("active".to_string())),
                deadline: row.get(4)?,
                progress: row.get::<_, f64>(5).unwrap_or(0.0),
                created_at: row.get::<_, String>(6).unwrap_or_default(),
                updated_at: row.get::<_, String>(7).unwrap_or_default(),
            })
        },
    ) {
        Ok(g) => g,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Failed to read back goal: {e}")),
            };
        }
    };

    CommandResult {
        success: true,
        data: Some(goal),
        error: None,
    }
}

#[tauri::command]
fn delete_goal(state: tauri::State<AppState>, id: i64) -> CommandResult<()> {
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

    if let Err(e) = conn.execute("DELETE FROM goals WHERE id = ?1", params![id]) {
        return CommandResult {
            success: false,
            data: None,
            error: Some(format!("Failed to delete goal: {e}")),
        };
    }

    CommandResult {
        success: true,
        data: None,
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
                error: Some(format!("Database lock error: {e}")),
            };
        }
    };

    let tasks: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
        .unwrap_or(0);
    let goals: i64 = conn
        .query_row("SELECT COUNT(*) FROM goals", [], |row| row.get(0))
        .unwrap_or(0);
    let receipts: i64 = conn
        .query_row("SELECT COUNT(*) FROM receipts", [], |row| row.get(0))
        .unwrap_or(0);
    let notes: i64 = conn
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .unwrap_or(0);
    let urls: i64 = conn
        .query_row("SELECT COUNT(*) FROM urls", [], |row| row.get(0))
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
fn list_urls(state: tauri::State<AppState>) -> CommandResult<Vec<SavedUrl>> {
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

    let mut stmt = match conn.prepare(
        "SELECT id, title, url, source, tags, is_new, created_at
         FROM urls ORDER BY is_new DESC, created_at DESC",
    ) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult {
                success: false,
                data: None,
                error: Some(format!("Query error: {e}")),
            };
        }
    };

    let urls: Vec<SavedUrl> = match stmt.query_map([], |row| {
        Ok(SavedUrl {
            id: row.get(0)?,
            title: row.get(1)?,
            url: row.get(2)?,
            source: row.get(3)?,
            tags: row.get::<_, String>(4).unwrap_or_else(|_| "[]".to_string()),
            is_new: row.get::<_, i64>(5).unwrap_or(1),
            created_at: row.get::<_, String>(6).unwrap_or_default(),
        })
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
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
        data: Some(urls),
        error: None,
    }
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
    let sync_api_key = get_sync_api_key();
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
            list_notes,
            read_note,
            save_note,
            create_note,
            list_receipts,
            read_receipt,
            archive_receipt,
            list_tasks,
            create_task,
            update_task,
            delete_task,
            list_goals,
            create_goal,
            update_goal,
            delete_goal,
            get_counts,
            list_urls,
            create_url,
            delete_url,
            mark_url_read,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
