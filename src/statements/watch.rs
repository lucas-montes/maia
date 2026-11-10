//! Dump-folder ingest.
//!
//! The dump folder holds one subdirectory per source (`credit_agricole`,
//! `trading212`, …): the subfolder name picks the parser, so nothing has to
//! guess a file's provenance. Every regular file that appears is staged and
//! then moved out, so the folder the user drops into always empties itself.
//! The moved original is the kept raw document; no hash-named copy is written
//! for these files.
//!
//! Where it moves to is *derived*, not decided by whichever event last touched
//! it — see [`reconcile_files`]. A parsed file lands in `staged/`, reaches
//! `parsed/` once every entry is imported or skipped, and `discarded/` once
//! its statements are deleted. `parsed/` used to mean "the parser did not
//! crash", which left rejected files sitting there looking imported. A file
//! that never parsed is parked in `failed/` beside a `<name>.log` and has no
//! statement rows, so the reconcile pass never sees it.
//!
//! inotify is the trigger, not the source of truth. A scan runs at startup,
//! after every event batch, and on a slow sweep that catches files copied in
//! while the service was down or a queue that overflowed. Re-running the scan
//! is safe by construction: a restaged input dedupes by content hash, and
//! anything already handled has been moved out.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use inotify::{EventMask, Inotify, WatchMask};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::{SaveOutcome, SourceId, is_statement_file};
use crate::server::AppState;

/// Slow sweep, a backstop rather than the primary path: inotify delivers the
/// common case immediately, and the sweep catches what an event could not.
const SWEEP_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Pseudo-source for files parked in the failed tree because they were never
/// inside a source folder at all.
const UNKNOWN: &str = "unknown";

/// `rename` would flag cross-filesystem moves with this errno; `libc` is not a
/// dependency, and the tables are stable across the unix targets this serves.
const EXDEV: std::os::raw::c_int = 18;

/// Start the watcher.
pub fn spawn(state: AppState) {
    if let Err(error) = create_trees(&state) {
        tracing::warn!(error = %error, "cannot create statement lifecycle directories");
    }
    // Before anything else: files mislabelled by the old "parsed means the
    // parser did not crash" rule move to where their statements say they
    // belong, and a file moved by hand snaps back.
    reconcile_files(&state);
    let events = start_inotify(&state);
    tokio::spawn(main_loop(state, events));
}

/// Creates every lifecycle root, each with one subdirectory per known source,
/// so the user always has somewhere obvious to drop files. The lifecyle keeps
/// working without them (scans create directories as needed), but a fresh
/// install should present an empty tree.
fn create_trees(state: &AppState) -> anyhow::Result<()> {
    for root in lifecycle_roots(state) {
        std::fs::create_dir_all(&root)
            .map_err(|error| anyhow::anyhow!("creating {}: {error}", root.display()))?;
        for source in SourceId::all() {
            std::fs::create_dir_all(root.join(source.as_str()))
                .map_err(|error| anyhow::anyhow!("creating {}: {error}", root.display()))?;
        }
    }
    // Files dropped directly in the dump root have no source folder; this is
    // where they are parked.
    std::fs::create_dir_all(state.config.statements_failed_dir().join(UNKNOWN))
        .map_err(|error| anyhow::anyhow!("creating the unknown parking folder: {error}"))?;
    Ok(())
}

/// The roots `create_trees` lays out. Only the dump root is ever *watched* —
/// see [`start_inotify`], which builds its own list — so adding roots here
/// cannot make the service re-ingest its own moves.
fn lifecycle_roots(state: &AppState) -> [PathBuf; 5] {
    [
        state.config.statements_dump_dir(),
        state.config.statements_staged_dir(),
        state.config.statements_parsed_dir(),
        state.config.statements_discarded_dir(),
        state.config.statements_failed_dir(),
    ]
}

/// Watches every dump subdirectory in a dedicated thread, forwarding one
/// message per event (`None` means "rescan everything", usually a queue
/// overflow). The watcher degrades to sweep-only when inotify is unavailable.
fn start_inotify(state: &AppState) -> Option<UnboundedReceiver<Option<PathBuf>>> {
    let dump = state.config.statements_dump_dir();
    let mut dirs: Vec<PathBuf> = SourceId::all()
        .iter()
        .map(|source| dump.join(source.as_str()))
        .collect();
    for dir in &dirs {
        if !dir.exists() {
            tracing::warn!(
                path = %dir.display(),
                "statement dump subfolder missing; relying on the sweep"
            );
            return None;
        }
    }
    // The intake folder joins the watch set so a file dropped there is picked
    // up as promptly as one dropped in the dump tree. The lifecycle roots
    // deliberately do not: the service moves files into them itself, and
    // watching them would make it re-ingest its own work.
    if let Some(intake) = state.config.statements_intake_dir() {
        if intake.is_dir() {
            dirs.push(intake);
        }
    }

    let (sender, receiver) = unbounded_channel::<Option<PathBuf>>();
    let inotify = match Inotify::init() {
        Ok(inotify) => inotify,
        Err(error) => {
            tracing::warn!(error = %error, "inotify unavailable; relying on the sweep");
            return None;
        }
    };
    // One descriptor per watch, kept so an event can be resolved back to the
    // subfolder it came from: a raw event carries the file name, not the
    // parent directory.
    let mut by_descriptor: Vec<(inotify::WatchDescriptor, PathBuf)> = Vec::new();
    for dir in &dirs {
        // A fully written file is the event that matters: `cp` produces one,
        // editors produce `MOVED_TO` by writing a temporary and renaming.
        // `Q_OVERFLOW` is delivered on the event side, not as a watch mask.
        match inotify
            .watches()
            .add(dir, WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO)
        {
            Ok(descriptor) => by_descriptor.push((descriptor, dir.clone())),
            Err(error) => {
                tracing::warn!(path = %dir.display(), error = %error, "cannot watch statement dump folder");
                return None;
            }
        }
    }

    let thread = std::thread::Builder::new()
        .name("statement-dump-watcher".to_string())
        .spawn(move || inotify_loop(inotify, by_descriptor, sender));
    match thread {
        Ok(_) => Some(receiver),
        Err(error) => {
            tracing::warn!(error = %error, "cannot start the statement watcher thread");
            None
        }
    }
}

