//! Statement ingestion: file watchers, provider feeds, staging and import.
//!
//! The pipeline is deliberately split in two:
//!
//! 1. [`Parser::acquire`] does the I/O: read a file, or call a provider.
//! 2. [`Parser::parse`] is pure: bytes in, a DB-saveable result out.
//!
//! [`ToDatabaseRecord`] then lowers the source-specific result into
//! [`StatementDraft`]s and persists them through [`store`], where duplicate
//! detection, account resolution and transfer matching live once.

pub mod ca;
pub mod draft;
pub mod firstrade;
pub mod store;
pub mod t212;
pub mod watch;

use std::{
    future::Future,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub use draft::{LineKind, SaveOutcome, StatementDraft, StatementLineDraft, SuggestedType};

use crate::server::AppState;

/// Stable namespace for deterministic ids. **Never change these**: changing one
/// would make every re-import create a second account, statement or transaction
/// instead of matching the first.
pub const ACCOUNT_NS: Uuid = Uuid::from_u128(0x0a1c_2e4f_6b8d_4f01_9a3c_5e7f_9b1d_2f40);
pub const STATEMENT_NS: Uuid = Uuid::from_u128(0x1b2d_3f5a_7c9e_4a02_8b4d_6f80_1c3e_5a70);
pub const LINE_NS: Uuid = Uuid::from_u128(0x2c3e_4a6b_8d0f_4b03_9c5e_7a91_2d4f_6b80);
pub const TRANSACTION_NS: Uuid = Uuid::from_u128(0x3d4f_5b7c_9e1a_4c04_8d6f_9b12_3e5a_7c90);
pub const TRANSFER_NS: Uuid = Uuid::from_u128(0x4e5a_6c8d_0f2b_4d05_9e7a_1c23_4f6b_8d01);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceId {
    CreditAgricole,
    Trading212,
    Firstrade,
}

impl SourceId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreditAgricole => "credit_agricole",
            Self::Trading212 => "trading212",
            Self::Firstrade => "firstrade",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "credit_agricole" | "credit-agricole" | "ca" => Some(Self::CreditAgricole),
            "trading212" | "trading_212" | "t212" => Some(Self::Trading212),
            "firstrade" => Some(Self::Firstrade),
            _ => None,
        }
    }

    pub fn institution(self) -> &'static str {
        match self {
            Self::CreditAgricole => "Crédit Agricole",
            Self::Trading212 => "Trading 212",
            Self::Firstrade => "Firstrade",
        }
    }

    pub fn is_feed(self) -> bool {
        matches!(self, Self::Trading212)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    Csv,
    Pdf,
    Json,
}

impl DocumentKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Pdf => "pdf",
            Self::Json => "json",
        }
    }

    pub fn parse_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "csv" => Some(Self::Csv),
            "pdf" => Some(Self::Pdf),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

/// One raw input. A file source returns one; a feed returns one per endpoint.
#[derive(Clone, Debug)]
pub struct RawDocument {
    pub kind: DocumentKind,
    pub bytes: Vec<u8>,
    pub filename: Option<String>,
    pub source_path: Option<PathBuf>,
    pub endpoint: Option<String>,
}

impl RawDocument {
    pub fn file(kind: DocumentKind, bytes: Vec<u8>, path: PathBuf) -> Self {
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        Self {
            kind,
            bytes,
            filename,
            source_path: Some(path),
            endpoint: None,
        }
    }

    pub fn api(kind: DocumentKind, bytes: Vec<u8>, endpoint: impl Into<String>) -> Self {
        Self {
            kind,
            bytes,
            filename: None,
            source_path: None,
            endpoint: Some(endpoint.into()),
        }
    }
}

