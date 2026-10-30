//! HTTP surface for statement staging, import and feeds.

use std::collections::HashSet;

use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::{
    config, db,
    error::ErrorBody,
    server::AppState,
    statements::{
        self, DocumentKind, RawDocument, SourceId, SourceSpec,
        store::{self, ImportOutcome, ImportRequest},
    },
};

type ApiError = (StatusCode, Json<ErrorBody>);

fn error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(ErrorBody {
            message: message.into(),
        }),
    )
}

// ---------------------------------------------------------------------------
// Staged statements
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn list_statements(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state
        .db
        .get()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.source, s.institution, s.account_name, s.account_number,
                    s.account_id, s.currency_code, s.period_start, s.period_end,
                    s.opening_balance, s.closing_balance, s.status, s.warnings,
                    s.parse_error, s.created_at, s.updated_at,
                    (SELECT COUNT(*) FROM bank_statement_lines l WHERE l.statement_id=s.id),
                    (SELECT COUNT(*) FROM bank_statement_lines l WHERE l.statement_id=s.id AND l.status='pending'),
                    (SELECT COUNT(*) FROM bank_statement_lines l WHERE l.statement_id=s.id AND l.status='imported'),
                    a.name
             FROM bank_statements s
             LEFT JOIN accounts a ON a.id = s.account_id
             WHERE s.deleted_at IS NULL
               AND (?1 IS NULL OR s.status = ?1)
               AND (?2 IS NULL OR s.source = ?2)
             ORDER BY s.updated_at DESC
             LIMIT ?3",
        )
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items = stmt
        .query_map(
            rusqlite::params![
                query.status,
                query.source,
                query.limit.unwrap_or(200).clamp(1, 500)
            ],
            |row| {
                let warnings: Option<String> = row.get(12)?;
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "source": row.get::<_, String>(1)?,
                    "institution": row.get::<_, String>(2)?,
                    "accountName": row.get::<_, String>(3)?,
                    "accountNumber": row.get::<_, Option<String>>(4)?,
                    "accountId": row.get::<_, Option<String>>(5)?,
                    "resolvedAccountName": row.get::<_, Option<String>>(19)?,
                    "currencyCode": row.get::<_, String>(6)?,
                    "periodStart": row.get::<_, Option<String>>(7)?,
                    "periodEnd": row.get::<_, Option<String>>(8)?,
                    "openingBalance": row.get::<_, Option<f64>>(9)?,
                    "closingBalance": row.get::<_, Option<f64>>(10)?,
                    "status": row.get::<_, String>(11)?,
                    "warnings": warnings.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()).unwrap_or(serde_json::Value::Array(Vec::new())),
                    "parseError": row.get::<_, Option<String>>(13)?,
                    "createdAt": row.get::<_, i64>(14)?,
                    "updatedAt": row.get::<_, i64>(15)?,
                    "lineCount": row.get::<_, i64>(16)?,
                    "pendingCount": row.get::<_, i64>(17)?,
                    "importedCount": row.get::<_, i64>(18)?,
                }))
            },
        )
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({ "items": items })))
}