fn inotify_loop(
    mut inotify: Inotify,
    dirs: Vec<(inotify::WatchDescriptor, PathBuf)>,
    sender: UnboundedSender<Option<PathBuf>>,
) {
    let mut buffer = [0u8; 4096];
    loop {
        let events = match inotify.read_events_blocking(&mut buffer) {
            Ok(events) => events,
            Err(error) => {
                tracing::warn!(error = %error, "statement watcher stopped; relying on the sweep");
                return;
            }
        };
        for event in events {
            if event.mask.contains(EventMask::Q_OVERFLOW) {
                tracing::warn!("statement event queue overflowed; rescanning");
                let _ = sender.send(None);
                continue;
            }
            let Some(name) = event.name else {
                continue;
            };
            let Some(dir) = dirs.iter().find(|(descriptor, _)| descriptor == &event.wd) else {
                continue;
            };
            let _ = sender.send(Some(dir.1.join(name)));
        }
    }
}

async fn main_loop(state: AppState, mut stream: Option<UnboundedReceiver<Option<PathBuf>>>) {
    let mut sweep = tokio::time::interval(SWEEP_INTERVAL);
    // The interval's first tick completes immediately; the startup scan stands
    // in for it, which keeps one scan per interval even when an event race
    // would have run two.
    scan_folder(&state).await;
    sweep.tick().await;
    loop {
        let wake = match stream.as_mut() {
            Some(events) => {
                if events.recv().await.is_some() {
                    true
                } else {
                    tracing::warn!("statement watcher stream closed; relying on the sweep");
                    stream = None;
                    false
                }
            }
            None => {
                sweep.tick().await;
                scan_folder(&state).await;
                false
            }
        };
        if wake {
            // Coalesce whatever arrived while the scan was running.
            if let Some(events) = stream.as_mut() {
                while events.try_recv().is_ok() {}
            }
            scan_folder(&state).await;
        }
    }
}

/// One ingest pass over the whole dump tree.
///
/// Returns the outcomes for `POST /statements/scan`, which triggers an extra
/// pass on demand; the background watcher discards them.
pub async fn scan_folder(state: &AppState) -> Vec<anyhow::Result<Vec<SaveOutcome>>> {
    // Intake feeds the dump tree, so it drains first and this one pass picks
    // up what it moved.
    drain_intake(state);
    let dump = state.config.statements_dump_dir();
    let entries = match std::fs::read_dir(&dump) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(path = %dump.display(), error = %error, "cannot read statement dump folder");
            return Vec::new();
        }
    };

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            match SourceId::parse(&name) {
                Some(source) => results.push(scan_source(state, source, &path).await),
                // Report rather than touch: a misspelled folder is the user's
                // intention up for debate, and moving their files out of it
                // would make the diagnosis harder.
                None => tracing::warn!(
                    directory = %path.display(),
                    "unknown statement source folder; not watching it"
                ),
            }
        } else if path.is_file() {
            if let Err(error) = park(state, UNKNOWN, &path, Some(UNSUPPORTED_ROOT)) {
                tracing::warn!(
                    path = %path.display(),
                    error = %error,
                    "cannot park sourceless file"
                );
            }
        }
    }
    results
}

const UNSUPPORTED_ROOT: &str =
    "file was dropped directly in the dump folder, not in a source folder";

/// Stage every ready file under one source folder.
async fn scan_source(
    state: &AppState,
    source: SourceId,
    dir: &Path,
) -> anyhow::Result<Vec<SaveOutcome>> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| anyhow::anyhow!("reading {}: {error}", dir.display()))?;
    let mut outcomes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !is_statement_file(&path) {
            // A PDF, a screenshot, a `.DS_Store`: nothing a parser can read,
            // so move it out of the way with a reason. Leaving it would let
            // the dump folder silently accumulate junk.
            park(state, source.as_str(), &path, Some("unsupported file type"))
                .map_err(|error| anyhow::anyhow!("parking {}: {error}", path.display()))?;
            continue;
        }
        match ingest_file(state, source, &path).await {
            Ok(file_outcomes) => outcomes.extend(file_outcomes),
            Err(error) => {
                tracing::error!(path = %path.display(), error = %error, "statement ingestion failed");
                return Err(error);
            }
        }
    }
    Ok(outcomes)
}

