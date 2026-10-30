//! Polling watcher for statement files.
//!
//! Polling rather than inotify: the set of watched directories is small, the
//! interval is a minute, and a missed event (or a file copied in while the
//! service was down) is handled by the next tick. Re-ingesting a file is a
//! no-op because the statement's `source_key` is derived from its hash.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use super::{SaveOutcome, SourceId, SourceSpec, is_statement_file, run_spec};
use crate::server::AppState;

const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Start the background poller. Does nothing when no directory is configured.
pub fn spawn(state: AppState) {
    if state.config.watch_dirs.is_empty() {
        return;
    }
    tokio::spawn(async move {
        let mut seen = HashSet::new();
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        loop {
            ticker.tick().await;
            scan_once(&state, &mut seen).await;
        }
    });
}

/// One scan pass. `seen` holds `(path, len, mtime)` triples so an unchanged file
/// is not re-read every minute; the database hash check still catches a file
/// that was already ingested before a restart.
pub async fn scan_once(
    state: &AppState,
    seen: &mut HashSet<(PathBuf, u64, u64)>,
) -> Vec<anyhow::Result<Vec<SaveOutcome>>> {
    let mut results = Vec::new();
    let store = state.config.statements_store_dir();
    for dir in &state.config.watch_dirs {
        let Some(source) = SourceId::parse(&dir.source) else {
            tracing::warn!(
                source = %dir.source,
                path = %dir.path.display(),
                "unknown statement source; skipping watch directory"
            );
            continue;
        };
        if source.is_feed() {
            tracing::warn!(
                source = %dir.source,
                "API feed configured as a watch directory; skipping"
            );
            continue;
        }

        let entries = match std::fs::read_dir(&dir.path) {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!(path = %dir.path.display(), %error, "cannot read watch directory");
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || !is_statement_file(&path) {
                continue;
            }
            // The server stores a copy of every input in `<store>/<source>/`.
            // If a watch folder was configured to point there, re-ingesting
            // those copies would at best be wasted work and at worst, before
            // content-only hashing, an endless chain of new statements. Only
            // that exact directory is excluded, so a watch folder can still be
            // a sibling of the store (as the checkout's inbox is).
            let store_dir = store.join(source.as_str());
            if path.parent() == Some(store_dir.as_path()) {
                continue;
            }
            let Some(key) = file_key(&path) else {
                continue;
            };
            if !seen.insert(key) {
                continue;
            }
            let spec = SourceSpec::File {
                source,
                path: path.clone(),
            };
            tracing::info!(path = %path.display(), source = source.as_str(), "ingesting statement");
            let result = run_spec(&spec, state).await;
            match &result {
                Ok(outcomes) => {
                    for outcome in outcomes {
                        tracing::info!(
                            statement = %outcome.statement_id,
                            inserted = outcome.inserted_lines,
                            pending = outcome.pending_lines,
                            skipped = ?outcome.skipped_reason,
                            "statement staged"
                        );
                    }
                }
                Err(error) => {
                    // Keep the file in `seen`: a parser error will not fix
                    // itself on the next minute. `POST /statements/scan` uses a
                    // fresh set when the user wants to retry.
                    tracing::error!(path = %path.display(), %error, "statement ingestion failed");
                }
            }
            results.push(result);
        }
    }
    results
}

fn file_key(path: &Path) -> Option<(PathBuf, u64, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    let len = metadata.len();
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    Some((path.to_path_buf(), len, mtime))
}
