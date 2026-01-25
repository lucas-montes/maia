use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::watcher::{WatchedDir, WatchedDirKind};


/// The cofiguration for the Maia daemon. It will look at the following paths and the given rate periodically.
/// When a new files is detected it will process it accordingly.
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// Path to the directory to watch for new nutriments images.
    nutriments_path: Option<PathBuf>,
    /// Path to the directory to watch for new receipts.
    receipts_path: Option<PathBuf>,
    /// Path to the directory to watch for new bank statements.
    bank_statements_path: Option<PathBuf>,
    /// Path to the directory to watch for new investment statements.
    investments_statements_path: Option<PathBuf>,
    /// Path to the Sqlite database file.
    database: PathBuf,
    /// Optional API endpoint for the AI model.
    models_api: Vec<ModelApi>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ModelApi {
    endpoint: String,
    token: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            nutriments_path: None,
            receipts_path: None,
            bank_statements_path: None,
            investments_statements_path: None,
            database: PathBuf::from("maia.db"),
            models_api: Vec::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, std::io::Error> {
        let file = std::fs::read("maia.json")?;
        let cfg = serde_json::from_slice(&file)?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write("maia.json", json)
    }

    pub fn needs_to_listen(&self) -> bool {
        //TODO: add the missing paths
        self.receipts_path.is_some()
            || self.bank_statements_path.is_some()
            || self.investments_statements_path.is_some()
    }
//TODO: add the missing paths
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

    //TODO: change this to something better
    pub fn model_api(&self) -> Option<String> {
        self.models_api.first().map(|m|m.token.to_owned().unwrap())
    }

}