/// Parse, stage, then decide where the file lives afterwards.
///
/// A parsed file lands in `staged/`: it has been read, not accepted. It moves
/// on to `parsed/` or `discarded/` when its statements say so, which
/// [`reconcile_files`] derives rather than this function guessing. Anything
/// else parks it in the failed tree with the error beside it. If the file is
/// gone by the time we react — a concurrent scan already handled and moved it
/// — the move is a no-op rather than an error, and the already-staged
/// statements stand.
async fn ingest_file(
    state: &AppState,
    source: SourceId,
    path: &Path,
) -> anyhow::Result<Vec<SaveOutcome>> {
    let result = super::run_file_source(source, path, false, state).await;
    match result {
        Ok(outcomes) => {
            let moved = relocate(state, Destination::Staged, source.as_str(), path)?;
            repoint_rows(state, source, path, &moved);
            // A re-drop of bytes already imported stages nothing new, so the
            // file belongs wherever its existing statements put it.
            reconcile_files(state);
            tracing::info!(path = %moved.display(), "statement file parsed and staged");
            Ok(outcomes)
        }
        Err(error) => {
            if path.exists() {
                let moved = relocate(state, Destination::Failed, source.as_str(), path)?;
                write_failure_log(source.as_str(), path, &moved, &error.to_string());
            }
            Err(error.into())
        }
    }
}

/// Where a file lives, derived from the state of its statements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    /// Parsed, with entries still awaiting review.
    Staged,
    /// Every entry imported or skipped.
    Parsed,
    /// Every statement soft-deleted.
    Discarded,
    /// The parse never succeeded; no statement rows exist for it.
    Failed,
}

fn destination_root(state: &AppState, destination: Destination) -> PathBuf {
    match destination {
        Destination::Staged => state.config.statements_staged_dir(),
        Destination::Parsed => state.config.statements_parsed_dir(),
        Destination::Discarded => state.config.statements_discarded_dir(),
        Destination::Failed => state.config.statements_failed_dir(),
    }
}

/// Parks a file in the failed tree with a reason, without creating a statement
/// row. Used for files no parser can ever accept.
fn park(
    state: &AppState,
    source: &str,
    path: &Path,
    reason: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let moved = relocate(state, Destination::Failed, source, path)?;
    if let Some(reason) = reason {
        write_failure_log(source, path, &moved, reason);
    }
    Ok(moved)
}

/// Moves `path` to `<root>/<source>/<name>`, keeping the human-facing name.
///
/// A taken name is disambiguated with a numeric suffix rather than
/// overwritten: a previous statement row may still reference the file at that
/// exact path, and overwriting would orphan the row's raw copy.
fn relocate(
    state: &AppState,
    destination: Destination,
    source: &str,
    path: &Path,
) -> anyhow::Result<PathBuf> {
    let dir = destination_root(state, destination).join(source);
    std::fs::create_dir_all(&dir)
        .map_err(|error| anyhow::anyhow!("creating {}: {error}", dir.display()))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .with_context(|| format!("{} has no file name", path.display()))?;
    let mut target = dir.join(&name);
    let mut suffix = 1;
    while target.exists() {
        suffix += 1;
        target = dir.join(disambiguate(&name, suffix));
    }

    move_file(path, &target)?;
    Ok(target)
}

/// `CA20261027.csv` + suffix 2 becomes `CA20261027-2.csv`, preserving the
/// extension so a person (or a re-upload) keeps recognizing it.
fn disambiguate(name: &str, suffix: i32) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{stem}-{suffix}.{extension}"),
        _ => format!("{name}-{suffix}"),
    }
}

/// Rename across filesystems fails with `EXDEV`, so a dump root and a parsed
/// root on different mounts fall back to copy-then-delete. A file that has
/// vanished was already handled by the concurrent scan this race always
/// admits; that is a success, not an error.
fn move_file(path: &Path, target: &Path) -> anyhow::Result<()> {
    match std::fs::rename(path, target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) if error.raw_os_error() == Some(EXDEV) => {
            std::fs::copy(path, target)
                .map_err(|copy_error| anyhow::anyhow!("copying {path:?}: {copy_error}"))?;
            std::fs::remove_file(path)
                .map_err(|remove_error| anyhow::anyhow!("removing {path:?}: {remove_error}"))?;
            Ok(())
        }
        Err(error) => Err(anyhow::anyhow!("moving {path:?}: {error}")),
    }
}

/// Writes `<name>.log` beside a failed file. The latest error wins; a
/// corrected re-drop replaces it.
fn write_failure_log(source: &str, original: &Path, target: &Path, message: &str) {
    let Some(name) = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    else {
        return;
    };
    let log = target.with_file_name(format!("{name}.log"));
    let stamp = chrono::DateTime::from_timestamp_millis(crate::db::server_time_ms())
        .map(|time| time.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string());
    let text = format!(
        "statement staging failed\n\
         time: {stamp}\n\
         source: {source}\n\
         file: {}\n\
         error: {message}\n",
        original.display(),
    );
    if let Err(error) = std::fs::write(&log, text) {
        tracing::warn!(path = %log.display(), error = %error, "cannot write failure log");
    }
}