/// Context computed by [`run`] after acquisition. Parsers copy the hash and
/// stored path into their drafts so a later re-parse can find the original.
#[derive(Clone, Debug)]
pub struct ParseContext {
    pub source_hash: String,
    pub stored_path: Option<String>,
    pub fetched_at: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Malformed(String),
    #[error("unsupported document: {0}")]
    Unsupported(String),
    #[error("HTTP {status} from {url}: {body}")]
    Http {
        status: u16,
        url: String,
        body: String,
    },
    #[error("rate limited by {provider}; retry after {retry_after}s")]
    RateLimited {
        provider: &'static str,
        retry_after: u64,
    },
    #[error("missing configuration: {0}")]
    Config(String),
}

impl SourceError {
    pub fn malformed(message: impl Into<String>) -> Self {
        Self::Malformed(message.into())
    }
}

/// A source of statements. `acquire` is the only I/O; `parse` is pure so the
/// provider quirks can be tested against fixtures without a network or a disk.
pub trait Parser {
    type Output: ToDatabaseRecord;

    fn source(&self) -> SourceId;

    fn acquire(&self) -> impl Future<Output = Result<Vec<RawDocument>, SourceError>> + Send;

    fn parse(&self, docs: &[RawDocument], ctx: &ParseContext) -> Result<Self::Output, SourceError>;
}

/// A parsed result that can be staged. Each source lowers its own result type
/// into the shared [`StatementDraft`] shape.
pub trait ToDatabaseRecord {
    fn into_drafts(self) -> Vec<StatementDraft>;

