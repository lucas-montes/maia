//! Markdown mirror for `notes`.
//!
//! SQLite stays the source of truth; every non-deleted note also exists as a
//! `.md` file under [`crate::config::Config::notes_dir`] so it can be read and
//! edited with any external editor. [`reconcile`] is the entry point both the
//! startup scan ([`crate::server::start_server_with_shutdown`]) and the live
//! [`watch`] module drive: it walks the directory once, diffs it against the
//! DB, and leaves both sides consistent. Every branch is idempotent, so
//! running it again with nothing changed is a no-op.
//!
//! File identity is carried in YAML frontmatter (`id`), not the filename: the
//! filename (`<date>-<slug>-<id-prefix>.md`) is cosmetic and gets regenerated
//! whenever the title changes — including reverting a manual rename on disk,
//! since the mirror is a derived projection of the DB, not the other way
//! round. Timestamps in frontmatter are epoch milliseconds, matching every
//! other timestamp in this codebase, rather than an ISO string that would
//! need its own parser.
//!
//! [`SignatureMap`] is what tells a reconcile pass apart from the watcher
//! re-noticing its own last write: every export records the content hash of
//! what it wrote, keyed by note id, and a file whose current hash still
//! matches is left alone even if its mtime looks "newer" than the DB's
//! `updated_at` — mtime alone is not trustworthy once a filesystem's
//! timestamp granularity is coarser than two edits landing in the same
//! window.

pub mod watch;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};

