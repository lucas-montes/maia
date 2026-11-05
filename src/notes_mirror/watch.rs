//! Live readback for the markdown mirror.
//!
//! Shaped like `statements/watch.rs`: inotify is the trigger, not the source
//! of truth, and a periodic sweep is the backstop for whatever inotify
//! misses (unavailable, a queue overflow, a file touched while the service
//! was down). Every trigger — event or sweep — runs the exact same
//! [`super::reconcile`] pass the startup scan uses, debounced so a burst of
//! editor writes collapses into one pass instead of one per event.
//!
//! Running the same reconcile on every trigger is also what gets rename
//! detection and self-write suppression for free: [`super::reconcile`]
//! matches files to notes by the `id` in their frontmatter, not by path, so a
//! file that disappeared from one path and reappeared at another with the
//! same id is just "found" again rather than treated as a delete followed by
//! an unknown file. [`super::SignatureMap`] is what keeps the same pass from
//! mistaking its own last export for an edit worth reimporting.

use std::path::PathBuf;
use std::time::Duration;

use inotify::{Inotify, WatchMask};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::server::AppState;

/// Collapses a burst of inotify events (an editor's temp-write-then-rename
/// routinely fires two or three) into a single reconcile pass.
const DEBOUNCE: Duration = Duration::from_millis(400);

/// Backstop sweep, not the primary path: inotify delivers the common case
/// within the debounce window, and this catches whatever it missed.
const SWEEP_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Starts the watcher. A no-op when `notes_dir` is not configured.
pub fn spawn(state: AppState) {
    let Some(notes_dir) = state.config.notes_dir.clone() else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(&notes_dir) {
        tracing::warn!(error = %error, path = %notes_dir.display(), "cannot create notes_dir");
        return;
    }
    let events = start_inotify(&notes_dir);
    tokio::spawn(main_loop(state, notes_dir, events));
}

fn start_inotify(notes_dir: &std::path::Path) -> Option<UnboundedReceiver<()>> {
    let (sender, receiver) = unbounded_channel::<()>();
    let inotify = match Inotify::init() {
        Ok(inotify) => inotify,
        Err(error) => {
            tracing::warn!(error = %error, "inotify unavailable for notes mirror; relying on the sweep");
            return None;
        }
    };
    // `CLOSE_WRITE` is a finished write; `MOVED_TO`/`MOVED_FROM` cover an
    // editor's atomic save (temp file written, then renamed over the
    // original) and a manual rename; `DELETE` is what a tombstone check
    // reacts to. The exact event kind does not matter beyond "something
    // happened" — every trigger runs the same full reconcile.
    let watch = inotify.watches().add(
        notes_dir,
        WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO | WatchMask::MOVED_FROM | WatchMask::DELETE,
    );
    if let Err(error) = watch {
        tracing::warn!(path = %notes_dir.display(), error = %error, "cannot watch notes_dir");
        return None;
    }

    let thread = std::thread::Builder::new()
        .name("notes-mirror-watcher".to_string())
        .spawn(move || inotify_loop(inotify, sender));
    match thread {
        Ok(_) => Some(receiver),
        Err(error) => {
            tracing::warn!(error = %error, "cannot start the notes mirror watcher thread");
            None
        }
    }
}

fn inotify_loop(mut inotify: Inotify, sender: UnboundedSender<()>) {
    let mut buffer = [0u8; 4096];
    loop {
        let events = match inotify.read_events_blocking(&mut buffer) {
            Ok(events) => events,
            Err(error) => {
                tracing::warn!(error = %error, "notes mirror watcher stopped; relying on the sweep");
                return;
            }
        };
        // One trigger per batch is enough: the receiver side debounces and
        // the next reconcile pass sees the directory's current state
        // regardless of how many individual events got here.
        if events.count() > 0 && sender.send(()).is_err() {
            return;
        }
    }
}

async fn main_loop(state: AppState, notes_dir: PathBuf, mut events: Option<UnboundedReceiver<()>>) {
    let mut sweep = tokio::time::interval(SWEEP_INTERVAL);
    // The startup reconcile in `start_server_with_shutdown` already covered
    // this pass; consuming the interval's immediate first tick here avoids
    // running it a second time back to back.
    sweep.tick().await;
    loop {
        match events.as_mut() {
            Some(rx) => {
                tokio::select! {
                    received = rx.recv() => {
                        match received {
                            Some(()) => {
                                tokio::time::sleep(DEBOUNCE).await;
                                while rx.try_recv().is_ok() {}
                                run_once(&state, &notes_dir).await;
                            }
                            None => {
                                tracing::warn!("notes mirror watcher stream closed; relying on the sweep");
                                events = None;
                            }
                        }
                    }
                    _ = sweep.tick() => {
                        run_once(&state, &notes_dir).await;
                    }
                }
            }
            None => {
                sweep.tick().await;
                run_once(&state, &notes_dir).await;
            }
        }
    }
}

async fn run_once(state: &AppState, notes_dir: &std::path::Path) {
    let conn = match state.db.get() {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(error = %error, "notes mirror watcher: database unavailable");
            return;
        }
    };
    if let Err(error) = super::reconcile(&conn, notes_dir, &state.notes_signatures) {
        tracing::warn!(error = %error, "notes mirror watcher: reconcile failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// Unique scratch directory, removed when dropped.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "maia-notes-watch-{}-{}-{tag}",
                std::process::id(),
                uuid::Uuid::new_v4().simple(),
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn state_for(notes_dir: &std::path::Path, db: crate::db::DbPool) -> AppState {
        let config = Config::from_values(
            serde_json::json!({ "notes_dir": notes_dir.display().to_string() })
                .as_object()
                .unwrap()
                .clone(),
            serde_json::Map::new(),
        );
        AppState::new(config, db)
    }

    /// End-to-end through the live watcher: a file dropped on disk with no
    /// id gets adopted, and an edit to an existing note's file is picked up
    /// without the test needing to call `reconcile` itself.
    ///
    /// Each poll checks a connection out of the pool and drops it again
    /// rather than holding one for the whole loop: `init_memory`'s pool is
    /// sized to exactly one connection, and holding it here would starve
    /// `run_once`'s own `state.db.get()` for its full r2d2 checkout timeout.
    #[tokio::test]
    async fn a_dropped_file_is_adopted_by_the_running_watcher() {
        let tmp = TempDir::new("live-adopt");
        let db = crate::db::init_memory().unwrap();
        let state = state_for(&tmp.0, db);

        spawn(state.clone());
        std::fs::write(
            tmp.0.join("dropped.md"),
            "---\ntitle: \"Dropped In\"\ntags: []\n---\n\nhello from the filesystem\n",
        )
        .unwrap();

        let mut found = false;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let count: i64 = {
                let conn = state.db.get().unwrap();
                conn.query_row("SELECT count(*) FROM notes", [], |r| r.get(0)).unwrap()
            };
            if count == 1 {
                found = true;
                break;
            }
        }
        assert!(found, "watcher did not adopt the dropped file in time");
        let conn = state.db.get().unwrap();
        let title: String = conn.query_row("SELECT title FROM notes", [], |r| r.get(0)).unwrap();
        assert_eq!(title, "Dropped In");
    }
}
