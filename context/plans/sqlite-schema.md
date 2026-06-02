 # Maia SQLite Schema (draft)

This file drafts the SQLite schema for the Maia desktop app (notes, tasks, goals, receipts, urls, settings).

Design notes
- All domain tables use AUTOINCREMENT integer primary keys unless a stable external ID is needed (e.g., `receipt_id`).
- Metadata fields and tag lists are stored as JSON text (SQLite `JSON1` extension can be used for queries).
- Timestamps use ISO8601 strings (`TEXT`) produced by `datetime('now')` on write.
- The application layer is responsible for updating `updated_at` on writes.

Enable foreign keys (application should run this at connection time):

```sql
PRAGMA foreign_keys = ON;
```

Schema (DDL)

-- Settings: DO NOT store runtime defaults or authoritative config in the DB.
-- Persist user-editable settings in `maia.json` (the app will read/write that file).
-- The DB should not be used as the primary source of truth for user configuration.

-- Notes: Markdown files on disk. `path` points to the file on disk; `metadata` is a JSON object stored in the DB.
CREATE TABLE IF NOT EXISTS notes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT,
  path TEXT NOT NULL UNIQUE,
  metadata TEXT, -- JSON
  tags TEXT,     -- JSON array of strings
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_notes_title ON notes(title);
CREATE INDEX IF NOT EXISTS idx_notes_tags ON notes(tags);

-- Goals: parent entities that group tasks. Progress is a numeric [0..100].
CREATE TABLE IF NOT EXISTS goals (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  description TEXT,
  status TEXT, -- e.g. active, on-hold, done
  deadline TEXT, -- ISO8601 date/time
  progress REAL DEFAULT 0.0,
  metadata TEXT, -- JSON for extensibility
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);

-- Tasks: many-to-one -> goals, with an order_index for optional ordering.
-- Use `completed_at` (nullable) instead of a boolean so we have completion timestamps.
CREATE TABLE IF NOT EXISTS tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  goal_id INTEGER REFERENCES goals(id) ON DELETE SET NULL,
  title TEXT NOT NULL,
  description TEXT,
  due_date TEXT,
  priority TEXT, -- e.g. High/Medium/Low
  tags TEXT,     -- JSON array
  order_index INTEGER DEFAULT 0,
  completed_at TEXT, -- ISO8601 timestamp when completed; NULL = not completed
  metadata TEXT, -- JSON
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_tasks_goal ON tasks(goal_id);
CREATE INDEX IF NOT EXISTS idx_tasks_due ON tasks(due_date);

-- Receipts: file-backed images. `receipt_id` is a stable external id (UUID or content hash).
CREATE TABLE IF NOT EXISTS receipts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  receipt_id TEXT UNIQUE, -- stable ID (recommended UUID)
  original_path TEXT NOT NULL,
  current_path TEXT NOT NULL,
  archived_path TEXT, -- optional
  status TEXT, -- e.g. pending, reviewed, archived
  parsed_json_path TEXT, -- optional sidecar JSON path
  merchant TEXT,
  total TEXT,
  date TEXT,
  category TEXT,
  tags TEXT, -- JSON array
  checksum TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  processed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_receipts_receipt_id ON receipts(receipt_id);
CREATE INDEX IF NOT EXISTS idx_receipts_original_path ON receipts(original_path);

-- URLs captured from Chrome extension. `is_new` indicates unread/unreviewed state.
CREATE TABLE IF NOT EXISTS urls (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT,
  url TEXT NOT NULL,
  source TEXT, -- e.g. chrome-extension, manual
  tags TEXT, -- JSON array
  is_new INTEGER DEFAULT 1,
  created_at TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_urls_url ON urls(url);
CREATE INDEX IF NOT EXISTS idx_urls_is_new ON urls(is_new);

-- Optional: a lightweight migrations table to track DB schema upgrades.
CREATE TABLE IF NOT EXISTS migrations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE,
  applied_at TEXT DEFAULT (datetime('now'))
);

-- Notes on usage
- Use the `notes.path`, `receipts.current_path` fields as pointers to on-disk files. The renderer should call Tauri/Rust commands to read bytes for previews.
- URLs are DB-only records (no snapshot files). URLs are opened externally via `tauri-plugin-opener`.
- Persist authoritative config to `maia.json` as the single source of truth for user-editable settings; do not use the DB for defaults.
- Keep tag queries efficient by using the JSON1 functions (e.g., `json_each`) or add a normalized tags table later if needed.

---
Draft created for: `ui-rebuild` — next: review schema, then I can generate a small migration SQL file and Rust/JS access helpers if you want.