/// A note as read from the DB, tags already joined in.
pub struct NoteRecord {
    pub id: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

/// A note as read back out of a `.md` file. `id` is `None` for a file with no
/// frontmatter at all, or frontmatter missing the field — both are treated the
/// same way by [`reconcile`]: adopted as a new note.
pub struct ParsedNote {
    pub id: Option<String>,
    pub title: String,
    pub tags: Vec<String>,
    pub created_at: Option<i64>,
    pub body: String,
}

/// What a note's file looked like the last time we wrote it: its path (so a
/// delete event can be resolved back to a note without re-parsing a file
/// that is already gone) and a hash of its exact bytes (so a reconcile pass
/// can tell "nothing changed" apart from "changed but mtime didn't move").
#[derive(Clone)]
pub struct Signature {
    pub path: PathBuf,
    pub hash: String,
}

/// Shared across the startup reconcile and the live watcher — both read and
/// write the same map, so whichever one last touched a file is the one the
/// other trusts.
pub type SignatureMap = Arc<Mutex<HashMap<String, Signature>>>;

pub fn new_signature_map() -> SignatureMap {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Runs the full export/import pass: imports files changed since their note's
/// `updated_at` (or whose content no longer matches what [signatures] last
/// recorded for them), exports notes whose file is missing or stale, adopts
/// files with no recognized id as new notes, and deletes the file for any
/// note that is tombstoned in the DB.
pub fn reconcile(conn: &Connection, notes_dir: &Path, signatures: &SignatureMap) -> anyhow::Result<()> {
    std::fs::create_dir_all(notes_dir)?;
    let scanned = scan_dir(notes_dir)?;
    let mut by_id: HashMap<String, ScannedFile> = HashMap::new();
    let mut orphans: Vec<ScannedFile> = Vec::new();
    for file in scanned {
        match file.parsed.id.clone() {
            Some(id) => {
                by_id.insert(id, file);
            }
            None => orphans.push(file),
        }
    }

    let notes = load_notes(conn, true)?;
    let mut seen: HashSet<String> = HashSet::new();
    for note in &notes {
        seen.insert(note.id.clone());
        if note.deleted_at.is_some() {
            // DB tombstones remove the file and are not recreated: a delete
            // made it through to the DB, so the mirror must agree.
            if let Some(file) = by_id.remove(&note.id) {
                let _ = std::fs::remove_file(&file.path);
            }
            forget_signature(signatures, &note.id);
            continue;
        }
        match by_id.remove(&note.id) {
            Some(file) => {
                let changed = file.mtime_ms > note.updated_at || hash_differs(signatures, &note.id, &file.path);
                if changed {
                    import_file(conn, &note.id, &file)?;
                    if let Some(refreshed) = load_note(conn, &note.id)? {
                        let path = export_note(notes_dir, Some(&file.path), &refreshed)?;
                        record_signature(signatures, &note.id, &path);
                    }
                } else {
                    let path = export_note(notes_dir, Some(&file.path), note)?;
                    record_signature(signatures, &note.id, &path);
                }
            }
            None => {
                let path = export_note(notes_dir, None, note)?;
                record_signature(signatures, &note.id, &path);
            }
        }
    }

    // Whatever is left in `by_id` carries an id this DB has never issued —
    // stale, foreign, or simply wrong. Treat it the same as a file with no id
    // at all: adopt it as a brand-new note.
    for (_, file) in by_id {
        adopt_file(conn, notes_dir, &file, signatures)?;
    }
    for file in orphans {
        adopt_file(conn, notes_dir, &file, signatures)?;
    }
    Ok(())
}

/// Writes (or rewrites) [note]'s file, renaming away from [existing_path] when
/// the canonical name has changed (title edit, or a freshly adopted id).
///
/// The file's mtime is stamped to `note.updated_at` rather than left at
/// "whenever this write happened". That is the first line of defense against
/// mistaking our own write for an external edit — [`SignatureMap`] is the
/// second, for when mtime granularity is too coarse to tell two writes apart.
pub fn export_note(
    notes_dir: &Path,
    existing_path: Option<&Path>,
    note: &NoteRecord,
) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(notes_dir)?;
    let canonical = notes_dir.join(filename_for(&note.id, &note.title, note.created_at));
    std::fs::write(&canonical, render_markdown(note))?;
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&canonical) {
        let stamp = UNIX_EPOCH + Duration::from_millis(note.updated_at.max(0) as u64);
        let _ = file.set_modified(stamp);
    }
    if let Some(old) = existing_path {
        if old != canonical && old.exists() {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(canonical)
}

// ---------------------------------------------------------------------------
// Signature map
// ---------------------------------------------------------------------------

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// True when [path]'s current content no longer matches the hash recorded
/// for [note_id] — including when nothing was ever recorded, which errs
/// toward re-importing (idempotent, since the content already matches the
/// DB in that case) rather than silently sitting on a real edit.
fn hash_differs(signatures: &SignatureMap, note_id: &str, path: &Path) -> bool {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return true,
    };
    let hash = hash_bytes(&bytes);
    match signatures.lock().unwrap().get(note_id) {
        Some(sig) => sig.hash != hash,
        None => true,
    }
}

fn record_signature(signatures: &SignatureMap, note_id: &str, path: &Path) {
    if let Ok(bytes) = std::fs::read(path) {
        let hash = hash_bytes(&bytes);
        signatures.lock().unwrap().insert(
            note_id.to_string(),
            Signature {
                path: path.to_path_buf(),
                hash,
            },
        );
    }
}

fn forget_signature(signatures: &SignatureMap, note_id: &str) {
    signatures.lock().unwrap().remove(note_id);
}

// ---------------------------------------------------------------------------
// DB access
// ---------------------------------------------------------------------------

fn load_note_tags(conn: &Connection, note_id: &str) -> Vec<String> {
    conn.prepare(
        "SELECT t.name FROM tags t JOIN note_tags nt ON nt.tag_id = t.id \
         WHERE nt.note_id = ?1 ORDER BY t.name",
    )
    .and_then(|mut stmt| {
        let rows = stmt.query_map(rusqlite::params![note_id], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .unwrap_or_default()
}

/// Loads every note, deleted ones included when [include_deleted] so the
/// caller can tell a tombstone apart from an id it has simply never seen.
fn load_notes(conn: &Connection, include_deleted: bool) -> anyhow::Result<Vec<NoteRecord>> {
    let sql = if include_deleted {
        "SELECT id, title, body, created_at, updated_at, deleted_at FROM notes"
    } else {
        "SELECT id, title, body, created_at, updated_at, deleted_at FROM notes WHERE deleted_at IS NULL"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows: Vec<(String, String, String, i64, i64, Option<i64>)> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows
        .into_iter()
        .map(|(id, title, body, created_at, updated_at, deleted_at)| {
            let tags = load_note_tags(conn, &id);
            NoteRecord {
                id,
                title,
                body,
                tags,
                created_at,
                updated_at,
                deleted_at,
            }
        })
        .collect())
}

fn load_note(conn: &Connection, id: &str) -> anyhow::Result<Option<NoteRecord>> {
    let row = conn
        .query_row(
            "SELECT id, title, body, created_at, updated_at, deleted_at FROM notes WHERE id = ?1",
            rusqlite::params![id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                ))
            },
        )
        .optional()?;
    Ok(row.map(
        |(id, title, body, created_at, updated_at, deleted_at)| {
            let tags = load_note_tags(conn, &id);
            NoteRecord {
                id,
                title,
                body,
                tags,
                created_at,
                updated_at,
                deleted_at,
            }
        },
    ))
}

/// Updates an existing note's title/body/tags from a changed file, bumping
/// `updated_at` to the file's mtime rather than a fresh server timestamp —
/// that is the import side of last-write-wins.
fn import_file(conn: &Connection, note_id: &str, file: &ScannedFile) -> anyhow::Result<()> {
    let title = non_empty_title(&file.parsed.title);
    conn.execute(
        "UPDATE notes SET title = ?1, body = ?2, updated_at = ?3 WHERE id = ?4",
        rusqlite::params![title, file.parsed.body, file.mtime_ms, note_id],
    )?;
    upsert_tags(conn, note_id, &file.parsed.tags, file.mtime_ms)?;
    Ok(())
}

/// Inserts a file with no recognized id as a new note (fresh UUIDv7), then
/// renames/rewrites the file in place so it carries that id from now on.
fn adopt_file(
    conn: &Connection,
    notes_dir: &Path,
    file: &ScannedFile,
    signatures: &SignatureMap,
) -> anyhow::Result<()> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = crate::db::server_time_ms();
    let title = non_empty_title(&file.parsed.title);
    let created_at = file.parsed.created_at.unwrap_or(file.mtime_ms.min(now));
    let updated_at = file.mtime_ms.max(created_at);
    conn.execute(
        "INSERT INTO notes (id, title, body, created_at, updated_at, deleted_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
        rusqlite::params![id, title, file.parsed.body, created_at, updated_at],
    )?;
    upsert_tags(conn, &id, &file.parsed.tags, updated_at)?;
    if let Some(record) = load_note(conn, &id)? {
        let path = export_note(notes_dir, Some(&file.path), &record)?;
        record_signature(signatures, &id, &path);
    }
    Ok(())
}

fn non_empty_title(title: &str) -> &str {
    if title.trim().is_empty() {
        "Untitled"
    } else {
        title
    }
}

/// Replaces a note's tag links wholesale from a list of names — the same
/// full-state semantics `push_notes` uses, since the file is the complete set
/// of tags for that note, not a diff.
fn upsert_tags(conn: &Connection, note_id: &str, tag_names: &[String], now: i64) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM note_tags WHERE note_id = ?1",
        rusqlite::params![note_id],
    )?;
    for name in tag_names {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let tag_id: String = conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1",
                rusqlite::params![name],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        conn.execute(
            "INSERT INTO tags (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3) \
             ON CONFLICT(id) DO NOTHING",
            rusqlite::params![tag_id, name, now],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO note_tags (tag_id, note_id) VALUES (?1, ?2)",
            rusqlite::params![tag_id, note_id],
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Filesystem scan
// ---------------------------------------------------------------------------

struct ScannedFile {
    path: PathBuf,
    parsed: ParsedNote,
    mtime_ms: i64,
}

fn scan_dir(notes_dir: &Path) -> anyhow::Result<Vec<ScannedFile>> {
    if !notes_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(notes_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        let parsed = parse_markdown(&content);
        let mtime_ms = entry
            .metadata()?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        out.push(ScannedFile {
            path,
            parsed,
            mtime_ms,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Frontmatter: minimal hand-rolled format, not a general YAML parser.
// ---------------------------------------------------------------------------

/// `<ISO-date>-<title-slug>-<id-prefix(8)>.md`. The date is the note's
/// creation date, not its last edit, so a note's file does not jump around
/// a date-sorted directory listing every time it is touched.
fn filename_for(id: &str, title: &str, created_at_ms: i64) -> String {
    let date = chrono::DateTime::from_timestamp_millis(created_at_ms)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp_millis(0).unwrap())
        .format("%Y-%m-%d");
    let slug = slugify(title);
    let short = id.get(..8).unwrap_or(id);
    format!("{date}-{slug}-{short}.md")
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = true;
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                slug.push(lower);
            }
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug = "note".to_string();
    }
    if slug.chars().count() > 60 {
        slug = slug.chars().take(60).collect();
        while slug.ends_with('-') {
            slug.pop();
        }
    }
    slug
}

fn render_markdown(note: &NoteRecord) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("id: {}\n", note.id));
    out.push_str(&format!("title: {}\n", yaml_quote(&note.title)));
    out.push_str(&format!("created_at: {}\n", note.created_at));
    out.push_str(&format!("updated_at: {}\n", note.updated_at));
    let tags = note
        .tags
        .iter()
        .map(|t| yaml_quote(t))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!("tags: [{tags}]\n"));
    out.push_str("---\n\n");
    out.push_str(&note.body);
    if !note.body.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn parse_markdown(content: &str) -> ParsedNote {
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return ParsedNote {
            id: None,
            title: String::new(),
            tags: Vec::new(),
            created_at: None,
            body: content.to_string(),
        };
    }

    let mut id = None;
    let mut title = String::new();
    let mut tags = Vec::new();
    let mut created_at = None;
    let mut body_lines: Vec<&str> = Vec::new();
    let mut in_frontmatter = true;

    for line in lines {
        if in_frontmatter {
            if line.trim() == "---" {
                in_frontmatter = false;
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim();
                match key.trim() {
                    "id" => id = Some(value.trim_matches('"').to_string()),
                    "title" => title = yaml_unquote(value),
                    "created_at" => created_at = value.parse().ok(),
                    "tags" => tags = parse_tag_list(value),
                    _ => {}
                }
            }
        } else {
            body_lines.push(line);
        }
    }
    // The blank line `render_markdown` inserts to separate frontmatter from
    // body round-trips back out as an empty first element here.
    if body_lines.first() == Some(&"") {
        body_lines.remove(0);
    }
    // `render_markdown` always ends the body in `\n`; `Lines` strips every
    // terminator, including the last one, so it has to be put back here for
    // the round trip to be exact.
    let body = format!("{}\n", body_lines.join("\n"));
    ParsedNote {
        id,
        title,
        tags,
        created_at,
        body,
    }
}