    fn save(self, tx: &rusqlite::Transaction<'_>) -> anyhow::Result<Vec<SaveOutcome>>
    where
        Self: Sized,
    {
        let drafts = self.into_drafts();
        let mut outcomes = Vec::with_capacity(drafts.len());
        for draft in drafts {
            outcomes.push(store::save_statement(tx, draft)?);
        }
        // Run after every draft in the batch is visible: a CA file's internal
        // transfer pair lives in two sections, so matching per draft would miss
        // it.
        store::match_pending(tx)?;
        Ok(outcomes)
    }
}

impl ToDatabaseRecord for StatementDraft {
    fn into_drafts(self) -> Vec<StatementDraft> {
        vec![self]
    }
}

impl ToDatabaseRecord for Vec<StatementDraft> {
    fn into_drafts(self) -> Vec<StatementDraft> {
        self
    }
}

/// What to ingest. A file is chosen explicitly; a feed has no path.
#[derive(Clone, Debug)]
pub enum SourceSpec {
    File { source: SourceId, path: PathBuf },
    Feed { provider: SourceId },
}

/// Acquire, parse, persist. The whole pipeline for one source run.
pub async fn run<P: Parser>(parser: &P, state: &AppState) -> anyhow::Result<Vec<SaveOutcome>> {
    let docs = parser.acquire().await?;
    run_documents(parser, docs, state).await
}

/// The second half of [`run`], for callers that already have the bytes (an
/// HTTP upload) or that fetched them through a different path.
pub async fn run_documents<P: Parser>(
    parser: &P,
    docs: Vec<RawDocument>,
    state: &AppState,
) -> anyhow::Result<Vec<SaveOutcome>> {
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    let source = parser.source();
    let source_hash = hash_documents(&docs);
    let stored_path = persist_raw(state, source, &source_hash, &docs);
    let ctx = ParseContext {
        source_hash,
        stored_path,
        fetched_at: crate::db::server_time_ms(),
    };

    let parsed = match parser.parse(&docs, &ctx) {
        Ok(parsed) => parsed,
        Err(error) => {
            // Keep the failure visible in the review screen instead of only in
            // the log. A later successful parse of the same bytes updates the
            // same row, so a parser fix is a retry rather than a new record.
            record_failure(state, source, &ctx, &docs, &error)?;
            return Err(error.into());
        }
    };

    let mut conn = state.db.get().context("getting a database connection")?;
    let tx = conn.transaction()?;
    // A previous parse of these exact bytes left an error row behind. Now that
    // they parse, that row is stale: it would otherwise sit in the review list
    // as a permanent failure next to the statements this run stages.
    tx.execute(
        "DELETE FROM bank_statements
         WHERE source=?1 AND source_hash=?2 AND status='error'",
        rusqlite::params![source.as_str(), ctx.source_hash],
    )?;
    let outcomes = parsed.save(&tx)?;
    tx.commit()?;
    Ok(outcomes)
}

fn record_failure(
    state: &AppState,
    source: SourceId,
    ctx: &ParseContext,
    docs: &[RawDocument],
    error: &SourceError,
) -> anyhow::Result<()> {
    let conn = state.db.get()?;
    let now = crate::db::server_time_ms();
    let source_path = docs
        .iter()
        .find_map(|doc| doc.source_path.as_ref())
        .map(|path| path.display().to_string());
    conn.execute(
        "INSERT INTO bank_statements
            (id, source, institution, account_name, account_number, currency_code,
             source_path, stored_path, source_hash, source_key, status, parse_error,
             warnings, created_at, updated_at)
         VALUES (?1, ?2, ?3, '', NULL, '', ?4, ?5, ?6, ?7, 'error', ?8, '[]', ?9, ?9)
         ON CONFLICT(source, source_key) DO UPDATE SET
            status='error', parse_error=excluded.parse_error,
            stored_path=excluded.stored_path, updated_at=excluded.updated_at",
        rusqlite::params![
            stable_uuid(
                STATEMENT_NS,
                &format!("statement:{}:{}", source.as_str(), ctx.source_hash)
            ),
            source.as_str(),
            source.institution(),
            source_path,
            ctx.stored_path,
            ctx.source_hash,
            ctx.source_hash,
            error.to_string(),
            now,
        ],
    )?;
    Ok(())
}

/// Hash over the ordered documents, including their provenance, so the same
/// T212 payload fetched twice is recognised as the same input while a different
/// endpoint or a changed row is not.
///
/// A file's *name* is deliberately not part of the hash: the server stores a
/// copy under `<hash>.csv`, and that copy is re-ingested when the store
/// directory is also watched. Hashing the name would give the copy a new hash
/// every pass, staging a new statement and a new file forever. Content is what
/// identifies a statement; a rename is not a new export.
pub fn hash_documents(docs: &[RawDocument]) -> String {
    let mut hasher = Sha256::new();
    for doc in docs {
        let label = doc.endpoint.as_deref().unwrap_or(doc.kind.extension());
        hasher.update(label.as_bytes());
        hasher.update([0]);
        hasher.update(&doc.bytes);
        hasher.update([0xff]);
    }
    hex::encode(hasher.finalize())
}

/// Best-effort copy of the raw input under `<store_dir>/<source>/<hash>.<ext>`.
///
/// Failing to persist the original must not lose the parsed statement, so this
/// only warns. A single document keeps its real extension; a multi-document
/// feed is written as one JSON array of endpoint/body pairs.
fn persist_raw(
    state: &AppState,
    source: SourceId,
    hash: &str,
    docs: &[RawDocument],
) -> Option<String> {
    let root = state.config.statements_store_dir();
    let dir = root.join(source.as_str());
    if let Err(error) = std::fs::create_dir_all(&dir) {
        tracing::warn!(path = %dir.display(), %error, "cannot create statement store");
        return None;
    }
    // Bank exports are personal documents. The deployment's data directory is
    // group/world readable by default, so tighten what this module creates
    // rather than relying on the umask.
    restrict(&root, 0o700);
    restrict(&dir, 0o700);
    let (extension, bytes) = if docs.len() == 1 {
        (docs[0].kind.extension(), docs[0].bytes.clone())
    } else {
        let bundle: Vec<serde_json::Value> = docs
            .iter()
            .map(|doc| {
                serde_json::json!({
                    "endpoint": doc.endpoint,
                    "filename": doc.filename,
                    "kind": doc.kind.extension(),
                    "body": String::from_utf8_lossy(&doc.bytes),
                })
            })
            .collect();
        (
            "json",
            serde_json::to_vec_pretty(&bundle).unwrap_or_default(),
        )
    };
    let path = dir.join(format!("{hash}.{extension}"));
    match std::fs::write(&path, bytes) {
        Ok(()) => {
            restrict(&path, 0o600);
            Some(path.display().to_string())
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "cannot store statement source");
            None
        }
    }
}

