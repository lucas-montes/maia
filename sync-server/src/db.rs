use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, bail};
use rusqlite::Connection;

pub type DbPool = Arc<Mutex<Connection>>;

pub fn init_db(path: &Path) -> anyhow::Result<DbPool> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let conn = if path.as_os_str() == ":memory:" {
        Connection::open_in_memory()?
    } else {
        Connection::open(path)?
    };
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    migrate(&conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

pub fn init_memory() -> anyhow::Result<DbPool> {
    init_db(Path::new(":memory:"))
}

// ---------------------------------------------------------------------------
// Schema migrations
//
// The schema is embedded in this binary, so the binary *is* the schema
// authority. `PRAGMA user_version` records which step has been applied and is
// the only version signal; there is no separate migrations table.
//
// To add a migration: append an entry to `MIGRATIONS` with the next version and
// bump `LATEST_VERSION` to match. Never edit or renumber an existing entry.
// ---------------------------------------------------------------------------

/// How many pre-migration snapshots to retain under `<db_dir>/snapshots`.
const SNAPSHOT_KEEP: usize = 10;

/// One step in the schema history.
///
/// `Guarded` exists because SQLite has no `ALTER TABLE ... ADD COLUMN IF NOT
/// EXISTS`, so a column addition has to be conditional in Rust. Holding it as a
/// normal step keeps it versioned, ordered and transactional instead of
/// re-running on every boot.
pub enum Step {
    /// Executed as a single batch.
    Sql(&'static str),
    /// Runs inside the same transaction as any other step.
    Guarded(fn(&Connection) -> anyhow::Result<()>),
}

/// Ordered, append-only schema history. Each version is the `user_version` the
/// step leaves behind.
pub const MIGRATIONS: &[(i64, Step)] = &[
    // The original boot-time DDL, verbatim. Every statement is `IF NOT EXISTS`,
    // so replaying it over a database created before versioning existed is a
    // no-op for tables it already has *and* creates the ones it is missing.
    (1, Step::Sql(V1_SCHEMA)),
    // `receipts` columns that shipped after the v1 DDL was frozen. Existing
    // databases need the `ALTER` because the `CREATE TABLE IF NOT EXISTS`
    // receipts statement in v1 leaves an already-present table untouched.
    (2, Step::Guarded(add_receipt_columns)),
    // `meal_ingredients` was superseded by the `foods` / `food_ingredients` /
    // `meal_foods` layer and nothing on the server reads it any more.
    (3, Step::Sql("DROP TABLE IF EXISTS meal_ingredients;")),
];

/// Highest version in `MIGRATIONS`.
///
/// A database newer than this means the data is newer than the binary, which is
/// what a Nix rollback looks like. `migrate` refuses rather than serving.
pub const LATEST_VERSION: i64 = 3;

/// The v1 schema, exactly as it shipped.
///
/// Kept verbatim so that databases already at v1 need no change and so the
/// fresh-database path is byte-identical to the old `create_schema`.
const V1_SCHEMA: &str = r#"

        CREATE TABLE IF NOT EXISTS ingredients (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            calories_per100g REAL NOT NULL,
            protein_per100g REAL NOT NULL,
            carbs_per100g REAL NOT NULL,
            fat_per100g REAL NOT NULL,
            sodium_per100g REAL,
            fiber_per100g REAL,
            sugar_per100g REAL,
            is_archived INTEGER NOT NULL DEFAULT 0,
            brand TEXT,
            barcode TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_ingredients_barcode ON ingredients(barcode);
        CREATE INDEX IF NOT EXISTS idx_ingredients_updated ON ingredients(updated_at);

        CREATE TABLE IF NOT EXISTS stores (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_stores_updated ON stores(updated_at);

        CREATE TABLE IF NOT EXISTS ingredient_pictures (
            id TEXT PRIMARY KEY,
            ingredient_id TEXT NOT NULL REFERENCES ingredients(id) ON DELETE CASCADE,
            image_path TEXT NOT NULL,
            sort_order INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_ingredient_pictures_ingredient ON ingredient_pictures(ingredient_id);

        CREATE TABLE IF NOT EXISTS ingredient_prices (
            id TEXT PRIMARY KEY,
            ingredient_id TEXT NOT NULL REFERENCES ingredients(id) ON DELETE CASCADE,
            store_id TEXT NOT NULL REFERENCES stores(id) ON DELETE CASCADE,
            price REAL NOT NULL,
            currency_code TEXT NOT NULL,
            package_grams REAL,
            recorded_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER,
            UNIQUE(ingredient_id, store_id, recorded_at)
        );
        CREATE INDEX IF NOT EXISTS idx_ingredient_prices_updated ON ingredient_prices(updated_at);

        CREATE TABLE IF NOT EXISTS meals (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            eaten_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        -- A food logged in a meal, with the amount eaten.
        --
        -- The macro columns are a **snapshot of this portion**, resolved by the
        -- client at log time through foods -> food_ingredients -> ingredients
        -- and stored verbatim. Reading a meal is therefore join-free, and
        -- editing an ingredient afterwards cannot restate a meal that was
        -- already logged.
        --
        -- The food_id reference is deliberately NO ACTION: an archived food
        -- stays referenced so the snapshot keeps rendering.
        CREATE TABLE IF NOT EXISTS meal_foods (
            id TEXT PRIMARY KEY,
            meal_id TEXT NOT NULL REFERENCES meals(id) ON DELETE CASCADE,
            food_id TEXT NOT NULL REFERENCES foods(id),
            amount REAL NOT NULL,
            calories REAL NOT NULL,
            protein REAL NOT NULL,
            carbs REAL NOT NULL,
            fat REAL NOT NULL,
            sodium REAL,
            fiber REAL,
            sugar REAL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_meal_foods_meal ON meal_foods(meal_id);

        -- A food is a recipe: a named collection of ingredients with amounts.
        -- It carries no nutrition of its own — that is always resolved through
        -- food_ingredients -> ingredients, so editing an ingredient is
        -- immediately visible in every food built from it.
        --
        -- Deliberately no per-100g macro columns here. A stored copy would go
        -- stale the moment an ingredient is edited, and the whole point of the
        -- foods layer is that composition is the single source of truth.
        CREATE TABLE IF NOT EXISTS foods (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            archived_at INTEGER,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        -- The composition of a food: which ingredients go in it, and how much
        -- of each. The sum of `amount` across a food is the batch weight its
        -- per-100g figures are derived from, so there is no yield column.
        --
        -- Components are presented alphabetically by ingredient name; there is
        -- no stored sort order, because the order carries no meaning.
        --
        -- Cascade rules:
        --   * food_id CASCADE — deleting the food drops its composition.
        --   * ingredient_id NO ACTION — a part is never implicitly detached,
        --     because that would silently change what a logged food means.
        CREATE TABLE IF NOT EXISTS food_ingredients (
            food_id TEXT NOT NULL REFERENCES foods(id) ON DELETE CASCADE,
            ingredient_id TEXT NOT NULL REFERENCES ingredients(id),
            amount REAL NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER,
            PRIMARY KEY (food_id, ingredient_id)
        );
        CREATE INDEX IF NOT EXISTS idx_food_ingredients_ingredient ON food_ingredients(ingredient_id);

        CREATE TABLE IF NOT EXISTS exercises (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            exercise_type TEXT NOT NULL,
            body_part TEXT,
            equipment TEXT,
            primary_muscle TEXT,
            secondary_muscle TEXT,
            instructions TEXT,
            tips TEXT,
            faqs TEXT,
            keywords TEXT,
            image_path TEXT,
            video_path TEXT,
            similar_to TEXT,
            tags TEXT,
            is_canonical INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_exercises_updated ON exercises(updated_at);

        CREATE TABLE IF NOT EXISTS workouts (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            date INTEGER NOT NULL,
            started_at INTEGER,
            completed_at INTEGER,
            notes TEXT,
            routine_id TEXT,
            template_id TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_workouts_updated ON workouts(updated_at);
        CREATE INDEX IF NOT EXISTS idx_workouts_routine ON workouts(routine_id);

        CREATE TABLE IF NOT EXISTS workout_templates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            notes TEXT,
            start_date INTEGER NOT NULL,
            recurrence TEXT,
            excluded_dates TEXT,
            source_routine_id TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS workout_template_exercises (
            id TEXT PRIMARY KEY,
            template_id TEXT NOT NULL REFERENCES workout_templates(id) ON DELETE CASCADE,
            exercise_id TEXT NOT NULL REFERENCES exercises(id),
            sort_order INTEGER NOT NULL,
            notes TEXT,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS workout_template_sets (
            id TEXT PRIMARY KEY,
            template_exercise_id TEXT NOT NULL REFERENCES workout_template_exercises(id) ON DELETE CASCADE,
            set_number INTEGER NOT NULL,
            reps INTEGER,
            weight_kg REAL,
            rest_seconds INTEGER,
            duration_minutes INTEGER,
            distance_meters REAL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS workout_exercises (
            id TEXT PRIMARY KEY,
            workout_id TEXT NOT NULL REFERENCES workouts(id) ON DELETE CASCADE,
            exercise_id TEXT NOT NULL REFERENCES exercises(id),
            sort_order INTEGER NOT NULL,
            notes TEXT,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS exercise_sets (
            id TEXT PRIMARY KEY,
            workout_exercise_id TEXT NOT NULL REFERENCES workout_exercises(id) ON DELETE CASCADE,
            set_number INTEGER NOT NULL,
            reps INTEGER,
            weight_kg REAL,
            rest_seconds INTEGER,
            actual_reps INTEGER,
            actual_weight_kg REAL,
            actual_rest_seconds INTEGER,
            completed_at INTEGER,
            duration_minutes INTEGER,
            distance_meters REAL,
            actual_duration_minutes INTEGER,
            actual_distance_meters REAL,
            notes TEXT,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS notes (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '',
            updated_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            date INTEGER NOT NULL,
            title TEXT NOT NULL,
            done INTEGER NOT NULL,
            task_status INTEGER,
            carry_over INTEGER NOT NULL DEFAULT 1,
            sort_order INTEGER NOT NULL,
            due_date INTEGER,
            due_time_minutes INTEGER,
            start_time_minutes INTEGER,
            end_time_minutes INTEGER,
            notes TEXT,
            workout_id TEXT,
            workout_template_id TEXT,
            recurrence TEXT,
            series_id TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_tasks_updated ON tasks(updated_at);

        CREATE TABLE IF NOT EXISTS experiments (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            purpose TEXT,
            start_date INTEGER NOT NULL,
            end_date INTEGER,
            status TEXT NOT NULL DEFAULT 'planned',
            categories TEXT,
            reminder_enabled INTEGER NOT NULL DEFAULT 1,
            reminder_time_minutes INTEGER NOT NULL DEFAULT 1200,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS experiment_checkins (
            id TEXT PRIMARY KEY,
            experiment_id TEXT NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
            day INTEGER NOT NULL,
            rating INTEGER NOT NULL,
            note TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER,
            UNIQUE(experiment_id, day)
        );

        CREATE TABLE IF NOT EXISTS tags (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            color INTEGER,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS goals (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT,
            start_date INTEGER NOT NULL,
            end_date INTEGER,
            status TEXT NOT NULL DEFAULT 'planned',
            target_type TEXT NOT NULL DEFAULT 'none',
            target_value REAL,
            baseline_value REAL,
            unit TEXT,
            reminder_enabled INTEGER NOT NULL DEFAULT 0,
            reminder_time_minutes INTEGER NOT NULL DEFAULT 1200,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS goal_progress_entries (
            id TEXT PRIMARY KEY,
            goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
            recorded_at INTEGER NOT NULL,
            value REAL NOT NULL,
            note TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER,
            UNIQUE(goal_id, recorded_at)
        );

        CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            type TEXT NOT NULL,
            opening_balance REAL NOT NULL DEFAULT 0.0,
            note TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS transactions (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL,
            amount REAL NOT NULL,
            currency_code TEXT NOT NULL,
            amount_base REAL NOT NULL,
            rate_used REAL NOT NULL,
            account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
            to_account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
            category TEXT,
            date INTEGER NOT NULL,
            note TEXT,
            receipt_id TEXT,
            is_draft INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS receipts (
            id TEXT PRIMARY KEY,
            local_path TEXT NOT NULL,
            remote_path TEXT,
            upload_status INTEGER NOT NULL DEFAULT 0,
            parsed INTEGER NOT NULL DEFAULT 0,
            parsed_json TEXT,
            parse_error TEXT,
            transaction_id TEXT,
            archived_path TEXT,
            archived_at INTEGER,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS fx_rates (
            code TEXT NOT NULL,
            base_code TEXT NOT NULL,
            rate_date TEXT NOT NULL DEFAULT '0001-01-01',
            rate_to_base REAL NOT NULL,
            updated_at INTEGER NOT NULL,
            manual INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER,
            PRIMARY KEY (code, base_code, rate_date)
        );
        CREATE INDEX IF NOT EXISTS idx_fx_rates_updated ON fx_rates(updated_at);

        CREATE TABLE IF NOT EXISTS body_metrics (
            id TEXT PRIMARY KEY,
            date INTEGER NOT NULL,
            weight_kg REAL,
            height_cm REAL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS task_tags (
            tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            PRIMARY KEY (tag_id, task_id)
        );
        CREATE TABLE IF NOT EXISTS experiment_tags (
            tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            experiment_id TEXT NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
            PRIMARY KEY (tag_id, experiment_id)
        );
        CREATE TABLE IF NOT EXISTS goal_tags (
            tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
            PRIMARY KEY (tag_id, goal_id)
        );
        CREATE TABLE IF NOT EXISTS note_tags (
            tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            PRIMARY KEY (tag_id, note_id)
        );
        CREATE TABLE IF NOT EXISTS task_experiments (
            task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            experiment_id TEXT NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
            label TEXT,
            PRIMARY KEY (task_id, experiment_id)
        );
        CREATE TABLE IF NOT EXISTS task_goals (
            task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
            label TEXT,
            PRIMARY KEY (task_id, goal_id)
        );
        CREATE TABLE IF NOT EXISTS experiment_goals (
            experiment_id TEXT NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
            goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
            label TEXT,
            PRIMARY KEY (experiment_id, goal_id)
        );
        CREATE TABLE IF NOT EXISTS task_notes (
            task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            label TEXT,
            PRIMARY KEY (task_id, note_id)
        );
        CREATE TABLE IF NOT EXISTS experiment_notes (
            experiment_id TEXT NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
            note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            label TEXT,
            PRIMARY KEY (experiment_id, note_id)
        );
        CREATE TABLE IF NOT EXISTS goal_notes (
            goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
            note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            label TEXT,
            PRIMARY KEY (goal_id, note_id)
        );
        CREATE TABLE IF NOT EXISTS goal_workouts (
            goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
            workout_id TEXT NOT NULL REFERENCES workouts(id) ON DELETE CASCADE,
            label TEXT,
            PRIMARY KEY (goal_id, workout_id)
        );
        CREATE TABLE IF NOT EXISTS note_workouts (
            note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            workout_id TEXT NOT NULL REFERENCES workouts(id) ON DELETE CASCADE,
            label TEXT,
            PRIMARY KEY (note_id, workout_id)
        );
        CREATE TABLE IF NOT EXISTS note_audio (
            id TEXT PRIMARY KEY,
            note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            audio_path TEXT NOT NULL,
            duration_ms INTEGER NOT NULL DEFAULT 0,
            position INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS urls (
            id TEXT PRIMARY KEY,
            title TEXT,
            url TEXT NOT NULL,
            source TEXT,
            tags TEXT,
            is_new INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_urls_url ON urls(url);
        CREATE INDEX IF NOT EXISTS idx_urls_is_new ON urls(is_new);

"#;

/// Brings `conn` up to [`LATEST_VERSION`].
///
/// Safe to call on every boot and from several processes at once: each step
/// takes `BEGIN IMMEDIATE`, so SQLite serialises concurrent runners and the
/// loser waits on `busy_timeout` instead of failing. Steps at or below the
/// current version are skipped, so repeated calls are no-ops.
pub fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch("PRAGMA busy_timeout = 5000;")?;

    let current = user_version(conn)?;
    if current > LATEST_VERSION {
        bail!(
            "database is at v{current} but this binary only knows v{LATEST_VERSION}.\n\
             The data is newer than the server binary, which is what a Nix rollback \
             looks like: an older `sync-server` cannot know how to read schema \
             v{current}.\n\
             Rebuild forward (`nixos-rebuild switch`) or restore a snapshot:\n    \
             sqlite3 <db> \".restore snapshots/maia.v{LATEST_VERSION}.db\""
        );
    }

    let pending: Vec<(i64, &Step)> = MIGRATIONS
        .iter()
        .filter(|(v, _)| *v > current)
        .map(|(v, step)| (*v, step))
        .collect();

    // Tracks the version the database is at right now, so each snapshot is named
    // for the state it preserves rather than repeating the starting version.
    let mut from = current;
    for (version, step) in pending {
        tracing::info!(version, "applying schema migration");
        // Rollback path first. `VACUUM INTO` cannot run inside a transaction.
        if let Some(path) = snapshot(conn, from)? {
            tracing::info!(path = %path.display(), "wrote pre-migration snapshot");
        }
        from = version;

        conn.execute_batch("BEGIN IMMEDIATE;")?;
        let applied: anyhow::Result<()> = match step {
            Step::Sql(sql) => conn.execute_batch(sql).map_err(Into::into),
            Step::Guarded(f) => f(conn),
        };
        match applied {
            Ok(()) => {
                set_user_version(conn, version)?;
                conn.execute_batch("COMMIT;")?;
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK;");
                return Err(e).with_context(|| format!("migration v{version} failed"));
            }
        }
    }
    Ok(())
}

/// The last applied migration, i.e. `PRAGMA user_version`.
pub fn user_version(conn: &Connection) -> anyhow::Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

fn set_user_version(conn: &Connection, version: i64) -> anyhow::Result<()> {
    conn.pragma_update(None, "user_version", version)?;
    Ok(())
}

/// True when a table with this name exists in `main`. Callers pass literals.
fn table_exists(conn: &Connection, table: &str) -> anyhow::Result<bool> {
    // `table` is always a literal from the caller, never from a request.
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Columns added to `receipts` after the v1 DDL was frozen, applied by v2.
const RECEIPT_COLUMNS: &[(&str, &str)] = &[
    // Set when a background OCR job fails, so the phone can report why.
    ("parse_error", "TEXT"),
    // Desktop-side archive location and time; see `archive_receipt`.
    ("archived_path", "TEXT"),
    ("archived_at", "INTEGER"),
];

/// v2: the `receipts` columns added after the v1 DDL was frozen.
fn add_receipt_columns(conn: &Connection) -> anyhow::Result<()> {
    for (name, ty) in RECEIPT_COLUMNS {
        if column_exists(conn, "receipts", name)? {
            continue;
        }
        // `name`/`ty` come from the const table above, never from user input.
        conn.execute_batch(&format!("ALTER TABLE receipts ADD COLUMN {name} {ty}"))?;
    }
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> anyhow::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Consistent pre-migration copy of the database, named for the version it was
/// taken at. Returns `None` for in-memory databases.
///
/// `VACUUM INTO` rather than `cp` because the database is live in WAL mode: a
/// copy of the main file can miss committed rows still sitting in the `-wal`
/// sidecar. It also compacts, and cannot run inside a transaction, so it runs
/// before the step's `BEGIN IMMEDIATE`.
fn snapshot(conn: &Connection, from_version: i64) -> anyhow::Result<Option<PathBuf>> {
    let Some(path) = db_path(conn)? else {
        return Ok(None);
    };
    let dir = path.parent().unwrap_or(Path::new(".")).join("snapshots");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let target = dir.join(format!("maia.v{from_version}.db"));
    // An interrupted earlier attempt may have left a partial file behind.
    let _ = fs::remove_file(&target);
    // `VACUUM INTO` takes a literal, not a bound parameter. This path is derived
    // from our own database file, never from a request.
    let escaped = target.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{escaped}'"))
        .with_context(|| format!("snapshotting database to {}", target.display()))?;
    rotate_snapshots(&dir, SNAPSHOT_KEEP)?;
    Ok(Some(target))
}

/// Filesystem path of the database, or `None` for in-memory databases.
fn db_path(conn: &Connection) -> anyhow::Result<Option<PathBuf>> {
    let file: String = conn.query_row("PRAGMA database_list", [], |r| r.get(2))?;
    if file.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(file)))
}

/// Keeps the `keep` most recent `maia.v<N>.db` snapshots and prunes the rest.
fn rotate_snapshots(dir: &Path, keep: usize) -> anyhow::Result<()> {
    let mut snapshots: Vec<(i64, PathBuf)> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(version) = name
            .strip_prefix("maia.v")
            .and_then(|rest| rest.strip_suffix(".db"))
            .and_then(|digits| digits.parse::<i64>().ok())
        else {
            continue;
        };
        snapshots.push((version, entry.path()));
    }
    // Highest version first, so the tail we skip holds the oldest.
    snapshots.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    for (_, stale) in snapshots.into_iter().skip(keep) {
        tracing::info!(path = %stale.display(), "pruning old snapshot");
        let _ = fs::remove_file(stale);
    }
    Ok(())
}

pub fn server_time_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// Unique scratch directory, removed by the caller.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("maia-test-{tag}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A connection with no schema and no version stamp — the state of a
    /// database from before versioning existed.
    fn legacy_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .unwrap();
        conn
    }

    /// Fresh databases go straight to the current version.
    #[test]
    fn fresh_db_reaches_latest_version() {
        let pool = init_memory().unwrap();
        let conn = pool.lock().unwrap();
        assert_eq!(super::user_version(&conn).unwrap(), super::LATEST_VERSION);
        let count: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))
            .unwrap();
        assert!(count >= 30, "expected the v1 schema to be created, saw {count} tables");
    }

    /// Running the runner again on an up-to-date database does nothing.
    #[test]
    fn migrate_is_idempotent() {
        let pool = init_memory().unwrap();
        let conn = pool.lock().unwrap();
        super::migrate(&conn).unwrap();
        super::migrate(&conn).unwrap();
        assert_eq!(super::user_version(&conn).unwrap(), super::LATEST_VERSION);
    }

    /// A pre-versioning database (`user_version` 0, older table set) is brought
    /// all the way forward.
    ///
    /// Replaying v1 is safe precisely because every statement in it is
    /// `IF NOT EXISTS`, and it is necessary: a database created before a table was
    /// added only gets that table by running v1.
    #[test]
    fn legacy_database_is_stamped_and_brought_forward() {
        let conn = legacy_db();
        // A v1-era database: the original DDL minus the tables added since, plus the
        // `meal_ingredients` table the foods layer superseded.
        conn.execute_batch(super::V1_SCHEMA).unwrap();
        conn.execute_batch(
            "DROP TABLE foods;
             DROP TABLE food_ingredients;
             DROP TABLE meal_foods;
             CREATE TABLE meal_ingredients (
                 meal_id TEXT NOT NULL,
                 ingredient_id TEXT NOT NULL,
                 amount REAL NOT NULL
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO urls (id, url, created_at) VALUES ('u1', 'https://example.com', 1000)",
            [],
        )
        .unwrap();
        assert_eq!(super::user_version(&conn).unwrap(), 0);

        super::migrate(&conn).unwrap();

        assert_eq!(super::user_version(&conn).unwrap(), super::LATEST_VERSION);
        let n: i64 = conn.query_row("SELECT count(*) FROM urls WHERE id='u1'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "migration must not lose rows");
        // v1 has to actually run, otherwise tables added after this database was
        // created are never created at all.
        for table in ["foods", "food_ingredients", "meal_foods"] {
            assert!(
                super::table_exists(&conn, table).unwrap(),
                "v1 did not create {table}"
            );
        }
        assert!(!super::table_exists(&conn, "meal_ingredients").unwrap());
    }

    /// `CREATE TABLE IF NOT EXISTS` cannot widen a table that already exists, so
    /// v2 repairs a `receipts` table from before those columns shipped.
    #[test]
    fn receipt_columns_added_to_pre_existing_table() {
        let conn = legacy_db();
        conn.execute_batch(super::V1_SCHEMA).unwrap();
        conn.execute_batch(
            "DROP TABLE receipts;
             CREATE TABLE receipts (
                 id TEXT PRIMARY KEY,
                 local_path TEXT NOT NULL,
                 remote_path TEXT,
                 upload_status INTEGER NOT NULL DEFAULT 0,
                 parsed INTEGER NOT NULL DEFAULT 0,
                 parsed_json TEXT,
                 transaction_id TEXT,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL DEFAULT 0,
                 deleted_at INTEGER
             );",
        )
        .unwrap();
        assert!(!super::column_exists(&conn, "receipts", "parse_error").unwrap());

        super::migrate(&conn).unwrap();

        for (name, _) in super::RECEIPT_COLUMNS {
            assert!(
                super::column_exists(&conn, "receipts", name).unwrap(),
                "missing column {name}"
            );
        }
        // The new columns are usable.
        conn.execute(
            "INSERT INTO receipts (id, local_path, created_at, parse_error, archived_path, archived_at)
             VALUES ('r1', '/tmp/x.jpg', 1000, 'boom', 'receipts_archive/r1', 2000)",
            [],
        )
        .unwrap();
        let (err, arch): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT parse_error, archived_path FROM receipts WHERE id = 'r1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(err.as_deref(), Some("boom"));
        assert_eq!(arch.as_deref(), Some("receipts_archive/r1"));
    }

    /// The v3 step removes the superseded table.
    #[test]
    fn orphan_meal_ingredients_table_dropped() {
        let conn = legacy_db();
        conn.execute_batch(super::V1_SCHEMA).unwrap();
        conn.execute_batch(
            "CREATE TABLE meal_ingredients (
                 meal_id TEXT NOT NULL,
                 ingredient_id TEXT NOT NULL,
                 amount REAL NOT NULL
             );",
        )
        .unwrap();
        assert!(super::table_exists(&conn, "meal_ingredients").unwrap());

        super::migrate(&conn).unwrap();

        assert!(!super::table_exists(&conn, "meal_ingredients").unwrap());
        assert!(super::table_exists(&conn, "foods").unwrap());
        assert!(super::table_exists(&conn, "food_ingredients").unwrap());
    }

    /// A database newer than the binary is a Nix rollback. Serving it would fail
    /// later, at query time, with confusing "no such column" errors.
    #[test]
    fn database_newer_than_binary_is_refused() {
        let pool = init_memory().unwrap();
        let conn = pool.lock().unwrap();
        super::migrate(&conn).unwrap();
        super::set_user_version(&conn, super::LATEST_VERSION + 5).unwrap();

        let err = super::migrate(&conn).unwrap_err().to_string();
        assert!(err.contains("newer"), "unhelpful error: {err}");
        assert!(err.contains("rollback"), "should name the rollback case: {err}");
    }

    /// Each version change is preceded by a consistent snapshot named for the
    /// version it was taken at.
    #[test]
    fn snapshot_written_before_each_migration() {
        let dir = temp_dir("snapshot");
        let db_path = dir.join("maia.db");
        let conn = Connection::open(&db_path).unwrap();
        super::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO receipts (id, local_path, created_at) VALUES ('r1', '/tmp/x.jpg', 1000)",
            [],
        )
        .unwrap();
        // Pretend an older binary left the database at v1.
        super::set_user_version(&conn, 1).unwrap();

        super::migrate(&conn).unwrap();

        let snapshots = dir.join("snapshots");
        let taken = snapshots.join("maia.v1.db");
        assert!(taken.exists(), "expected {} to exist", taken.display());
        // The snapshot is a real database holding the row committed before the
        // migration, which is the whole point of taking it first.
        let snap = Connection::open(&taken).unwrap();
        let n: i64 = snap
            .query_row("SELECT count(*) FROM receipts WHERE id='r1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "snapshot must capture committed rows");
        assert_eq!(super::user_version(&conn).unwrap(), super::LATEST_VERSION);
    }

    /// Rotation prunes the oldest snapshots once the retention count is exceeded.
    #[test]
    fn snapshot_rotation_keeps_the_newest() {
        let dir = temp_dir("rotate");
        for v in 1..=(super::SNAPSHOT_KEEP as i64 + 3) {
            fs::write(dir.join(format!("maia.v{v}.db")), b"x").unwrap();
        }
        fs::write(dir.join("unrelated.txt"), b"x").unwrap();

        super::rotate_snapshots(&dir, super::SNAPSHOT_KEEP).unwrap();

        let kept: Vec<i64> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| {
                let n = e.unwrap().file_name().into_string().ok()?;
                n.strip_prefix("maia.v")?.strip_suffix(".db")?.parse().ok()
            })
            .collect();
        assert_eq!(kept.len(), super::SNAPSHOT_KEEP);
        assert!(!dir.join("maia.v1.db").exists(), "oldest should be pruned");
        assert!(dir.join("unrelated.txt").exists(), "unrelated files untouched");
    }
    #[test]
    fn unique_constraints_hold() {
        let pool = init_memory().unwrap();
        let conn = pool.lock().unwrap();
        conn.execute(
            "INSERT INTO stores (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params!["s1", "Carrefour", 1000, 1000],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at) VALUES (?1, ?2, 100, 10, 20, 5, ?3, ?4)",
            params!["i1", "Oats", 1000, 1000],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ingredient_prices (id, ingredient_id, store_id, price, currency_code, recorded_at, created_at, updated_at) VALUES (?1, ?2, ?3, 2.4, 'EUR', 1000, 1000, 1000)",
            params!["p1", "i1", "s1"],
        )
        .unwrap();
        let err = conn
            .execute(
                "INSERT INTO ingredient_prices (id, ingredient_id, store_id, price, currency_code, recorded_at, created_at, updated_at) VALUES (?1, ?2, ?3, 3.0, 'EUR', 1000, 1000, 1000)",
                params!["p2", "i1", "s1"],
            )
            .unwrap_err();
        assert!(err.to_string().contains("UNIQUE"));

        conn.execute(
            "INSERT INTO fx_rates (code, base_code, rate_date, rate_to_base, updated_at) VALUES ('EUR', 'USD', '2026-09-08', 0.92, 1000)",
            params![],
        )
        .unwrap();
        let err2 = conn
            .execute(
                "INSERT INTO fx_rates (code, base_code, rate_date, rate_to_base, updated_at) VALUES ('EUR', 'USD', '2026-09-08', 0.93, 1001)",
                params![],
            )
            .unwrap_err();
        assert!(err2.to_string().contains("UNIQUE") || err2.to_string().contains("PRIMARY"));
    }

    #[test]
    fn can_insert_aggregates() {
        let pool = init_memory().unwrap();
        let conn = pool.lock().unwrap();
        conn.execute(
            "INSERT INTO workouts (id, name, date, created_at, updated_at) VALUES ('w1', 'Push', 1000, 1000, 1000)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e1', 'Bench', 'weightlifting', 1000, 1000)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workout_exercises (id, workout_id, exercise_id, sort_order, updated_at) VALUES ('we1', 'w1', 'e1', 0, 1000)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exercise_sets (id, workout_exercise_id, set_number, updated_at) VALUES ('s1', 'we1', 1, 1000)",
            [],
        )
        .unwrap();
        let c: i64 = conn
            .query_row("SELECT count(*) FROM workout_exercises", [], |r| r.get(0))
            .unwrap();
        assert_eq!(c, 1);
    }

    #[test]
    fn since_filtering() {
        let pool = init_memory().unwrap();
        let conn = pool.lock().unwrap();
        conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e1','A','weightlifting',1000,1000)", []).unwrap();
        conn.execute("INSERT INTO exercises (id, name, exercise_type, created_at, updated_at) VALUES ('e2','B','weightlifting',2000,2000)", []).unwrap();
        let mut stmt = conn.prepare("SELECT id FROM exercises WHERE updated_at > ?1 ORDER BY updated_at").unwrap();
        let ids: Vec<String> = stmt.query_map(params![1500], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(ids, vec!["e2"]);
    }
}