fn parse_tag_list(value: &str) -> Vec<String> {
    let inner = value.trim();
    let inner = inner.strip_prefix('[').unwrap_or(inner);
    let inner = inner.strip_suffix(']').unwrap_or(inner);
    if inner.trim().is_empty() {
        return Vec::new();
    }
    split_unquoted_commas(inner)
        .into_iter()
        .map(|part| yaml_unquote(part.trim()))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Splits on `,` outside of double quotes, so a quoted tag name may itself
/// contain a comma (`"two, with comma"`) without being cut in half.
fn split_unquoted_commas(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
        } else if b == b'\\' {
            escaped = true;
        } else if b == b'"' {
            in_quotes = !in_quotes;
        } else if b == b',' && !in_quotes {
            parts.push(&s[start..i]);
            start = i + 1;
        }
    }
    parts.push(&s[start..]);
    parts
}

fn yaml_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn yaml_unquote(s: &str) -> String {
    let s = s.trim();
    let inner = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique scratch directory, removed when dropped.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "maia-notes-mirror-{}-{}-{tag}",
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

    fn insert_note(conn: &Connection, id: &str, title: &str, body: &str, created_at: i64, updated_at: i64) {
        conn.execute(
            "INSERT INTO notes (id, title, body, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, title, body, created_at, updated_at],
        )
        .unwrap();
    }

    fn tag_names_for(conn: &Connection, note_id: &str) -> Vec<String> {
        let mut names = load_note_tags(conn, note_id);
        names.sort();
        names
    }

    fn one_md_file(dir: &Path) -> PathBuf {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect();
        assert_eq!(files.len(), 1, "expected exactly one .md file in {dir:?}");
        files.pop().unwrap()
    }

    #[test]
    fn export_round_trip_is_idempotent() {
        let tmp = TempDir::new("round-trip");
        let pool = crate::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        let signatures = new_signature_map();
        insert_note(&conn, "11111111-1111-1111-1111-111111111111", "Hello, World!", "first line\nsecond line\n", 1_000, 2_000);
        upsert_tags(&conn, "11111111-1111-1111-1111-111111111111", &["work".to_string(), "idea".to_string()], 2_000).unwrap();

        reconcile(&conn, &tmp.0, &signatures).unwrap();
        let path = one_md_file(&tmp.0);
        assert!(path.file_name().unwrap().to_str().unwrap().contains("hello-world"));
        let first_pass = std::fs::read_to_string(&path).unwrap();

        // Re-running reconcile with nothing changed must not touch the file:
        // same content, same path.
        reconcile(&conn, &tmp.0, &signatures).unwrap();
        let second_pass = std::fs::read_to_string(&path).unwrap();
        assert_eq!(first_pass, second_pass);

        let parsed = parse_markdown(&first_pass);
        assert_eq!(parsed.id, Some("11111111-1111-1111-1111-111111111111".to_string()));
        assert_eq!(parsed.title, "Hello, World!");
        assert_eq!(parsed.body, "first line\nsecond line\n");
        let mut tags = parsed.tags.clone();
        tags.sort();
        assert_eq!(tags, vec!["idea".to_string(), "work".to_string()]);
    }

    /// Reconcile without a pre-populated signature map (the state right after
    /// process start) must not treat its own earlier export as a changed
    /// file just because the map is empty — `None` only forces a re-import,
    /// never a change the DB didn't ask for.
    #[test]
    fn a_fresh_signature_map_does_not_cause_spurious_reimport() {
        let tmp = TempDir::new("fresh-map");
        let pool = crate::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        insert_note(&conn, "55555555-5555-5555-5555-555555555555", "Stable Title", "stable body\n", 1_000, 1_000);

        reconcile(&conn, &tmp.0, &new_signature_map()).unwrap();
        let (title, body, updated_at): (String, String, i64) = conn
            .query_row(
                "SELECT title, body, updated_at FROM notes WHERE id = '55555555-5555-5555-5555-555555555555'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(title, "Stable Title");
        assert_eq!(body, "stable body\n");
        assert_eq!(updated_at, 1_000, "an unrelated restart must not bump updated_at");
    }

    #[test]
    fn frontmatter_parses_quoted_title_and_tag_list() {
        let content = "---\nid: abc-123\ntitle: \"Colon: still works\"\ncreated_at: 500\ntags: [\"one\", \"two, with comma\"]\n---\n\nbody text\n";
        let parsed = parse_markdown(content);
        assert_eq!(parsed.id, Some("abc-123".to_string()));
        assert_eq!(parsed.title, "Colon: still works");
        assert_eq!(parsed.created_at, Some(500));
        assert_eq!(parsed.tags, vec!["one".to_string(), "two, with comma".to_string()]);
        assert_eq!(parsed.body, "body text\n");
    }

    #[test]
    fn a_file_with_no_recognizable_id_is_adopted_as_a_new_note() {
        let tmp = TempDir::new("adopt");
        let pool = crate::db::init_memory().unwrap();
        let conn = pool.get().unwrap();

        std::fs::write(
            tmp.0.join("scratch.md"),
            "---\ntitle: \"From the editor\"\ntags: [draft]\n---\n\nwritten outside the app\n",
        )
        .unwrap();

        reconcile(&conn, &tmp.0, &new_signature_map()).unwrap();

        let count: i64 = conn.query_row("SELECT count(*) FROM notes", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
        let (id, title, body): (String, String, String) = conn
            .query_row("SELECT id, title, body FROM notes", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!(title, "From the editor");
        assert_eq!(body, "written outside the app\n");
        assert_eq!(tag_names_for(&conn, &id), vec!["draft".to_string()]);

        // The old filename is gone; the note now owns a canonical one.
        let path = one_md_file(&tmp.0);
        let rewritten = std::fs::read_to_string(&path).unwrap();
        assert!(rewritten.contains(&format!("id: {id}")));
    }

    #[test]
    fn a_tombstoned_note_has_no_file_recreated() {
        let tmp = TempDir::new("tombstone");
        let pool = crate::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        let signatures = new_signature_map();
        insert_note(&conn, "22222222-2222-2222-2222-222222222222", "Gone Soon", "body\n", 1_000, 1_000);
        reconcile(&conn, &tmp.0, &signatures).unwrap();
        assert_eq!(std::fs::read_dir(&tmp.0).unwrap().count(), 1);

        conn.execute(
            "UPDATE notes SET deleted_at = 5000 WHERE id = '22222222-2222-2222-2222-222222222222'",
            [],
        )
        .unwrap();
        reconcile(&conn, &tmp.0, &signatures).unwrap();
        assert_eq!(std::fs::read_dir(&tmp.0).unwrap().count(), 0, "tombstoned note's file must not be recreated");
        assert!(signatures.lock().unwrap().get("22222222-2222-2222-2222-222222222222").is_none());
    }

    #[test]
    fn tag_upsert_reuses_an_existing_tag_by_name() {
        let pool = crate::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO tags (id, name, created_at, updated_at) VALUES ('tag-1', 'work', 100, 100)",
            [],
        )
        .unwrap();
        insert_note(&conn, "33333333-3333-3333-3333-333333333333", "Note", "body\n", 100, 100);

        upsert_tags(&conn, "33333333-3333-3333-3333-333333333333", &["work".to_string()], 200).unwrap();

        let tag_count: i64 = conn.query_row("SELECT count(*) FROM tags WHERE name = 'work'", [], |r| r.get(0)).unwrap();
        assert_eq!(tag_count, 1, "must not create a duplicate tag for an existing name");
        let linked_id: String = conn
            .query_row("SELECT tag_id FROM note_tags WHERE note_id = '33333333-3333-3333-3333-333333333333'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(linked_id, "tag-1");
    }

    #[test]
    fn an_edited_title_renames_the_file_in_place() {
        let tmp = TempDir::new("rename");
        let pool = crate::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        let signatures = new_signature_map();
        insert_note(&conn, "44444444-4444-4444-4444-444444444444", "Original Title", "body\n", 1_000, 1_000);
        reconcile(&conn, &tmp.0, &signatures).unwrap();
        let old_path = one_md_file(&tmp.0);

        conn.execute(
            "UPDATE notes SET title = 'Renamed Title', updated_at = 2000 WHERE id = '44444444-4444-4444-4444-444444444444'",
            [],
        )
        .unwrap();
        reconcile(&conn, &tmp.0, &signatures).unwrap();

        let new_path = one_md_file(&tmp.0);
        assert_ne!(old_path, new_path);
        assert!(!old_path.exists());
        assert!(new_path.file_name().unwrap().to_str().unwrap().contains("renamed-title"));
        assert_eq!(signatures.lock().unwrap().get("44444444-4444-4444-4444-444444444444").unwrap().path, new_path);
    }

    /// A pure filesystem rename (same bytes, new path — e.g. the user
    /// reorganizing files, or an editor's atomic-save temp-then-rename) must
    /// not be mistaken for a delete-then-create: the note keeps its single DB
    /// row, and only its signature's tracked path moves.
    #[test]
    fn a_manual_file_rename_is_not_treated_as_delete_and_recreate() {
        let tmp = TempDir::new("manual-rename");
        let pool = crate::db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        let signatures = new_signature_map();
        insert_note(&conn, "66666666-6666-6666-6666-666666666666", "Keep Me", "same body\n", 1_000, 1_000);
        reconcile(&conn, &tmp.0, &signatures).unwrap();
        let exported_path = one_md_file(&tmp.0);

        // A real `rename(2)` — unlike delete-then-recreate — leaves the
        // file's mtime untouched, exactly like a user's `mv` or an editor's
        // atomic save. That is what actually exercises the rename-detection
        // path rather than the ordinary "content changed" one.
        let renamed_path = tmp.0.join("my-own-name.md");
        std::fs::rename(&exported_path, &renamed_path).unwrap();

        reconcile(&conn, &tmp.0, &signatures).unwrap();

        let count: i64 = conn.query_row("SELECT count(*) FROM notes", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "a rename must not create a second note");
        let (title, updated_at): (String, i64) = conn
            .query_row(
                "SELECT title, updated_at FROM notes WHERE id = '66666666-6666-6666-6666-666666666666'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(title, "Keep Me");
        assert_eq!(updated_at, 1_000, "unchanged content must not bump updated_at");
    }
}
