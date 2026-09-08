use std::{path::Path, sync::{Arc, Mutex}};

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
    create_schema(&conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

pub fn init_memory() -> anyhow::Result<DbPool> {
    init_db(Path::new(":memory:"))
}

fn create_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
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

        CREATE TABLE IF NOT EXISTS meal_ingredients (
            id TEXT PRIMARY KEY,
            meal_id TEXT NOT NULL REFERENCES meals(id) ON DELETE CASCADE,
            ingredient_id TEXT NOT NULL REFERENCES ingredients(id) ON DELETE CASCADE,
            grams REAL NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0,
            deleted_at INTEGER
        );

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
            transaction_id TEXT,
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
        "#,
    )?;
    Ok(())
}

pub fn server_time_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn migrations_idempotent() {
        let pool = init_memory().unwrap();
        let conn = pool.lock().unwrap();
        super::create_schema(&conn).unwrap();
        super::create_schema(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))
            .unwrap();
        assert!(count >= 30);
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
