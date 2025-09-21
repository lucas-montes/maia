use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};
use std::path::Path;

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(db_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Create database if it doesn't exist
        if (!Sqlite::database_exists(db_path).await.unwrap_or(false)) {
            tracing::info!("Creating database at {}", db_path);
            Sqlite::create_database(db_path).await?;
        }

        let pool = SqlitePool::connect(db_path).await?;

        // Run migrations
        Self::run_migrations(&pool).await?;

        Ok(Database { pool })
    }

    async fn run_migrations(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Running database migrations");

        // Create todos table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS todos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                description TEXT,
                status TEXT DEFAULT 'pending' CHECK (status IN ('pending', 'done')),
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME
            )
            "#
        )
        .execute(pool)
        .await?;

        // Create notes table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                file_path TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME
            )
            "#
        )
        .execute(pool)
        .await?;

        // Create expenses table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS expenses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                store TEXT,
                total REAL NOT NULL,
                date DATE NOT NULL,
                raw_data JSON,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#
        )
        .execute(pool)
        .await?;

        // Create expense_items table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS expense_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                expense_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                price REAL NOT NULL,
                quantity REAL DEFAULT 1.0,
                category TEXT,
                FOREIGN KEY (expense_id) REFERENCES expenses (id) ON DELETE CASCADE
            )
            "#
        )
        .execute(pool)
        .await?;

        // Create calorie_entries table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS calorie_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                food_item TEXT NOT NULL,
                quantity REAL NOT NULL,
                unit TEXT NOT NULL,
                calories_total REAL NOT NULL,
                date DATE NOT NULL,
                expense_item_id INTEGER,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (expense_item_id) REFERENCES expense_items (id) ON DELETE SET NULL
            )
            "#
        )
        .execute(pool)
        .await?;

        // Health domain tables
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS food_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                calories_per_100g REAL NOT NULL,
                protein REAL,
                carbs REAL,
                fat REAL,
                fiber REAL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS food_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                food_item_id INTEGER NOT NULL,
                food_item_name TEXT NOT NULL,
                quantity REAL NOT NULL,
                meal_type TEXT NOT NULL,
                date TEXT NOT NULL,
                calories_consumed REAL NOT NULL,
                image_path TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (food_item_id) REFERENCES food_items (id)
            )"
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS weight_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                weight REAL NOT NULL,
                date TEXT NOT NULL,
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS workout_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                exercise TEXT NOT NULL,
                weight REAL,
                reps INTEGER,
                sets INTEGER,
                duration_minutes INTEGER,
                date TEXT NOT NULL,
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS grocery_purchases (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                store TEXT,
                total_amount REAL,
                date TEXT NOT NULL,
                receipt_image_path TEXT,
                expense_id INTEGER,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS grocery_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                purchase_id INTEGER NOT NULL,
                food_item_id INTEGER,
                name TEXT NOT NULL,
                quantity REAL NOT NULL,
                unit TEXT NOT NULL,
                price REAL,
                FOREIGN KEY (purchase_id) REFERENCES grocery_purchases (id),
                FOREIGN KEY (food_item_id) REFERENCES food_items (id)
            )"
        )
        .execute(pool)
        .await?;

        tracing::info!("Database migrations completed");
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn get_pool(&self) -> &SqlitePool {
        &self.pool
    }

    // Health domain methods
    pub async fn insert_food_item(&self, food_item: &health::models::FoodItem) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            "INSERT INTO food_items (name, calories_per_100g, protein, carbs, fat, fiber, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
            food_item.name,
            food_item.calories_per_100g,
            food_item.protein,
            food_item.carbs,
            food_item.fat,
            food_item.fiber,
            food_item.created_at
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result.id)
    }

    pub async fn get_food_item_by_name(&self, name: &str) -> Result<health::models::FoodItem, sqlx::Error> {
        let row = sqlx::query!(
            "SELECT id, name, calories_per_100g, protein, carbs, fat, fiber, created_at
             FROM food_items WHERE name = ?",
            name
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(health::models::FoodItem {
            id: Some(row.id),
            name: row.name,
            calories_per_100g: row.calories_per_100g,
            protein: row.protein,
            carbs: row.carbs,
            fat: row.fat,
            fiber: row.fiber,
            created_at: row.created_at,
        })
    }

    pub async fn insert_food_log(&self, food_log: &health::models::FoodLog) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            "INSERT INTO food_logs (food_item_id, food_item_name, quantity, meal_type, date, calories_consumed, image_path, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
            food_log.food_item_id,
            food_log.food_item_name,
            food_log.quantity,
            food_log.meal_type,
            food_log.date,
            food_log.calories_consumed,
            food_log.image_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            food_log.created_at
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result.id)
    }

    pub async fn insert_weight_log(&self, weight_log: &health::models::WeightLog) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            "INSERT INTO weight_logs (weight, date, notes, created_at)
             VALUES (?, ?, ?, ?) RETURNING id",
            weight_log.weight,
            weight_log.date,
            weight_log.notes,
            weight_log.created_at
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result.id)
    }

    pub async fn get_weight_logs_last_n_days(&self, days: u32) -> Result<Vec<health::models::WeightLog>, sqlx::Error> {
        let rows = sqlx::query!(
            "SELECT id, weight, date, notes, created_at
             FROM weight_logs
             WHERE date >= date('now', '-{} days')
             ORDER BY date ASC",
            days
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| health::models::WeightLog {
            id: Some(row.id),
            weight: row.weight,
            date: row.date,
            notes: row.notes,
            created_at: row.created_at,
        }).collect())
    }

    pub async fn insert_workout_log(&self, workout_log: &health::models::WorkoutLog) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            "INSERT INTO workout_logs (exercise, weight, reps, sets, duration_minutes, date, notes, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
            workout_log.exercise,
            workout_log.weight,
            workout_log.reps,
            workout_log.sets,
            workout_log.duration_minutes,
            workout_log.date,
            workout_log.notes,
            workout_log.created_at
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result.id)
    }

    pub async fn insert_grocery_purchase(&self, purchase: &health::models::GroceryPurchase) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            "INSERT INTO grocery_purchases (store, total_amount, date, receipt_image_path, expense_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
            purchase.store,
            purchase.total_amount,
            purchase.date,
            purchase.receipt_image_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            purchase.expense_id,
            purchase.created_at
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result.id)
    }
}