/// Best-effort `chmod`. A failure is logged rather than fatal: losing the
/// original file would be worse than a permissive one, and the caller's
/// statement still saves.
#[cfg(unix)]
fn restrict(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(error) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)) {
        tracing::warn!(path = %path.display(), %error, "cannot restrict statement permissions");
    }
}

#[cfg(not(unix))]
fn restrict(_path: &Path, _mode: u32) {}

/// SHA-256, hex encoded. Used for source and line identities.
pub fn sha256_hex(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

pub fn stable_uuid(namespace: Uuid, key: &str) -> String {
    Uuid::new_v5(&namespace, key.as_bytes()).to_string()
}

/// Dispatch a spec to the concrete source. Kept as an enum rather than
/// `dyn Parser` because the trait's futures are not object safe without boxing,
/// and there are only a handful of sources.
pub async fn run_spec(spec: &SourceSpec, state: &AppState) -> anyhow::Result<Vec<SaveOutcome>> {
    match spec {
        SourceSpec::File { source, path } => match source {
            SourceId::CreditAgricole => {
                run(&ca::CreditAgricoleSource::new(path.clone()), state).await
            }
            SourceId::Firstrade => run(&firstrade::FirstradeSource::new(path.clone()), state).await,
            SourceId::Trading212 => {
                anyhow::bail!("trading212 is an API feed; use SourceSpec::Feed")
            }
        },
        SourceSpec::Feed { provider } => match provider {
            SourceId::Trading212 => {
                let source = t212::Trading212Source::from_config(&state.config, &state.http)?;
                run(&source, state).await
            }
            other => anyhow::bail!("{} is not an API feed", other.as_str()),
        },
    }
}

/// Ingest bytes that did not come from a configured path, e.g. a desktop
/// upload. The source still owns parsing; only `acquire` is skipped.
pub async fn run_upload(
    source: SourceId,
    docs: Vec<RawDocument>,
    state: &AppState,
) -> anyhow::Result<Vec<SaveOutcome>> {
    match source {
        SourceId::CreditAgricole => {
            run_documents(
                &ca::CreditAgricoleSource::new(PathBuf::from("upload")),
                docs,
                state,
            )
            .await
        }
        SourceId::Firstrade => {
            run_documents(
                &firstrade::FirstradeSource::new(PathBuf::from("upload")),
                docs,
                state,
            )
            .await
        }
        SourceId::Trading212 => {
            anyhow::bail!("Trading 212 is an API feed; sync it instead of uploading a file")
        }
    }
}

/// True when [path] has an extension a file source understands.
pub fn is_statement_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(DocumentKind::parse_extension)
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rename_does_not_change_the_source_hash() {
        let one = RawDocument::file(
            DocumentKind::Csv,
            b"date;amount\n".to_vec(),
            PathBuf::from("/tmp/one.csv"),
        );
        let two = RawDocument::file(
            DocumentKind::Csv,
            b"date;amount\n".to_vec(),
            PathBuf::from("/tmp/two.csv"),
        );
        assert_eq!(hash_documents(&[one]), hash_documents(&[two]));
    }

    #[test]
    fn feed_endpoints_are_part_of_the_hash() {
        let summary = RawDocument::api(DocumentKind::Json, b"{}".to_vec(), "/api/v0/summary");
        let orders = RawDocument::api(DocumentKind::Json, b"{}".to_vec(), "/api/v0/orders");
        assert_ne!(hash_documents(&[summary]), hash_documents(&[orders]));
    }
}
