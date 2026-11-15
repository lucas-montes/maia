//! What a receipt changed, so its page can open, edit and undo each change.
//!
//! Every writer that acts for a receipt (a matcher, an accept, a manual edit
//! from the receipt's page) records one row in `receipt_effects`, in the same
//! database transaction as the change. A row is never deleted: an undo marks
//! it `undone_at`, and so does a later change that replaces it.
//!
//! An undo also remembers what it rejected (`receipt_rejections`), so the
//! matcher does not propose the same pair again on the next run.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

pub const STORE_RESOLVED: &str = "store_resolved";
pub const STORE_CREATED: &str = "store_created";
pub const STORE_RENAMED: &str = "store_renamed";
pub const STORE_FILLED: &str = "store_filled";
pub const ALIAS_LEARNED: &str = "alias_learned";
pub const TRANSACTION_LINKED: &str = "transaction_linked";
pub const PRICE_RECORDED: &str = "price_recorded";
pub const PRODUCT_ALIAS_LEARNED: &str = "product_alias_learned";

/// What a rejection is about.
pub const REJECT_STORE: &str = "store";
pub const REJECT_TRANSACTION: &str = "transaction";
pub const REJECT_INGREDIENT: &str = "ingredient";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    pub id: String,
    pub receipt_id: String,
    pub kind: String,
    pub target_table: String,
    pub target_id: String,
    pub line: Option<i64>,
    pub before: Value,
    pub after: Value,
    pub source: String,
    pub created_at: i64,
    pub undone_at: Option<i64>,
}

/// One change to record.
pub struct NewEffect<'a> {
    pub receipt_id: &'a str,
    pub kind: &'a str,
    pub target_table: &'a str,
    pub target_id: &'a str,
    pub line: Option<i64>,
    pub before: Value,
    pub after: Value,
    pub source: &'a str,
}

pub fn record(conn: &Connection, effect: NewEffect<'_>, now: i64) -> rusqlite::Result<String> {
    let id = uuid::Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO receipt_effects
            (id, receipt_id, kind, target_table, target_id, line, before_json, after_json, source, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            effect.receipt_id,
            effect.kind,
            effect.target_table,
            effect.target_id,
            effect.line,
            effect.before.to_string(),
            effect.after.to_string(),
            effect.source,
            now
        ],
    )?;
    Ok(id)
}

const COLUMNS: &str = "id, receipt_id, kind, target_table, target_id, line, before_json, after_json, source, created_at, undone_at";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Effect> {
    let parse = |s: Option<String>| {
        s.and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null)
    };
    Ok(Effect {
        id: r.get(0)?,
        receipt_id: r.get(1)?,
        kind: r.get(2)?,
        target_table: r.get(3)?,
        target_id: r.get(4)?,
        line: r.get(5)?,
        before: parse(r.get(6)?),
        after: parse(r.get(7)?),
        source: r.get(8)?,
        created_at: r.get(9)?,
        undone_at: r.get(10)?,
    })
}

/// Every effect of a receipt, oldest first, undone ones included.
pub fn for_receipt(conn: &Connection, receipt_id: &str) -> rusqlite::Result<Vec<Effect>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM receipt_effects WHERE receipt_id = ?1 ORDER BY created_at, rowid"
    ))?;
    stmt.query_map(params![receipt_id], from_row)?.collect()
}

/// The live effects that touched a row: what the "from receipt" back link reads.
pub fn for_target(conn: &Connection, table: &str, id: &str) -> rusqlite::Result<Vec<Effect>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM receipt_effects
         WHERE target_table = ?1 AND target_id = ?2 AND undone_at IS NULL ORDER BY created_at"
    ))?;
    stmt.query_map(params![table, id], from_row)?.collect()
}

pub fn load(conn: &Connection, id: &str) -> rusqlite::Result<Option<Effect>> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM receipt_effects WHERE id = ?1"),
        params![id],
        from_row,
    )
    .optional()
}

/// Marks the receipt's live effects of `kind` (on `target_id` when given)
/// replaced, because a newer decision took their place.
pub fn supersede(
    conn: &Connection,
    receipt_id: &str,
    kind: &str,
    target_id: Option<&str>,
    line: Option<i64>,
    now: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE receipt_effects SET undone_at = ?1
         WHERE receipt_id = ?2 AND kind = ?3 AND undone_at IS NULL
           AND (?4 IS NULL OR target_id = ?4) AND (?5 IS NULL OR line = ?5)",
        params![now, receipt_id, kind, target_id, line],
    )
}

