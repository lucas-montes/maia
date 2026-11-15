//! The link between a receipt and the transaction it is the proof of.
//!
//! The link is the server's. It is stored at both ends,
//! `receipts.transaction_id` and `transactions.receipt_id`, and this module is
//! the only place that writes both. The phone only sends receipts and only
//! reads transactions, so a push from it can fill an empty end but never
//! clears one (see `handlers::receipts` and `handlers::push`).
//!
//! A link is one to one: a receipt proves one transaction, and a transaction
//! has one receipt. Only an expense or an income takes a receipt. A transfer,
//! which includes a trade, moves the user's own money and has nothing to prove.

use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, PartialEq)]
pub enum LinkError {
    ReceiptNotFound(String),
    TransactionNotFound(String),
    /// The transaction is a transfer (or a trade), which takes no receipt.
    NotLinkable {
        transaction_id: String,
        kind: String,
    },
    /// The transaction already holds another live receipt.
    TransactionTaken {
        transaction_id: String,
        receipt_id: String,
    },
    Db(String),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReceiptNotFound(id) => write!(f, "receipt {id} not found"),
            Self::TransactionNotFound(id) => write!(f, "transaction {id} not found"),
            Self::NotLinkable {
                transaction_id,
                kind,
            } => write!(
                f,
                "transaction {transaction_id} is a {kind}; only an expense or an income takes a receipt"
            ),
            Self::TransactionTaken {
                transaction_id,
                receipt_id,
            } => write!(
                f,
                "transaction {transaction_id} already has receipt {receipt_id}"
            ),
            Self::Db(e) => write!(f, "{e}"),
        }
    }
}

impl From<rusqlite::Error> for LinkError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.to_string())
    }
}

/// What a link changed, so the caller can say so.
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkOutcome {
    pub receipt_id: String,
    pub transaction_id: Option<String>,
    /// The transaction the receipt pointed at before, if it was another one.
    pub previous_transaction_id: Option<String>,
    /// True when that previous transaction was a draft made from this receipt
    /// and was deleted, because the real transaction now stands for it.
    pub previous_draft_deleted: bool,
}

/// Links a receipt to a transaction, replacing whatever the receipt pointed at.
///
/// Linking again to the same transaction changes nothing. A previous
/// transaction loses its end of the link. When it was a draft that held this
/// receipt, it only existed because of the receipt, so it is deleted rather than
/// left in the ledger as a second copy of the same spend.
pub fn link(
    conn: &Connection,
    receipt_id: &str,
    transaction_id: &str,
    now: i64,
) -> Result<LinkOutcome, LinkError> {
    let tx = conn.unchecked_transaction()?;
    let previous = live_receipt_link(&tx, receipt_id)?;

    let found: Option<(String, Option<String>, bool)> = tx
        .query_row(
            "SELECT t.type, t.receipt_id, EXISTS(SELECT 1 FROM trades tr WHERE tr.transaction_id = t.id)
             FROM transactions t WHERE t.id = ?1 AND t.deleted_at IS NULL",
            params![transaction_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((kind, held, is_trade)) = found else {
        return Err(LinkError::TransactionNotFound(transaction_id.to_string()));
    };
    if is_trade || (kind != "expense" && kind != "income") {
        return Err(LinkError::NotLinkable {
            transaction_id: transaction_id.to_string(),
            kind: if is_trade { "trade".to_string() } else { kind },
        });
    }
    if let Some(other) = held.filter(|r| r != receipt_id) {
        let other_is_live: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM receipts WHERE id = ?1 AND deleted_at IS NULL)",
            params![other],
            |r| r.get(0),
        )?;
        if other_is_live {
            return Err(LinkError::TransactionTaken {
                transaction_id: transaction_id.to_string(),
                receipt_id: other,
            });
        }
    }

    let previous = previous.filter(|p| p != transaction_id);
    let previous_draft_deleted = match &previous {
        Some(p) => release(&tx, p, receipt_id, true, now)?,
        None => false,
    };
    tx.execute(
        "UPDATE receipts SET transaction_id = ?1, updated_at = ?2 WHERE id = ?3 AND transaction_id IS NOT ?1",
        params![transaction_id, now, receipt_id],
    )?;
    tx.execute(
        "UPDATE transactions SET receipt_id = ?1, updated_at = ?2 WHERE id = ?3 AND receipt_id IS NOT ?1",
        params![receipt_id, now, transaction_id],
    )?;
    tx.commit()?;
    Ok(LinkOutcome {
        receipt_id: receipt_id.to_string(),
        transaction_id: Some(transaction_id.to_string()),
        previous_transaction_id: previous,
        previous_draft_deleted,
    })
}