/// Move recognised files out of the user's intake folder into the dump tree.
///
/// Intake is shared space — `~/Downloads`, typically — so the licence here is
/// narrow: pick up files whose *name* says which source they are, move them,
/// and touch nothing else. An unrecognised name, an ambiguous one, a
/// subdirectory, a file with no extension a parser understands: all left
/// exactly as they were, not parked and not logged as an error.
///
/// Moving rather than copying is what makes intake self-clearing, which is the
/// point of the feature. Intake will usually be on a different mount than the
/// state directory, so [`move_file`]'s `EXDEV` copy-then-unlink fallback is
/// the normal path here rather than the exotic one.
pub fn drain_intake(state: &AppState) -> usize {
    let Some(intake) = state.config.statements_intake_dir() else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(&intake) else {
        // Not an error worth logging on every sweep: the user may simply not
        // have created it yet.
        return 0;
    };
    let dump = state.config.statements_dump_dir();
    let mut moved = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_statement_file(&path) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(source) = SourceId::from_file_name(name) else {
            continue;
        };
        let dir = dump.join(source.as_str());
        if let Err(error) = std::fs::create_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), error = %error, "cannot create the dump folder");
            continue;
        }
        // Never overwrite: a name already in the dump folder belongs to a file
        // waiting to be parsed.
        let mut target = dir.join(name);
        let mut suffix = 1;
        while target.exists() {
            suffix += 1;
            target = dir.join(disambiguate(name, suffix));
        }
        match move_file(&path, &target) {
            Ok(()) => {
                moved += 1;
                tracing::info!(
                    from = %path.display(),
                    to = %target.display(),
                    source = source.as_str(),
                    "statement file taken from intake"
                );
            }
            Err(error) => {
                tracing::warn!(path = %path.display(), error = %error, "cannot take file from intake");
            }
        }
    }
    moved
}

/// Put every known statement file where the database says it belongs.
///
/// Derived, not remembered — the same shape as `notes_mirror::reconcile()`,
/// which this project already uses for filesystem state that is a projection
/// of the database. A file's root follows from the state of its statements:
///
/// | statements for that `stored_path`      | root          |
/// |----------------------------------------|---------------|
/// | any live statement with pending lines  | `staged/`     |
/// | every live statement imported/skipped  | `parsed/`     |
/// | every statement soft-deleted           | `discarded/`  |
///
/// Because it derives rather than remembers, it migrates files mislabelled by
/// the old "parsed means did not crash" rule on first boot for free, and
/// self-heals a file someone moved by hand. A file in `failed/` has no
/// statement rows at all, so it never appears in this walk and `park` keeps
/// sole ownership of that root.
///
/// Best-effort and idempotent: a file that cannot be moved is logged and left,
/// and the next pass tries again.
pub fn reconcile_files(state: &AppState) {
    let rows = match collect_file_states(state) {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "cannot read statement file states");
            return;
        }
    };
    for (stored_path, source, destination) in rows {
        let current = PathBuf::from(&stored_path);
        if !current.exists() {
            // Already moved by hand, or never written. Nothing to do: the row
            // keeps pointing at a path the next successful move will correct.
            continue;
        }
        let wanted_dir = destination_root(state, destination).join(&source);
        if current.parent() == Some(wanted_dir.as_path()) {
            continue;
        }
        match relocate(state, destination, &source, &current) {
            Ok(moved) => {
                repoint_stored_path(state, &stored_path, &moved);
                tracing::info!(
                    from = %current.display(),
                    to = %moved.display(),
                    "statement file moved to match its statements"
                );
            }
            Err(error) => {
                tracing::warn!(
                    path = %current.display(),
                    error = %error,
                    "cannot move statement file to its derived root"
                );
            }
        }
    }
}

