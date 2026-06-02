use rusqlite::{Connection, Result as SqlResult, params};
use std::{path::Path, sync::Arc};

#[derive(Clone)]
pub struct Database {
    conn: Arc<Connection>,
}

impl Database {
    pub fn new(db_path: &Path) -> SqlResult<Self> {
        let conn = Connection::open(db_path)?;

        if !db_path.exists() {
            println!("Creating database at {:?}", db_path);
        }

        Ok(Self {
            conn: Arc::new(conn),
        })
    }
}