/// Unlinks a receipt from its transaction, clearing both ends. The transaction
/// stays, draft or not: unlinking says the receipt was matched to the wrong
/// one, not that the spend did not happen.
pub fn unlink(conn: &Connection, receipt_id: &str, now: i64) -> Result<LinkOutcome, LinkError> {
    let tx = conn.unchecked_transaction()?;
    let previous = live_receipt_link(&tx, receipt_id)?;
    if let Some(p) = &previous {
        release(&tx, p, receipt_id, false, now)?;
        tx.execute(
            "UPDATE receipts SET transaction_id = NULL, updated_at = ?1 WHERE id = ?2",
            params![now, receipt_id],
        )?;
    }
    tx.commit()?;
    Ok(LinkOutcome {
        receipt_id: receipt_id.to_string(),
        transaction_id: None,
        previous_transaction_id: previous,
        previous_draft_deleted: false,
    })
}

/// The live receipt's current link, or an error when there is no such receipt.
fn live_receipt_link(conn: &Connection, receipt_id: &str) -> Result<Option<String>, LinkError> {
    conn.query_row(
        "SELECT transaction_id FROM receipts WHERE id = ?1 AND deleted_at IS NULL",
        params![receipt_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()?
    .ok_or_else(|| LinkError::ReceiptNotFound(receipt_id.to_string()))
}

/// Takes the receipt off a transaction it is leaving. With `drop_draft`, a
/// draft that held this receipt is deleted. Returns whether it was.
fn release(
    conn: &Connection,
    transaction_id: &str,
    receipt_id: &str,
    drop_draft: bool,
    now: i64,
) -> Result<bool, LinkError> {
    let is_own_draft: bool = conn
        .query_row(
            "SELECT is_draft = 1 FROM transactions WHERE id = ?1 AND receipt_id = ?2 AND deleted_at IS NULL",
            params![transaction_id, receipt_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(false);
    if drop_draft && is_own_draft {
        conn.execute(
            "UPDATE transactions SET receipt_id = NULL, deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, transaction_id],
        )?;
        return Ok(true);
    }
    conn.execute(
        "UPDATE transactions SET receipt_id = NULL, updated_at = ?1 WHERE id = ?2 AND receipt_id = ?3",
        params![now, transaction_id, receipt_id],
    )?;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    type Conn = r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>;

    fn conn() -> Conn {
        let conn = crate::db::init_memory().unwrap().get().unwrap();
        for id in ["r1", "r2"] {
            conn.execute(
                "INSERT INTO receipts (id, local_path, created_at, updated_at) VALUES (?1, '/p.jpg', 1, 1)",
                params![id],
            )
            .unwrap();
        }
        for (id, kind, draft) in [
            ("shop", "expense", 0),
            ("refund", "income", 0),
            ("draft", "expense", 1),
            ("move", "transfer", 0),
            ("buy", "transfer", 0),
        ] {
            conn.execute(
                "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, date, is_draft, created_at, updated_at)
                 VALUES (?1, ?2, 12.5, 'EUR', 12.5, 1.0, 1000, ?3, 1, 1)",
                params![id, kind, draft],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO trades (transaction_id, side, symbol, quantity, price, created_at, updated_at)
             VALUES ('buy', 'buy', 'VOO', 1, 12.5, 1, 1)",
            [],
        )
        .unwrap();
        conn
    }

    fn ends(
        conn: &Connection,
        receipt: &str,
        transaction: &str,
    ) -> (Option<String>, Option<String>) {
        let r = conn
            .query_row(
                "SELECT transaction_id FROM receipts WHERE id = ?1",
                params![receipt],
                |r| r.get(0),
            )
            .unwrap();
        let t = conn
            .query_row(
                "SELECT receipt_id FROM transactions WHERE id = ?1",
                params![transaction],
                |r| r.get(0),
            )
            .unwrap();
        (r, t)
    }

    #[test]
    fn a_link_is_written_at_both_ends() {
        let conn = conn();
        link(&conn, "r1", "shop", 5).unwrap();
        assert_eq!(
            ends(&conn, "r1", "shop"),
            (Some("shop".into()), Some("r1".into()))
        );
        // A refund's receipt proves an income.
        link(&conn, "r2", "refund", 5).unwrap();
    }

    #[test]
    fn a_transfer_or_a_trade_takes_no_receipt() {
        let conn = conn();
        assert!(matches!(
            link(&conn, "r1", "move", 5),
            Err(LinkError::NotLinkable { .. })
        ));
        assert_eq!(
            link(&conn, "r1", "buy", 5),
            Err(LinkError::NotLinkable {
                transaction_id: "buy".into(),
                kind: "trade".into()
            })
        );
    }

    #[test]
    fn a_transaction_holds_one_receipt() {
        let conn = conn();
        link(&conn, "r1", "shop", 5).unwrap();
        assert!(matches!(
            link(&conn, "r2", "shop", 6),
            Err(LinkError::TransactionTaken { .. })
        ));
        // Once the first receipt is gone, its stale end does not block.
        conn.execute("UPDATE receipts SET deleted_at = 7 WHERE id = 'r1'", [])
            .unwrap();
        link(&conn, "r2", "shop", 8).unwrap();
    }

    #[test]
    fn relinking_moves_the_receipt_and_drops_its_own_draft() {
        let conn = conn();
        link(&conn, "r1", "draft", 5).unwrap();
        let outcome = link(&conn, "r1", "shop", 6).unwrap();
        assert_eq!(outcome.previous_transaction_id.as_deref(), Some("draft"));
        assert!(outcome.previous_draft_deleted);
        let deleted: Option<i64> = conn
            .query_row(
                "SELECT deleted_at FROM transactions WHERE id = 'draft'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deleted, Some(6));
        assert_eq!(
            ends(&conn, "r1", "shop"),
            (Some("shop".into()), Some("r1".into()))
        );
    }

    #[test]
    fn relinking_off_a_real_transaction_keeps_it() {
        let conn = conn();
        link(&conn, "r1", "shop", 5).unwrap();
        let outcome = link(&conn, "r1", "refund", 6).unwrap();
        assert!(!outcome.previous_draft_deleted);
        assert_eq!(ends(&conn, "r1", "shop").1, None);
        let deleted: Option<i64> = conn
            .query_row(
                "SELECT deleted_at FROM transactions WHERE id = 'shop'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deleted, None);
    }

    #[test]
    fn unlinking_clears_both_ends_and_keeps_a_draft() {
        let conn = conn();
        link(&conn, "r1", "draft", 5).unwrap();
        let outcome = unlink(&conn, "r1", 6).unwrap();
        assert_eq!(outcome.previous_transaction_id.as_deref(), Some("draft"));
        assert_eq!(ends(&conn, "r1", "draft"), (None, None));
        let deleted: Option<i64> = conn
            .query_row(
                "SELECT deleted_at FROM transactions WHERE id = 'draft'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deleted, None);
    }

    #[test]
    fn linking_again_to_the_same_transaction_changes_nothing() {
        let conn = conn();
        link(&conn, "r1", "shop", 5).unwrap();
        link(&conn, "r1", "shop", 9).unwrap();
        let updated: i64 = conn
            .query_row("SELECT updated_at FROM receipts WHERE id = 'r1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(updated, 5, "a no-op must not wake every client's pull");
    }

    #[test]
    fn unknown_ends_are_reported() {
        let conn = conn();
        assert_eq!(
            link(&conn, "nope", "shop", 5),
            Err(LinkError::ReceiptNotFound("nope".into()))
        );
        assert_eq!(
            link(&conn, "r1", "nope", 5),
            Err(LinkError::TransactionNotFound("nope".into()))
        );
    }
}
