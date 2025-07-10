use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};
use std::path::Path;

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(db_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Create database if it doesn't exist
        if !Sqlite::database_exists(db_path).await.unwrap_or(false) {
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

        tracing::info!("Database migrations completed");
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn get_pool(&self) -> &SqlitePool {
        &self.pool
    }
}
