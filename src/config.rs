use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    receipts_path: Option<PathBuf>,
    bank_statements_path: Option<PathBuf>,
    investments_statements_path: Option<PathBuf>,
    database: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            receipts_path: None,
            bank_statements_path: None,
            investments_statements_path: None,
            database: PathBuf::from("maia.db"),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, std::io::Error> {
        let file = std::fs::read_to_string("maia.json")?;
        let cfg = serde_json::from_str(&file)?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write("maia.json", json)
    }

    pub fn needs_to_listen(&self) -> bool {
        self.receipts_path.is_some()
            || self.bank_statements_path.is_some()
            || self.investments_statements_path.is_some()
    }

    pub fn paths_to_watch(&self) -> Vec<&Path> {
        let mut paths = Vec::with_capacity(3);
        if let Some(ref path) = self.receipts_path {
            paths.push(path.as_path());
        }
        if let Some(ref path) = self.bank_statements_path {
            paths.push(path.as_path());
        }
        if let Some(ref path) = self.investments_statements_path {
            paths.push(path.as_path());
        }
        paths
    }

    pub fn database(&self) -> &Path {
        &self.database
    }
}
