//! Receipt matching: what a receipt is the proof of, and what it teaches.
//!
//! The server does all of it; the phone only sends receipts and the desktop
//! shows and edits the results. See `context/receipts/matching.md`.
//!
//! - [`stores`]: the receipt's store (tax number, learned names, brand).
//! - [`transactions`]: the statement transaction the receipt paid for.
//! - [`products`]: each line's ingredient, and the price it records.
//! - [`assist`]: on demand, the model's proposals for what the rules left.
//! - [`effects`]: every change a receipt made, so each can be undone.
//!
//! Every decision records who made it: `auto` (a rule), `ai` (an accepted
//! proposal) or `manual`. A rule never overrides `manual` or `ai`, and an undo
//! is remembered so the rule does not make the same decision again.

pub mod assist;
pub mod effects;
pub mod products;
pub mod stores;
pub mod text;
pub mod transactions;

use rusqlite::{Connection, OptionalExtension, params};

use crate::receipt_ocr::ParsedReceipt;

pub const AUTO: &str = "auto";
pub const AI: &str = "ai";
pub const MANUAL: &str = "manual";

/// The receipt's reading, if it was read and the reading parses.
pub fn reading(conn: &Connection, receipt_id: &str) -> rusqlite::Result<Option<ParsedReceipt>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT parsed_json FROM receipts WHERE id = ?1 AND deleted_at IS NULL AND parsed = 1",
            params![receipt_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

/// What one matching pass decided, for the caller to report.
#[derive(Debug, Default, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pass {
    pub receipt_id: String,
    pub store_id: Option<String>,
    pub transaction_id: Option<String>,
    pub priced_lines: usize,
}

/// Runs every rule over one receipt: store, then transaction, then lines.
/// Runs in the caller's transaction. A receipt with no reading is skipped.
pub fn run(conn: &Connection, receipt_id: &str, now: i64) -> anyhow::Result<Pass> {
    let Some(reading) = reading(conn, receipt_id)? else {
        return Ok(Pass {
            receipt_id: receipt_id.to_string(),
            ..Default::default()
        });
    };
    let store_id = stores::resolve(conn, receipt_id, &reading, now)?;
    let transaction_id = transactions::resolve(conn, receipt_id, &reading, now)?;
    let priced_lines = products::resolve(conn, receipt_id, &reading, now)?;
    Ok(Pass {
        receipt_id: receipt_id.to_string(),
        store_id,
        transaction_id,
        priced_lines,
    })
}

/// [`run`] in its own transaction; what the parse job calls once a reading lands.
pub fn after_parse(conn: &Connection, receipt_id: &str, now: i64) -> anyhow::Result<Pass> {
    let tx = conn.unchecked_transaction()?;
    let pass = run(&tx, receipt_id, now)?;
    tx.commit()?;
    Ok(pass)
}

/// Every live receipt with a reading, matched again: the backfill, and what a
/// statement import calls for the transactions it booked. Idempotent.
pub fn run_all(conn: &Connection, now: i64) -> anyhow::Result<Vec<Pass>> {
    let tx = conn.unchecked_transaction()?;
    let ids: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM receipts WHERE deleted_at IS NULL AND parsed = 1 ORDER BY created_at",
        )?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?
    };
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        out.push(run(&tx, &id, now)?);
    }
    tx.commit()?;
    Ok(out)
}