pub async fn get_statement(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state
        .db
        .get()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let statement = conn
        .query_row(
            "SELECT s.id, s.source, s.institution, s.account_name, s.account_number,
                    s.account_id, s.currency_code, s.period_start, s.period_end,
                    s.opening_balance, s.closing_balance, s.source_path, s.stored_path,
                    s.status, s.parsed_json, s.parse_error, s.warnings,
                    s.created_at, s.updated_at, a.name
             FROM bank_statements s
             LEFT JOIN accounts a ON a.id = s.account_id
             WHERE s.id=?1 AND s.deleted_at IS NULL",
            rusqlite::params![id],
            |row| {
                let warnings: Option<String> = row.get(16)?;
                let parsed: Option<String> = row.get(14)?;
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "source": row.get::<_, String>(1)?,
                    "institution": row.get::<_, String>(2)?,
                    "accountName": row.get::<_, String>(3)?,
                    "accountNumber": row.get::<_, Option<String>>(4)?,
                    "accountId": row.get::<_, Option<String>>(5)?,
                    "resolvedAccountName": row.get::<_, Option<String>>(19)?,
                    "currencyCode": row.get::<_, String>(6)?,
                    "periodStart": row.get::<_, Option<String>>(7)?,
                    "periodEnd": row.get::<_, Option<String>>(8)?,
                    "openingBalance": row.get::<_, Option<f64>>(9)?,
                    "closingBalance": row.get::<_, Option<f64>>(10)?,
                    "sourcePath": row.get::<_, Option<String>>(11)?,
                    "storedPath": row.get::<_, Option<String>>(12)?,
                    "status": row.get::<_, String>(13)?,
                    "parsed": parsed.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
                    "parseError": row.get::<_, Option<String>>(15)?,
                    "warnings": warnings.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()).unwrap_or(serde_json::Value::Array(Vec::new())),
                    "createdAt": row.get::<_, i64>(17)?,
                    "updatedAt": row.get::<_, i64>(18)?,
                }))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => error(StatusCode::NOT_FOUND, "statement not found"),
            other => error(StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        })?;

    let mut line_stmt = conn
        .prepare(
            "SELECT l.id, l.source_ref, l.posted_at, l.description, l.amount,
                    l.currency_code, l.balance_after, l.category_hint, l.line_kind,
                    l.suggested_type, l.suggested_category, l.metadata_json,
                    l.status, l.transaction_id, l.import_error, l.duplicate_of
             FROM bank_statement_lines l
             WHERE l.statement_id=?1
             ORDER BY l.posted_at DESC, l.created_at ASC",
        )
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let lines = line_stmt
        .query_map(rusqlite::params![id], |row| {
            let metadata: Option<String> = row.get(11)?;
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "sourceRef": row.get::<_, Option<String>>(1)?,
                "postedAt": row.get::<_, i64>(2)?,
                "description": row.get::<_, String>(3)?,
                "amount": row.get::<_, f64>(4)?,
                "currencyCode": row.get::<_, String>(5)?,
                "balanceAfter": row.get::<_, Option<f64>>(6)?,
                "categoryHint": row.get::<_, Option<String>>(7)?,
                "lineKind": row.get::<_, String>(8)?,
                "suggestedType": row.get::<_, Option<String>>(9)?,
                "suggestedCategory": row.get::<_, Option<String>>(10)?,
                "metadata": metadata.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
                "status": row.get::<_, String>(12)?,
                "transactionId": row.get::<_, Option<String>>(13)?,
                "importError": row.get::<_, Option<String>>(14)?,
                "duplicateOf": row.get::<_, Option<String>>(15)?,
            }))
        })
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();

    let mut match_stmt = conn
        .prepare(
            "SELECT m.id, m.debit_line_id, m.credit_line_id, m.transaction_id,
                    m.confidence, m.reason, m.status,
                    d.description, d.amount, d.currency_code, d.posted_at, d.status,
                    c.description, c.amount, c.currency_code, c.posted_at, c.status
             FROM transfer_matches m
             JOIN bank_statement_lines d ON d.id = m.debit_line_id
             JOIN bank_statement_lines c ON c.id = m.credit_line_id
             WHERE m.debit_line_id IN (SELECT id FROM bank_statement_lines WHERE statement_id=?1)
                OR m.credit_line_id IN (SELECT id FROM bank_statement_lines WHERE statement_id=?1)",
        )
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    // The two sides of a transfer usually live in different statements (a CA
    // file has one statement per account), so the counterpart is joined here
    // rather than looked up among this statement's lines. Without it the review
    // screen could see the match but not the other side.
    let matches = match_stmt
        .query_map(rusqlite::params![id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "debitLineId": row.get::<_, String>(1)?,
                "creditLineId": row.get::<_, String>(2)?,
                "transactionId": row.get::<_, Option<String>>(3)?,
                "confidence": row.get::<_, f64>(4)?,
                "reason": row.get::<_, Option<String>>(5)?,
                "status": row.get::<_, String>(6)?,
                "debit": {
                    "id": row.get::<_, String>(1)?,
                    "description": row.get::<_, String>(7)?,
                    "amount": row.get::<_, f64>(8)?,
                    "currencyCode": row.get::<_, String>(9)?,
                    "postedAt": row.get::<_, i64>(10)?,
                    "status": row.get::<_, String>(11)?,
                },
                "credit": {
                    "id": row.get::<_, String>(2)?,
                    "description": row.get::<_, String>(12)?,
                    "amount": row.get::<_, f64>(13)?,
                    "currencyCode": row.get::<_, String>(14)?,
                    "postedAt": row.get::<_, i64>(15)?,
                    "status": row.get::<_, String>(16)?,
                },
            }))
        })
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({
        "statement": statement,
        "lines": lines,
        "matches": matches,
    })))
}