pub fn reject(
    conn: &Connection,
    receipt_id: &str,
    kind: &str,
    target_id: &str,
    line: Option<i64>,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO receipt_rejections (receipt_id, kind, target_id, line, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![receipt_id, kind, target_id, line.unwrap_or(-1), now],
    )?;
    Ok(())
}

pub fn is_rejected(
    conn: &Connection,
    receipt_id: &str,
    kind: &str,
    target_id: &str,
    line: Option<i64>,
) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM receipt_rejections
                       WHERE receipt_id = ?1 AND kind = ?2 AND target_id = ?3 AND line = ?4)",
        params![receipt_id, kind, target_id, line.unwrap_or(-1)],
        |r| r.get(0),
    )
}

#[derive(Debug, PartialEq)]
pub enum UndoError {
    NotFound(String),
    AlreadyUndone,
    /// The target was changed after the effect; undoing would overwrite that.
    Changed(String),
    /// The effect cannot be undone while something still depends on it.
    InUse(String),
    Db(String),
}

impl std::fmt::Display for UndoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "effect {id} not found"),
            Self::AlreadyUndone => write!(f, "already undone"),
            Self::Changed(why) => write!(f, "changed since: {why}; edit it instead"),
            Self::InUse(why) => write!(f, "still in use: {why}"),
            Self::Db(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for UndoError {}

impl From<rusqlite::Error> for UndoError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.to_string())
    }
}

/// The identifier columns a receipt can fill on a store.
pub const STORE_DETAIL_COLUMNS: [&str; 10] = [
    "brand",
    "legal_name",
    "country",
    "tax_id",
    "tax_id_kind",
    "vat_number",
    "address",
    "city",
    "postcode",
    "website",
];

fn mark_undone(conn: &Connection, id: &str, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE receipt_effects SET undone_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    Ok(())
}

