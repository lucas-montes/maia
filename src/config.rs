use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::watcher::{WatchedDir, WatchedDirKind};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    receipts_path: Option<PathBuf>,
    bank_statements_path: Option<PathBuf>,
    investments_statements_path: Option<PathBuf>,
    database: PathBuf,
    model_api: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            receipts_path: None,
            bank_statements_path: None,
            investments_statements_path: None,
            database: PathBuf::from("maia.db"),
            model_api: None,
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

    pub fn watched_dirs(&self) -> Vec<WatchedDir> {
        let mut dirs = Vec::new();
        if let Some(ref path) = self.receipts_path {
            dirs.push(WatchedDir::new(path.clone(), WatchedDirKind::Receipt));
        }
        if let Some(ref path) = self.bank_statements_path {
            dirs.push(WatchedDir::new(path.clone(), WatchedDirKind::Bank));
        }
        if let Some(ref path) = self.investments_statements_path {
            dirs.push(WatchedDir::new(path.clone(), WatchedDirKind::Investment));
        }
        dirs
    }

    pub fn database(&self) -> &Path {
        &self.database
    }

    pub fn model_api(&self) -> Option<String> {
        self.model_api.to_owned()
    }
}