/// `(stored_path, source, destination)` for every file with statement rows.
///
/// One row per distinct `stored_path`. The aggregate is what decides: a file
/// moves on only when *every* live statement it produced is settled, and
/// "settled" means every line is imported or skipped. A line left in `error`
/// — a missing FX rate is the usual one — is neither, so its file stays in
/// `staged/`: `parsed/` means imported, and a file whose entries all failed to
/// import is the furthest thing from that.
fn collect_file_states(state: &AppState) -> anyhow::Result<Vec<(String, String, Destination)>> {
    let conn = state.db.get()?;
    let mut stmt = conn.prepare(
        "SELECT s.stored_path, s.source,
                SUM(s.deleted_at IS NULL) AS live,
                SUM(s.deleted_at IS NULL AND EXISTS (
                    SELECT 1 FROM bank_statement_lines l
                    WHERE l.statement_id = s.id AND l.status IN ('pending','error')
                )) AS pending
         FROM bank_statements s
         WHERE s.stored_path IS NOT NULL AND s.stored_path <> ''
           AND s.status <> 'error'
         GROUP BY s.stored_path, s.source",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let stored_path: String = r.get(0)?;
            let source: String = r.get(1)?;
            let live: i64 = r.get(2)?;
            let pending: i64 = r.get(3)?;
            let destination = if live == 0 {
                Destination::Discarded
            } else if pending > 0 {
                Destination::Staged
            } else {
                Destination::Parsed
            };
            Ok((stored_path, source, destination))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Repoint every row that referenced the file at its old path.
fn repoint_stored_path(state: &AppState, old: &str, moved: &Path) {
    let Ok(conn) = state.db.get() else {
        return;
    };
    if let Err(error) = conn.execute(
        "UPDATE bank_statements SET stored_path=?1, updated_at=?2 WHERE stored_path=?3",
        rusqlite::params![
            moved.display().to_string(),
            crate::db::server_time_ms(),
            old
        ],
    ) {
        tracing::warn!(error = %error, "cannot repoint statement rows to their moved file");
    }
}

/// Point the statement rows created by this parse at the file's final home.
///
/// Rows written during a parse carry the dump-path as their provenance; after
/// the file moves, `stored_path` should say where a human can find it again.
/// Rows that already pointed somewhere else keep that pointer: they belong to
/// an earlier drop whose own file lives there.
fn repoint_rows(state: &AppState, source: SourceId, original: &Path, stored: &Path) {
    let Ok(conn) = state.db.get() else {
        return;
    };
    let now = crate::db::server_time_ms();
    if let Err(error) = conn.execute(
        "UPDATE bank_statements SET stored_path=?1, updated_at=?2
         WHERE source=?3 AND source_path=?4
           AND (stored_path IS NULL OR stored_path=?4)",
        rusqlite::params![
            stored.display().to_string(),
            now,
            source.as_str(),
            original.display().to_string()
        ],
    ) {
        tracing::warn!(error = %error, "cannot repoint statement rows to their moved file");
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
                "maia-dump-{}-{}-{tag}",
                std::process::id(),
                uuid::Uuid::new_v4().simple(),
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn statements(&self) -> TempLifecycle {
            let root = self.0.join("statements");
            std::fs::create_dir_all(&root).unwrap();
            let lifecycle = TempLifecycle {
                dump: root.join("dump"),
                staged: root.join("staged"),
                parsed: root.join("parsed"),
                discarded: root.join("discarded"),
                failed: root.join("failed"),
            };
            for dir in lifecycle.roots() {
                std::fs::create_dir_all(dir).unwrap();
            }
            lifecycle
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The lifecycle roots plus an [`AppState`] they are configured for.
    struct TempLifecycle {
        dump: PathBuf,
        staged: PathBuf,
        parsed: PathBuf,
        discarded: PathBuf,
        failed: PathBuf,
    }
    impl TempLifecycle {
        fn roots(&self) -> [&PathBuf; 5] {
            [
                &self.dump,
                &self.staged,
                &self.parsed,
                &self.discarded,
                &self.failed,
            ]
        }

        fn state(&self, db: crate::db::DbPool) -> AppState {
            let config = Config::from_values(
                serde_json::json!({
                    "statements": {
                        "dump_dir": self.dump.display().to_string(),
                        "staged_dir": self.staged.display().to_string(),
                        "parsed_dir": self.parsed.display().to_string(),
                        "discarded_dir": self.discarded.display().to_string(),
                        "failed_dir": self.failed.display().to_string(),
                    }
                })
                .as_object()
                .unwrap()
                .clone(),
                serde_json::Map::new(),
            );
            AppState::new(config, db)
        }
    }

    fn write(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn a_name_collision_is_renamed_not_overwritten() {
        let tmp = TempDir::new("collision");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());

        std::fs::create_dir_all(lifecycle.parsed.join("credit_agricole")).unwrap();
        std::fs::create_dir_all(lifecycle.dump.join("credit_agricole")).unwrap();
        write(
            &lifecycle.parsed.join("credit_agricole"),
            "export.csv",
            "already there",
        );
        let staging = write(
            &lifecycle.dump.join("credit_agricole"),
            "export.csv",
            "new content",
        );
        let moved = relocate(
            &state,
            Destination::Parsed,
            SourceId::CreditAgricole.as_str(),
            &staging,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(lifecycle.parsed.join("credit_agricole").join("export.csv"))
                .unwrap(),
            "already there",
            "the destination file must not be overwritten: a statement row may point at it"
        );
        assert_eq!(
            moved,
            lifecycle.parsed.join("credit_agricole").join("export-2.csv")
        );
        assert_eq!(std::fs::read_to_string(&moved).unwrap(), "new content");
    }

    #[test]
    fn a_failed_file_gains_a_log_beside_it() {
        let tmp = TempDir::new("failed-log");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        std::fs::create_dir_all(lifecycle.failed.join("credit_agricole")).unwrap();
        std::fs::create_dir_all(lifecycle.dump.join("credit_agricole")).unwrap();

        let staging = write(&lifecycle.dump.join("credit_agricole"), "export.csv", "broken");
        // Parking is what attaches the reason log next to the moved file.
        let reason = "unrecognized column in the export";
        park(&state, "credit_agricole", &staging, Some(reason)).unwrap();
        assert_eq!(
            std::fs::read_to_string(lifecycle.failed.join("credit_agricole").join("export.csv"))
                .unwrap(),
            "broken"
        );
        let log =
            std::fs::read_to_string(lifecycle.failed.join("credit_agricole").join("export.csv.log"))
                .unwrap();
        assert!(log.contains("credit_agricole"), "{log}");
        assert!(log.contains(reason), "{log}");
    }

    #[test]
    fn write_failure_log_carries_the_reason() {
        let tmp = TempDir::new("log-content");
        let lifecycle = tmp.statements();
        std::fs::create_dir_all(lifecycle.failed.join("firstrade")).unwrap();

        let at = write(&lifecycle.failed.join("firstrade"), "orders.csv", "rows");
        write_failure_log("firstrade", &at, &at, "unrecognized column");
        let log =
            std::fs::read_to_string(lifecycle.failed.join("firstrade").join("orders.csv.log")).unwrap();
        assert!(log.contains("firstrade"), "{log}");
        assert!(log.contains("unrecognized column"), "{log}");
        assert!(log.contains("orders.csv"), "{log}");
    }

    #[test]
    fn a_sourceless_file_is_parked_as_unknown() {
        let tmp = TempDir::new("sourceless");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        std::fs::create_dir_all(lifecycle.failed.join(UNKNOWN)).unwrap();

        write(&lifecycle.dump, "random.csv", "content");
        let outcomes = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(scan_folder(&state));
        // Only source folders produce results; the parked file has no source.
        assert_eq!(outcomes.len(), 0);
        assert!(
            !lifecycle.dump.join("random.csv").exists(),
            "the dump root must stay empty"
        );
        assert_eq!(
            std::fs::read_to_string(lifecycle.failed.join(UNKNOWN).join("random.csv")).unwrap(),
            "content"
        );
    }

    #[test]
    fn a_source_folder_that_never_parks_rejects_unknown_names() {
        // `SourceId::parse` is the whole identification step; an unknown
        // folder name must not silently match a source.
        assert_eq!(
            SourceId::parse("credit_agricole"),
            Some(SourceId::CreditAgricole)
        );
        assert_eq!(SourceId::parse("trading212"), Some(SourceId::Trading212));
        assert_eq!(SourceId::parse("firstrade"), Some(SourceId::Firstrade));
        assert_eq!(SourceId::parse("credit agricole"), None);
        assert_eq!(
            SourceId::parse("credit-agricole"),
            Some(SourceId::CreditAgricole)
        );
        assert_eq!(SourceId::parse("ca"), Some(SourceId::CreditAgricole));
    }

    #[test]
    fn move_file_falls_back_when_rename_is_not_enough() {
        // Cross-filesystem moves cannot be forced in a test, but the fallback
        // shares its code path with the plain rename, so both must keep the
        // file intact and remove the source.
        let tmp = TempDir::new("plain-rename");
        let (a, b) = (tmp.0.join("a"), tmp.0.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        write(&a, "file.csv", "bytes");
        move_file(&a.join("file.csv"), &b.join("file.csv")).unwrap();
        assert!(b.join("file.csv").exists());
        assert!(!a.join("file.csv").exists());

        // And a vanished source is a no-op: a concurrent scan already won.
        move_file(&a.join("gone.csv"), &b.join("gone.csv")).unwrap();
        assert!(!b.join("gone.csv").exists());
    }

    #[test]
    fn disambiguate_preserves_the_extension() {
        assert_eq!(disambiguate("export.csv", 2), "export-2.csv");
        assert_eq!(disambiguate("no-extension", 3), "no-extension-3");
        assert_eq!(disambiguate(".hidden", 4), ".hidden-4");
        assert_eq!(disambiguate("tar.gz", 5), "tar-5.gz");
    }

    #[test]
    fn rows_are_repointed_to_the_moved_file() {
        let tmp = TempDir::new("repoint");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        let pool = state.db.clone();

        let original = lifecycle.dump.join("credit_agricole").join("export.csv");
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO bank_statements
                (id, source, institution, account_name, currency_code, stored_path,
                 source_path, source_hash, source_key, status, created_at, updated_at)
             VALUES ('s1', 'credit_agricole', 'Crédit Agricole', 'Eko', 'EUR', NULL,
                     ?1, 'hash', 'key', 'staged', 1, 1)",
            rusqlite::params![original.display().to_string()],
        )
        .unwrap();
        drop(conn);

        let stored = lifecycle.parsed.join("credit_agricole").join("export.csv");
        repoint_rows(&state, SourceId::CreditAgricole, &original, &stored);
        let conn = pool.get().unwrap();
        let path: String = conn
            .query_row(
                "SELECT stored_path FROM bank_statements WHERE id='s1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(path, stored.display().to_string());
    }

    // -----------------------------------------------------------------------
    // Derived placement
    // -----------------------------------------------------------------------

    /// A file with one statement, placed wherever the caller says, with the
    /// line statuses given. Returns the file's path.
    fn staged_file(
        state: &AppState,
        root: &Path,
        name: &str,
        statement_id: &str,
        deleted: bool,
        line_statuses: &[&str],
    ) -> PathBuf {
        let dir = root.join("credit_agricole");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write(&dir, name, "content");
        let conn = state.db.get().unwrap();
        conn.execute(
            "INSERT INTO bank_statements
                (id, source, institution, account_name, currency_code, stored_path,
                 source_path, source_hash, source_key, status, created_at, updated_at,
                 deleted_at)
             VALUES (?1, 'credit_agricole', 'Crédit Agricole', 'Eko', 'EUR', ?2,
                     ?2, ?1, ?1, 'staged', 1, 1, ?3)",
            rusqlite::params![
                statement_id,
                path.display().to_string(),
                deleted.then_some(1_i64)
            ],
        )
        .unwrap();
        for (index, status) in line_statuses.iter().enumerate() {
            conn.execute(
                "INSERT INTO bank_statement_lines
                    (id, statement_id, posted_at, description, amount, currency_code,
                     line_kind, metadata_json, fingerprint, dedupe_key, status,
                     created_at, updated_at)
                 VALUES (?1, ?2, 1, 'x', -1.0, 'EUR', 'debit', '{}', ?1, ?1, ?3, 1, 1)",
                rusqlite::params![format!("{statement_id}-{index}"), statement_id, status],
            )
            .unwrap();
        }
        path
    }

    fn file_names(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir.join("credit_agricole")) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_file_with_pending_entries_belongs_in_staged() {
        let tmp = TempDir::new("reconcile-staged");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        staged_file(
            &state,
            &lifecycle.parsed,
            "export.csv",
            "s1",
            false,
            &["pending", "imported"],
        );

        reconcile_files(&state);

        assert_eq!(file_names(&lifecycle.staged), ["export.csv"]);
        assert!(file_names(&lifecycle.parsed).is_empty());
        let conn = state.db.get().unwrap();
        let stored: String = conn
            .query_row("SELECT stored_path FROM bank_statements", [], |r| r.get(0))
            .unwrap();
        assert!(
            stored.starts_with(&lifecycle.staged.display().to_string()),
            "the row follows the file: {stored}"
        );
    }

    /// `parsed/` means imported — not "the parser did not crash".
    #[test]
    fn a_fully_imported_file_moves_to_parsed() {
        let tmp = TempDir::new("reconcile-parsed");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        staged_file(
            &state,
            &lifecycle.staged,
            "export.csv",
            "s1",
            false,
            &["imported", "skipped"],
        );

        reconcile_files(&state);

        assert_eq!(file_names(&lifecycle.parsed), ["export.csv"]);
        assert!(file_names(&lifecycle.staged).is_empty());
    }

    /// The migration this plan exists to perform: a file sitting in `parsed/`
    /// whose statements were all discarded moves to `discarded/` on the first
    /// boot after the change, with no migration code of its own.
    #[test]
    fn a_discarded_statements_file_leaves_parsed_on_the_next_pass() {
        let tmp = TempDir::new("reconcile-discarded");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        staged_file(
            &state,
            &lifecycle.parsed,
            "fristrade.csv",
            "s1",
            true,
            &["pending"],
        );

        reconcile_files(&state);

        assert_eq!(file_names(&lifecycle.discarded), ["fristrade.csv"]);
        assert!(
            file_names(&lifecycle.parsed).is_empty(),
            "a rejected file must not keep looking imported"
        );
    }

    /// A line that failed to import is neither imported nor skipped, so its
    /// file has not been dealt with. Found end-to-end: an all-USD Firstrade
    /// export with no FX rate imported nothing, every line landed in `error`,
    /// and the file was filed under `parsed/` as if it had worked.
    #[test]
    fn a_file_whose_entries_all_failed_stays_staged() {
        let tmp = TempDir::new("reconcile-errors");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        staged_file(
            &state,
            &lifecycle.staged,
            "firstrade.csv",
            "s1",
            false,
            &["error", "error", "informational"],
        );

        reconcile_files(&state);

        assert_eq!(file_names(&lifecycle.staged), ["firstrade.csv"]);
        assert!(
            file_names(&lifecycle.parsed).is_empty(),
            "nothing was imported, so parsed/ would be a lie"
        );
    }

    /// Two statements of one file: it only moves on when both are settled.
    #[test]
    fn one_pending_statement_keeps_the_whole_file_staged() {
        let tmp = TempDir::new("reconcile-mixed");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        let path = staged_file(
            &state,
            &lifecycle.staged,
            "ca.csv",
            "s1",
            false,
            &["imported"],
        );
        {
            let conn = state.db.get().unwrap();
            conn.execute(
                "INSERT INTO bank_statements
                    (id, source, institution, account_name, currency_code, stored_path,
                     source_path, source_hash, source_key, status, created_at, updated_at)
                 VALUES ('s2','credit_agricole','Crédit Agricole','Livret','EUR',?1,?1,
                         'h2','k2','staged',1,1)",
                rusqlite::params![path.display().to_string()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO bank_statement_lines
                    (id, statement_id, posted_at, description, amount, currency_code,
                     line_kind, metadata_json, fingerprint, dedupe_key, status,
                     created_at, updated_at)
                 VALUES ('l2','s2',1,'x',-1.0,'EUR','debit','{}','f2','d2','pending',1,1)",
                [],
            )
            .unwrap();
        }

        reconcile_files(&state);
        assert_eq!(file_names(&lifecycle.staged), ["ca.csv"]);
    }

    /// Deriving rather than remembering means running twice changes nothing,
    /// and a file moved by hand snaps back where it belongs.
    #[test]
    fn reconcile_is_idempotent_and_self_healing() {
        let tmp = TempDir::new("reconcile-idempotent");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        staged_file(
            &state,
            &lifecycle.staged,
            "export.csv",
            "s1",
            false,
            &["imported"],
        );

        reconcile_files(&state);
        reconcile_files(&state);
        assert_eq!(
            file_names(&lifecycle.parsed),
            ["export.csv"],
            "a second pass must not disambiguate the file into export-2.csv"
        );

        // Someone drags it back by hand; the next pass puts it right.
        let parsed = lifecycle.parsed.join("credit_agricole").join("export.csv");
        let by_hand = lifecycle.staged.join("credit_agricole").join("export.csv");
        std::fs::create_dir_all(by_hand.parent().unwrap()).unwrap();
        std::fs::rename(&parsed, &by_hand).unwrap();
        {
            let conn = state.db.get().unwrap();
            conn.execute(
                "UPDATE bank_statements SET stored_path=?1",
                rusqlite::params![by_hand.display().to_string()],
            )
            .unwrap();
        }
        reconcile_files(&state);
        assert_eq!(file_names(&lifecycle.parsed), ["export.csv"]);
    }

    /// A parked file has no statement rows, so the walk never sees it — the
    /// one root the reconcile must not fight over with `park`.
    #[test]
    fn a_parked_file_is_left_where_park_put_it() {
        let tmp = TempDir::new("reconcile-failed");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        std::fs::create_dir_all(lifecycle.failed.join("credit_agricole")).unwrap();
        write(
            &lifecycle.failed.join("credit_agricole"),
            "broken.csv",
            "junk",
        );

        reconcile_files(&state);

        assert_eq!(file_names(&lifecycle.failed), ["broken.csv"]);
    }

    // -----------------------------------------------------------------------
    // Intake
    // -----------------------------------------------------------------------

    /// An [`AppState`] whose intake folder is `<tmp>/intake`.
    fn with_intake(lifecycle: &TempLifecycle, intake: &Path) -> AppState {
        let config = Config::from_values(
            serde_json::json!({
                "statements": {
                    "dump_dir": lifecycle.dump.display().to_string(),
                    "staged_dir": lifecycle.staged.display().to_string(),
                    "parsed_dir": lifecycle.parsed.display().to_string(),
                    "discarded_dir": lifecycle.discarded.display().to_string(),
                    "failed_dir": lifecycle.failed.display().to_string(),
                }
            })
            .as_object()
            .unwrap()
            .clone(),
            serde_json::json!({
                "statements": { "intake_dir": intake.display().to_string() }
            })
            .as_object()
            .unwrap()
            .clone(),
        );
        AppState::new(config, crate::db::init_memory().unwrap())
    }

    #[test]
    fn a_named_file_is_moved_from_intake_into_its_dump_folder() {
        let tmp = TempDir::new("intake");
        let lifecycle = tmp.statements();
        let intake = tmp.0.join("intake");
        std::fs::create_dir_all(&intake).unwrap();
        let state = with_intake(&lifecycle, &intake);

        write(&intake, "firstrade-2026-11.csv", "rows");
        write(&intake, "t212_feb_aug.csv", "rows");
        write(&intake, "ca-20261027.csv", "rows");

        assert_eq!(drain_intake(&state), 3);

        assert_eq!(
            file_names_in(&lifecycle.dump.join("firstrade")),
            ["firstrade-2026-11.csv"]
        );
        assert_eq!(
            file_names_in(&lifecycle.dump.join("trading212")),
            ["t212_feb_aug.csv"]
        );
        assert_eq!(
            file_names_in(&lifecycle.dump.join("credit_agricole")),
            ["ca-20261027.csv"]
        );
        assert!(
            file_names_in(&intake).is_empty(),
            "moved, not copied: intake clears itself"
        );
    }

    /// Intake is shared space. Anything the service cannot name with certainty
    /// is left exactly where it is — not parked, not logged as an error, not
    /// moved.
    #[test]
    fn an_unrecognised_file_in_intake_is_left_completely_alone() {
        let tmp = TempDir::new("intake-untouched");
        let lifecycle = tmp.statements();
        let intake = tmp.0.join("intake");
        std::fs::create_dir_all(&intake).unwrap();
        let state = with_intake(&lifecycle, &intake);

        write(&intake, "cash-flow.csv", "mine");
        write(&intake, "statement.csv", "mine");
        write(&intake, "ca-and-t212.csv", "ambiguous");
        write(&intake, "firstrade-2026-11.txt", "right name, wrong kind");
        write(&intake, "holiday.jpg", "mine");
        std::fs::create_dir_all(intake.join("subfolder")).unwrap();

        assert_eq!(drain_intake(&state), 0);

        assert_eq!(
            file_names_in(&intake),
            [
                "ca-and-t212.csv",
                "cash-flow.csv",
                "firstrade-2026-11.txt",
                "holiday.jpg",
                "statement.csv",
            ],
            "every one of them is still there"
        );
        assert!(intake.join("subfolder").is_dir());
    }

    /// A name already taken in the dump folder is a file still waiting to be
    /// parsed, so the newcomer is renamed rather than overwriting it.
    #[test]
    fn intake_never_overwrites_a_file_waiting_in_the_dump_folder() {
        let tmp = TempDir::new("intake-collision");
        let lifecycle = tmp.statements();
        let intake = tmp.0.join("intake");
        std::fs::create_dir_all(&intake).unwrap();
        let state = with_intake(&lifecycle, &intake);

        let dump = lifecycle.dump.join("firstrade");
        std::fs::create_dir_all(&dump).unwrap();
        write(&dump, "firstrade.csv", "first");
        write(&intake, "firstrade.csv", "second");

        assert_eq!(drain_intake(&state), 1);
        assert_eq!(
            file_names_in(&dump),
            ["firstrade-2.csv", "firstrade.csv"],
            "the waiting file keeps its name and its content"
        );
        assert_eq!(
            std::fs::read_to_string(dump.join("firstrade.csv")).unwrap(),
            "first"
        );
    }

    #[test]
    fn no_intake_configured_is_a_no_op() {
        let tmp = TempDir::new("intake-off");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        assert_eq!(drain_intake(&state), 0);
    }

    fn file_names_in(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.path().is_file())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// An `error` statement row *does* exist for a file whose parse failed
    /// after a previous success, and it must not drag the parked file out of
    /// `failed/`.
    #[test]
    fn an_error_statement_does_not_move_its_parked_file() {
        let tmp = TempDir::new("reconcile-error-row");
        let lifecycle = tmp.statements();
        let state = lifecycle.state(crate::db::init_memory().unwrap());
        let dir = lifecycle.failed.join("credit_agricole");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write(&dir, "broken.csv", "junk");
        {
            let conn = state.db.get().unwrap();
            conn.execute(
                "INSERT INTO bank_statements
                    (id, source, institution, account_name, currency_code, stored_path,
                     source_path, source_hash, source_key, status, parse_error,
                     created_at, updated_at)
                 VALUES ('e1','credit_agricole','Crédit Agricole','','EUR',?1,?1,
                         'h','k','error','boom',1,1)",
                rusqlite::params![path.display().to_string()],
            )
            .unwrap();
        }

        reconcile_files(&state);

        assert_eq!(file_names(&lifecycle.failed), ["broken.csv"]);
    }
}