/// Undoes one effect. Runs in the caller's transaction.
pub fn undo(conn: &Connection, effect_id: &str, now: i64) -> Result<Effect, UndoError> {
    let effect =
        load(conn, effect_id)?.ok_or_else(|| UndoError::NotFound(effect_id.to_string()))?;
    if effect.undone_at.is_some() {
        return Err(UndoError::AlreadyUndone);
    }
    let receipt = effect.receipt_id.as_str();
    let target = effect.target_id.as_str();
    match effect.kind.as_str() {
        STORE_RESOLVED => {
            let current: Option<String> = conn
                .query_row(
                    "SELECT store_id FROM receipts WHERE id = ?1",
                    params![receipt],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            if current.as_deref() != Some(target) {
                return Err(UndoError::Changed(
                    "the receipt is on another store now".into(),
                ));
            }
            let before = effect.before.get("storeId").and_then(Value::as_str);
            let before_source = effect.before.get("storeSource").and_then(Value::as_str);
            conn.execute(
                "UPDATE receipts SET store_id = ?1, store_source = ?2 WHERE id = ?3",
                params![before, before.and(before_source), receipt],
            )?;
            reject(conn, receipt, REJECT_STORE, target, None, now)?;
            mark_undone(conn, &effect.id, now)?;
            // What came with the decision goes with it, where it still stands.
            for other in for_receipt(conn, receipt)? {
                if other.undone_at.is_none()
                    && other.created_at >= effect.created_at
                    && other.target_id == target
                    && other.kind == STORE_FILLED
                {
                    let _ = undo(conn, &other.id, now);
                }
                if other.undone_at.is_none()
                    && other.created_at >= effect.created_at
                    && other.kind == ALIAS_LEARNED
                    && other.after.get("storeId").and_then(Value::as_str) == Some(target)
                {
                    let _ = undo(conn, &other.id, now);
                }
            }
        }
        STORE_CREATED => {
            let prices: i64 = conn.query_row(
                "SELECT COUNT(*) FROM ingredient_prices WHERE store_id = ?1 AND deleted_at IS NULL",
                params![target],
                |r| r.get(0),
            )?;
            let receipts: i64 = conn.query_row(
                "SELECT COUNT(*) FROM receipts WHERE store_id = ?1 AND id <> ?2 AND deleted_at IS NULL",
                params![target, receipt],
                |r| r.get(0),
            )?;
            if prices > 0 || receipts > 0 {
                return Err(UndoError::InUse(format!(
                    "{prices} price(s) and {receipts} other receipt(s) use this store"
                )));
            }
            conn.execute(
                "UPDATE receipts SET store_id = NULL, store_source = NULL WHERE id = ?1 AND store_id = ?2",
                params![receipt, target],
            )?;
            conn.execute(
                "UPDATE stores SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
                params![now, target],
            )?;
            conn.execute(
                "DELETE FROM store_aliases WHERE store_id = ?1",
                params![target],
            )?;
            conn.execute(
                "UPDATE receipt_effects SET undone_at = ?1
                 WHERE receipt_id = ?2 AND undone_at IS NULL
                   AND (target_id = ?3 OR json_extract(after_json, '$.storeId') = ?3)",
                params![now, receipt, target],
            )?;
            reject(conn, receipt, REJECT_STORE, target, None, now)?;
        }
        STORE_RENAMED => {
            let name: Option<String> = conn
                .query_row(
                    "SELECT name FROM stores WHERE id = ?1",
                    params![target],
                    |r| r.get(0),
                )
                .optional()?;
            let after = effect.after.get("name").and_then(Value::as_str);
            if name.as_deref() != after {
                return Err(UndoError::Changed("the store was renamed again".into()));
            }
            let before = effect
                .before
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            conn.execute(
                "UPDATE stores SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![before, now, target],
            )?;
            mark_undone(conn, &effect.id, now)?;
        }
        STORE_FILLED => {
            let Some(after) = effect.after.as_object() else {
                mark_undone(conn, &effect.id, now)?;
                return Ok(effect);
            };
            for (column, value) in after {
                if !STORE_DETAIL_COLUMNS.contains(&column.as_str()) {
                    continue;
                }
                let current: Option<String> = conn
                    .query_row(
                        &format!("SELECT {column} FROM stores WHERE id = ?1"),
                        params![target],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten();
                if current.as_deref() != value.as_str() {
                    return Err(UndoError::Changed(format!(
                        "the store's {column} was edited"
                    )));
                }
            }
            for column in after
                .keys()
                .filter(|c| STORE_DETAIL_COLUMNS.contains(&c.as_str()))
            {
                let before = effect.before.get(column).and_then(Value::as_str);
                // `column` is one of STORE_DETAIL_COLUMNS, never user input.
                conn.execute(
                    &format!("UPDATE stores SET {column} = ?1 WHERE id = ?2"),
                    params![before, target],
                )?;
            }
            mark_undone(conn, &effect.id, now)?;
        }
        ALIAS_LEARNED => {
            let store = effect.after.get("storeId").and_then(Value::as_str);
            conn.execute(
                "DELETE FROM store_aliases WHERE alias = ?1 AND store_id = ?2",
                params![target, store],
            )?;
            mark_undone(conn, &effect.id, now)?;
        }
        TRANSACTION_LINKED => {
            let current: Option<String> = conn
                .query_row(
                    "SELECT transaction_id FROM receipts WHERE id = ?1",
                    params![receipt],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            if current.as_deref() != Some(target) {
                return Err(UndoError::Changed(
                    "the receipt is linked to another transaction now".into(),
                ));
            }
            crate::receipt_links::unlink_in(conn, receipt, now)
                .map_err(|e| UndoError::Db(e.to_string()))?;
            reject(conn, receipt, REJECT_TRANSACTION, target, None, now)?;
        }
        PRICE_RECORDED | PRODUCT_ALIAS_LEARNED => {
            crate::matching::products::undo_effect(conn, &effect, now)?;
        }
        other => return Err(UndoError::Db(format!("unknown effect kind {other}"))),
    }
    Ok(load(conn, effect_id)?.unwrap_or(effect))
}

/// `{column: value}` for the columns of `after` that differ from `before`.
pub fn changed(before: &Value, after: &Value) -> (Value, Value) {
    let mut b = serde_json::Map::new();
    let mut a = serde_json::Map::new();
    if let Some(after) = after.as_object() {
        for (k, v) in after {
            let old = before.get(k).cloned().unwrap_or(Value::Null);
            if &old != v {
                b.insert(k.clone(), old);
                a.insert(k.clone(), v.clone());
            }
        }
    }
    (Value::Object(b), Value::Object(a))
}

/// An empty JSON object, for effects with nothing before.
pub fn none() -> Value {
    json!({})
}