pub async fn import_statement(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ImportRequest>,
) -> Result<Json<ImportOutcome>, ApiError> {
    let mut conn = state
        .db
        .get()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let tx = conn
        .transaction()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let outcome = store::import_statement(&tx, &id, request, &state.config.base_currency)
        .map_err(|e| error(StatusCode::BAD_REQUEST, e.to_string()))?;
    tx.commit()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(outcome))
}

#[derive(Debug, Deserialize)]
pub struct BatchImportRequest {
    #[serde(default)]
    pub statements: Vec<StatementImport>,
}

#[derive(Debug, Deserialize)]
pub struct StatementImport {
    pub id: String,
    #[serde(flatten)]
    pub request: ImportRequest,
}

pub async fn import_statements(
    State(state): State<AppState>,
    Json(body): Json<BatchImportRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut conn = state
        .db
        .get()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let tx = conn
        .transaction()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut outcomes = Vec::new();
    for statement in body.statements {
        let outcome = store::import_statement(
            &tx,
            &statement.id,
            statement.request,
            &state.config.base_currency,
        )
        .map_err(|e| error(StatusCode::BAD_REQUEST, e.to_string()))?;
        outcomes.push(outcome);
    }
    tx.commit()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "statements": outcomes })))
}

pub async fn ignore_statement(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state
        .db
        .get()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let now = db::server_time_ms();
    let changed = conn
        .execute(
            "UPDATE bank_statements SET status='ignored', updated_at=?1
             WHERE id=?2 AND deleted_at IS NULL AND status <> 'imported'",
            rusqlite::params![now, id],
        )
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if changed == 0 {
        return Err(error(
            StatusCode::CONFLICT,
            "statement not found or already imported",
        ));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete_statement(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state
        .db
        .get()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let now = db::server_time_ms();
    let changed = conn
        .execute(
            "UPDATE bank_statements SET deleted_at=?1, updated_at=?1 WHERE id=?2",
            rusqlite::params![now, id],
        )
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if changed == 0 {
        return Err(error(StatusCode::NOT_FOUND, "statement not found"));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Scan / upload
// ---------------------------------------------------------------------------

pub async fn scan_statements(State(state): State<AppState>) -> Json<serde_json::Value> {
    let mut seen = HashSet::new();
    let results = statements::watch::scan_once(&state, &mut seen).await;
    let items = results
        .into_iter()
        .map(|result| match result {
            Ok(outcomes) => serde_json::json!({ "ok": true, "outcomes": outcomes }),
            Err(error) => serde_json::json!({ "ok": false, "error": error.to_string() }),
        })
        .collect::<Vec<_>>();
    Json(serde_json::json!({ "items": items }))
}

pub async fn upload_statement(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut source: Option<SourceId> = None;
    let mut file: Option<(String, Vec<u8>)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| error(StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "source" => {
                let value = field
                    .text()
                    .await
                    .map_err(|e| error(StatusCode::BAD_REQUEST, e.to_string()))?;
                source = SourceId::parse(value.trim());
            }
            "file" => {
                let filename = field
                    .file_name()
                    .map(str::to_owned)
                    .unwrap_or_else(|| "upload".to_string());
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| error(StatusCode::BAD_REQUEST, e.to_string()))?;
                file = Some((filename, bytes.to_vec()));
            }
            _ => {}
        }
    }

    let source = source.ok_or_else(|| error(StatusCode::BAD_REQUEST, "missing source field"))?;
    let (filename, bytes) =
        file.ok_or_else(|| error(StatusCode::BAD_REQUEST, "missing file field"))?;
    let kind = std::path::Path::new(&filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(DocumentKind::parse_extension)
        .ok_or_else(|| {
            error(
                StatusCode::BAD_REQUEST,
                "file must have a .csv or .pdf extension",
            )
        })?;

    let docs = vec![RawDocument::file(
        kind,
        bytes,
        std::path::PathBuf::from(&filename),
    )];
    let outcomes = statements::run_upload(source, docs, &state)
        .await
        .map_err(|e| error(StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "outcomes": outcomes })))
}

// ---------------------------------------------------------------------------
// Feeds
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FeedStatus {
    provider: String,
    enabled: bool,
    environment: String,
    lookback_days: i64,
    credentials_set: bool,
    status: String,
    last_synced_at: Option<i64>,
    last_attempt_at: Option<i64>,
    last_error: Option<String>,
}

pub async fn list_feeds(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = state
        .db
        .get()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let feeds = [("trading212", &state.config.t212)];
    let mut items = Vec::new();
    for (provider, config) in feeds {
        let row = conn
            .query_row(
                "SELECT status, last_synced_at, last_attempt_at, last_error
                 FROM statement_feeds WHERE provider=?1",
                rusqlite::params![provider],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .ok();
        let (status, last_synced_at, last_attempt_at, last_error) =
            row.unwrap_or(("idle".to_string(), None, None, None));
        items.push(FeedStatus {
            provider: provider.to_string(),
            enabled: config.enabled,
            environment: config.environment.clone(),
            lookback_days: config.lookback_days,
            credentials_set: !config.api_key.is_empty() && !config.api_secret.is_empty(),
            status,
            last_synced_at,
            last_attempt_at,
            last_error,
        });
    }
    Ok(Json(serde_json::json!({ "items": items })))
}

pub async fn sync_feed(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let source = SourceId::parse(&provider)
        .filter(|source| source.is_feed())
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown feed provider"))?;
    let now = db::server_time_ms();
    {
        let conn = state
            .db
            .get()
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        conn.execute(
            "INSERT INTO statement_feeds (provider, status, last_attempt_at, updated_at)
             VALUES (?1, 'syncing', ?2, ?2)
             ON CONFLICT(provider) DO UPDATE SET
                status='syncing', last_attempt_at=excluded.last_attempt_at,
                last_error=NULL, updated_at=excluded.updated_at",
            rusqlite::params![source.as_str(), now],
        )
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let result = statements::run_spec(&SourceSpec::Feed { provider: source }, &state).await;
    let conn = state
        .db
        .get()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match result {
        Ok(outcomes) => {
            conn.execute(
                "UPDATE statement_feeds SET status='idle', last_synced_at=?1,
                    last_attempt_at=?1, last_error=NULL, updated_at=?1 WHERE provider=?2",
                rusqlite::params![now, source.as_str()],
            )
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(Json(serde_json::json!({ "outcomes": outcomes })))
        }
        Err(cause) => {
            conn.execute(
                "UPDATE statement_feeds SET status='error', last_attempt_at=?1,
                    last_error=?2, updated_at=?1 WHERE provider=?3",
                rusqlite::params![now, cause.to_string(), source.as_str()],
            )
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Err(error(StatusCode::BAD_GATEWAY, cause.to_string()))
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedSettings {
    pub enabled: bool,
    pub environment: String,
    pub lookback_days: i64,
    pub api_key_set: bool,
    pub api_secret_set: bool,
}

pub async fn get_feed_settings(State(state): State<AppState>) -> Json<serde_json::Value> {
    let feed = &state.config.t212;
    Json(serde_json::json!({
        "trading212": FeedSettings {
            enabled: feed.enabled,
            environment: feed.environment.clone(),
            lookback_days: feed.lookback_days,
            api_key_set: !feed.api_key.is_empty(),
            api_secret_set: !feed.api_secret.is_empty(),
        }
    }))
}

#[derive(Debug, Deserialize)]
pub struct PutFeedSettings {
    #[serde(default)]
    pub trading212: Option<T212SettingsUpdate>,
}

#[derive(Debug, Deserialize)]
pub struct T212SettingsUpdate {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(rename = "lookbackDays", default)]
    pub lookback_days: Option<i64>,
    #[serde(rename = "apiKey", default)]
    pub api_key: Option<String>,
    #[serde(rename = "apiSecret", default)]
    pub api_secret: Option<String>,
    #[serde(rename = "clearCredentials", default)]
    pub clear_credentials: Option<bool>,
}

pub async fn put_feed_settings(
    Json(body): Json<PutFeedSettings>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(update) = body.trading212 else {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "missing trading212 settings",
        ));
    };
    if let Some(environment) = update.environment.as_deref() {
        if environment != "live" && environment != "demo" {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "environment must be live or demo",
            ));
        }
    }
    if let Some(days) = update.lookback_days {
        if !(1..=3650).contains(&days) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "lookbackDays must be between 1 and 3650",
            ));
        }
    }

    update_overlay(|root| {
        let statements = root
            .entry("statements".to_string())
            .or_insert_with(|| serde_json::json!({}));
        let feeds = statements
            .as_object_mut()
            .expect("statements is an object")
            .entry("feeds".to_string())
            .or_insert_with(|| serde_json::json!({}));
        let feed = feeds
            .as_object_mut()
            .expect("feeds is an object")
            .entry("trading212".to_string())
            .or_insert_with(|| serde_json::json!({}));
        let feed = feed.as_object_mut().expect("trading212 is an object");
        if let Some(enabled) = update.enabled {
            feed.insert("enabled".to_string(), serde_json::json!(enabled));
        }
        if let Some(environment) = update.environment {
            feed.insert("environment".to_string(), serde_json::json!(environment));
        }
        if let Some(days) = update.lookback_days {
            feed.insert("lookback_days".to_string(), serde_json::json!(days));
        }
        if update.clear_credentials.unwrap_or(false) {
            feed.remove("api_key");
            feed.remove("api_secret");
        } else {
            if let Some(key) = update.api_key.filter(|key| !key.trim().is_empty()) {
                feed.insert("api_key".to_string(), serde_json::json!(key.trim()));
            }
            if let Some(secret) = update.api_secret.filter(|secret| !secret.trim().is_empty()) {
                feed.insert("api_secret".to_string(), serde_json::json!(secret.trim()));
            }
        }
    })
    .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "restartRequired": true,
    })))
}

fn update_overlay(
    update: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) -> anyhow::Result<()> {
    let path = config::overlay_config_path();
    let mut root = if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        match serde_json::from_str::<serde_json::Value>(&text)? {
            serde_json::Value::Object(map) => map,
            _ => anyhow::bail!("{} must contain a JSON object", path.display()),
        }
    } else {
        serde_json::Map::new()
    };
    update(&mut root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Write-then-rename so a crash cannot leave a half-written config that the
    // next server start would reject. The file holds provider credentials, so
    // it is created owner-only rather than left at the umask's default.
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(&root)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&temp, &path)?;
    Ok(())
}
