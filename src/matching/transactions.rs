//! Receipt ↔ the statement transaction it paid for.
//!
//! A receipt is never booked: transactions come from statements only, and a
//! receipt is linked to one when the rules can tell which. Hard tests decide
//! whether a transaction *can* be the receipt's; the score only orders them.
//!
//! **Hard tests:** a live expense that is not a trade and holds no other
//! receipt; the receipt's total to the cent in the receipt's currency; the
//! purchase day (the `dd/mm` a card payment's description carries) within a
//! day of the receipt's, or else the posting day from a day before to five
//! after; and not a pair an undo rejected.
//!
//! **Automatic link:** exactly one transaction passes, its merchant or card
//! matches (or it is the only expense of that amount in a week), a cash
//! receipt is not put on a card payment, and no other receipt passes for that
//! transaction. Anything else is a suggestion. A link the user or the AI made
//! is never changed.

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::effects;
use super::text::{self, day_of, overlap, purchase_day, tokens};
use super::{AUTO, MANUAL};
use crate::receipt_ocr::ParsedReceipt;

const DAY_MS: i64 = 86_400_000;

/// One transaction the receipt could be for, as the picker shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub transaction_id: String,
    pub label: String,
    pub amount: f64,
    pub currency_code: String,
    pub date: i64,
    pub account_name: Option<String>,
    pub is_draft: bool,
    /// The receipt's total, in its currency, to the cent.
    pub exact_amount: bool,
    /// Every hard test passes, so this one could be linked automatically.
    pub passes: bool,
    pub merchant_match: bool,
    pub card_match: bool,
    /// Days between the receipt and the purchase (or posting) day.
    pub day_gap: i64,
    pub rejected: bool,
    pub score: f64,
}

/// What the receipt says about the payment.
struct Facts {
    day: NaiveDate,
    total: f64,
    currency: String,
    merchant_words: Vec<Vec<String>>,
    card: Option<String>,
    cash: bool,
}

/// The receipt's day from the reading's date text, or the day it was taken.
pub fn receipt_day(reading: &ParsedReceipt, fallback_ms: i64) -> NaiveDate {
    let raw = reading.date.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return dt.date_naive();
    }
    for format in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(raw, format) {
            return dt.date();
        }
    }
    for format in ["%Y-%m-%d", "%d.%m.%Y", "%d/%m/%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(raw.get(..10).unwrap_or(raw), format) {
            return d;
        }
    }
    day_of(fallback_ms).unwrap_or_default()
}

fn facts(
    conn: &Connection,
    receipt_id: &str,
    reading: &ParsedReceipt,
) -> rusqlite::Result<Option<Facts>> {
    let row: Option<(i64, Option<String>)> = conn
        .query_row(
            "SELECT created_at, store_id FROM receipts WHERE id = ?1 AND deleted_at IS NULL",
            params![receipt_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((created_at, store_id)) = row else {
        return Ok(None);
    };
    if !(reading.total > 0.0) {
        return Ok(None);
    }
    // The names this receipt's shop goes by: the reading's, and its store's.
    let mut names: Vec<String> = vec![reading.merchant.clone()];
    names.extend(reading.brand.clone());
    if let Some(id) = store_id {
        if let Some(s) = super::stores::load(conn, &id)? {
            names.push(s.name);
            names.extend(s.brand);
        }
        let mut stmt = conn.prepare("SELECT alias FROM store_aliases WHERE store_id = ?1")?;
        names.extend(
            stmt.query_map(params![id], |r| r.get::<_, String>(0))?
                .filter_map(Result::ok),
        );
    }
    let merchant_words = names
        .iter()
        .map(|n| tokens(n))
        .filter(|t| !t.is_empty())
        .collect();
    let card = reading
        .card_last4
        .as_deref()
        .map(|c| c.chars().filter(char::is_ascii_digit).collect::<String>())
        .filter(|c| c.len() == 4);
    Ok(Some(Facts {
        day: receipt_day(reading, created_at),
        total: reading.total,
        currency: reading.currency.trim().to_uppercase(),
        merchant_words,
        card,
        cash: reading.payment_method.as_deref() == Some("cash"),
    }))
}

struct Row {
    id: String,
    amount: f64,
    currency: String,
    date: i64,
    note: String,
    category: String,
    account_name: Option<String>,
    is_draft: bool,
}

fn evaluate(
    conn: &Connection,
    receipt_id: &str,
    f: &Facts,
    t: &Row,
) -> rusqlite::Result<Candidate> {
    let exact_amount =
        t.currency.eq_ignore_ascii_case(&f.currency) && (t.amount.abs() - f.total).abs() < 0.005;
    let description = format!("{} {}", t.note, t.category);
    let (gap, in_window) = match purchase_day(&t.note, t.date) {
        Some(bought) => {
            let gap = (bought - f.day).num_days();
            (gap.abs(), gap.abs() <= 1)
        }
        None => {
            let posted = day_of(t.date).unwrap_or_default();
            let gap = (posted - f.day).num_days();
            (gap.abs(), (-1..=5).contains(&gap))
        }
    };
    let words = tokens(&description);
    let merchant_match = f
        .merchant_words
        .iter()
        .any(|name| name.iter().all(|w| words.contains(w)) || overlap(name, &words) >= 0.5);
    let card_match = f.card.as_deref().is_some_and(|c| description.contains(c));
    let rejected =
        effects::is_rejected(conn, receipt_id, effects::REJECT_TRANSACTION, &t.id, None)?;
    let passes = exact_amount && in_window && !rejected;
    let score = if exact_amount { 10.0 } else { 0.0 }
        + if merchant_match { 3.0 } else { 0.0 }
        + if card_match { 3.0 } else { 0.0 }
        - gap.min(10) as f64 * 0.5
        - (t.amount.abs() - f.total).abs().min(50.0) * 0.1;
    Ok(Candidate {
        transaction_id: t.id.clone(),
        label: if t.note.is_empty() {
            t.category.clone()
        } else {
            t.note.clone()
        },
        amount: t.amount,
        currency_code: t.currency.clone(),
        date: t.date,
        account_name: t.account_name.clone(),
        is_draft: t.is_draft,
        exact_amount,
        passes,
        merchant_match,
        card_match,
        day_gap: gap,
        rejected,
        score,
    })
}

/// The expenses near the receipt's day that could take it: free of another
/// live receipt, or holding this one.
fn nearby(conn: &Connection, receipt_id: &str, day: NaiveDate) -> rusqlite::Result<Vec<Row>> {
    let start = text::day_start_ms(day) - 2 * DAY_MS;
    let end = text::day_start_ms(day) + 9 * DAY_MS;
    let mut stmt = conn.prepare(
        "SELECT t.id, t.amount, t.currency_code, t.date, COALESCE(t.note, ''), COALESCE(t.category, ''), a.name, t.is_draft
         FROM transactions t
         LEFT JOIN accounts a ON a.id = t.account_id
         WHERE t.deleted_at IS NULL AND t.type = 'expense'
           AND NOT EXISTS (SELECT 1 FROM trades tr WHERE tr.transaction_id = t.id)
           AND (t.receipt_id IS NULL OR t.receipt_id = ?1
                OR NOT EXISTS (SELECT 1 FROM receipts r WHERE r.id = t.receipt_id AND r.deleted_at IS NULL))
           AND t.date >= ?2 AND t.date < ?3
         ORDER BY t.date",
    )?;
    stmt.query_map(params![receipt_id, start, end], |r| {
        Ok(Row {
            id: r.get(0)?,
            amount: r.get(1)?,
            currency: r.get(2)?,
            date: r.get(3)?,
            note: r.get(4)?,
            category: r.get(5)?,
            account_name: r.get(6)?,
            is_draft: r.get::<_, i64>(7)? != 0,
        })
    })?
    .collect()
}

/// Every transaction near the receipt that could be its, best first.
pub fn candidates(
    conn: &Connection,
    receipt_id: &str,
    reading: &ParsedReceipt,
) -> rusqlite::Result<Vec<Candidate>> {
    let Some(f) = facts(conn, receipt_id, reading)? else {
        return Ok(vec![]);
    };
    let mut out = Vec::new();
    for t in nearby(conn, receipt_id, f.day)? {
        // A draft is a stand-in, never a candidate for the real thing.
        if t.is_draft {
            continue;
        }
        out.push(evaluate(conn, receipt_id, &f, &t)?);
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(out)
}

/// The transaction the rules would link the receipt to, if one stands out.
pub fn choose(
    conn: &Connection,
    receipt_id: &str,
    reading: &ParsedReceipt,
) -> rusqlite::Result<Option<String>> {
    let Some(f) = facts(conn, receipt_id, reading)? else {
        return Ok(None);
    };
    let all = candidates(conn, receipt_id, reading)?;
    let passing: Vec<&Candidate> = all.iter().filter(|c| c.passes).collect();
    let [one] = passing.as_slice() else {
        return Ok(None);
    };
    if f.cash && looks_like_card(conn, &one.transaction_id)? {
        return Ok(None);
    }
    let alone = all
        .iter()
        .filter(|c| c.exact_amount && c.day_gap <= 7)
        .count()
        == 1;
    if !(one.merchant_match || one.card_match || alone) {
        return Ok(None);
    }
    // Mutual: no other receipt waiting for a transaction passes for this one.
    let mut stmt = conn.prepare(
        "SELECT r.id FROM receipts r
         WHERE r.deleted_at IS NULL AND r.parsed = 1 AND r.id <> ?1
           AND (r.transaction_id IS NULL
                OR EXISTS (SELECT 1 FROM transactions t WHERE t.id = r.transaction_id AND t.is_draft = 1))",
    )?;
    let others: Vec<String> = stmt
        .query_map(params![receipt_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for other in others {
        let Some(their) = super::reading(conn, &other)? else {
            continue;
        };
        if candidates(conn, &other, &their)?
            .iter()
            .any(|c| c.passes && c.transaction_id == one.transaction_id)
        {
            return Ok(None);
        }
    }
    Ok(Some(one.transaction_id.clone()))
}

fn looks_like_card(conn: &Connection, transaction_id: &str) -> rusqlite::Result<bool> {
    let note: String = conn.query_row(
        "SELECT COALESCE(note, '') FROM transactions WHERE id = ?1",
        params![transaction_id],
        |r| r.get(0),
    )?;
    let lower = note.to_lowercase();
    Ok(lower.contains("carte") || lower.contains("card") || purchase_day(&note, 0).is_some())
}

/// The matcher's pass: links the receipt when one transaction stands out.
/// A link the user or the AI made is kept; so is any link to a real
/// transaction. Only an unlinked receipt, or one on its own draft, is matched.
pub fn resolve(
    conn: &Connection,
    receipt_id: &str,
    reading: &ParsedReceipt,
    now: i64,
) -> anyhow::Result<Option<String>> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT transaction_id, link_source FROM receipts WHERE id = ?1 AND deleted_at IS NULL",
            params![receipt_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((current, source)) = row else {
        return Ok(None);
    };
    if let Some(current) = &current {
        let on_own_draft: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM transactions WHERE id = ?1 AND is_draft = 1 AND receipt_id = ?2 AND deleted_at IS NULL)",
            params![current, receipt_id],
            |r| r.get(0),
        )?;
        if !on_own_draft
            || source.as_deref() == Some(MANUAL)
            || source.as_deref() == Some(super::AI)
        {
            return Ok(Some(current.clone()));
        }
    }
    match choose(conn, receipt_id, reading)? {
        Some(id) => {
            crate::receipt_links::link_in(conn, receipt_id, &id, AUTO, now)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(Some(id))
        }
        None => Ok(current),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::text::day_start_ms;

    type Conn = r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>;

    fn ms(y: i32, m: u32, d: u32) -> i64 {
        day_start_ms(NaiveDate::from_ymd_opt(y, m, d).unwrap())
    }

    fn conn() -> Conn {
        let conn = crate::db::init_memory().unwrap().get().unwrap();
        conn.execute(
            "INSERT INTO accounts (id, name, type, opening_balance, created_at, updated_at) VALUES ('eko', 'Eko by CA', 'cash', 0, 1, 1)",
            [],
        )
        .unwrap();
        conn
    }

    fn txn(conn: &Connection, id: &str, amount: f64, date: i64, note: &str) {
        conn.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, account_id, date, note, is_draft, created_at, updated_at)
             VALUES (?1, 'expense', ?2, 'EUR', ?2, 1.0, 'eko', ?3, ?4, 0, 1, 1)",
            params![id, amount, date, note],
        )
        .unwrap();
    }

    fn reading(total: f64, date: &str) -> ParsedReceipt {
        ParsedReceipt {
            merchant: "SPAR HRVATSKA d.o.o.".into(),
            brand: Some("Spar".into()),
            date: date.into(),
            total,
            currency: "EUR".into(),
            card_last4: Some("6510".into()),
            payment_method: Some("card".into()),
            ..Default::default()
        }
    }

    fn receipt(conn: &Connection, id: &str, r: &ParsedReceipt) {
        conn.execute(
            "INSERT INTO receipts (id, local_path, parsed, parsed_json, created_at, updated_at) VALUES (?1, '', 1, ?2, 1, 1)",
            params![id, serde_json::to_string(r).unwrap()],
        )
        .unwrap();
    }

    fn link_of(conn: &Connection, id: &str) -> (Option<String>, Option<String>) {
        conn.query_row(
            "SELECT transaction_id, link_source FROM receipts WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }

    #[test]
    fn a_card_payment_posted_days_later_is_linked_by_its_purchase_day() {
        let conn = conn();
        txn(
            &conn,
            "t1",
            5.96,
            ms(2026, 8, 3),
            "Paiement par carte X6510 SPAR ZAGREB 30/07 - Spar",
        );
        txn(
            &conn,
            "t2",
            5.96,
            ms(2026, 8, 3),
            "Paiement par carte X6510 SPAR ZAGREB 02/08 - Spar",
        );
        let r = reading(5.96, "2026-07-30T18:02:00");
        receipt(&conn, "r1", &r);
        assert_eq!(resolve(&conn, "r1", &r, 5).unwrap().as_deref(), Some("t1"));
        assert_eq!(link_of(&conn, "r1"), (Some("t1".into()), Some(AUTO.into())));
    }

    #[test]
    fn a_purchase_across_new_year() {
        let conn = conn();
        txn(
            &conn,
            "t1",
            12.5,
            ms(2027, 1, 2),
            "Paiement par carte X6510 SPAR ZAGREB 31/12",
        );
        let r = reading(12.5, "2026-12-31 10:00:00");
        receipt(&conn, "r1", &r);
        assert_eq!(resolve(&conn, "r1", &r, 5).unwrap().as_deref(), Some("t1"));
    }

    #[test]
    fn two_identical_receipts_link_nothing() {
        let conn = conn();
        txn(&conn, "t1", 12.5, ms(2026, 9, 1), "Spar");
        let r = reading(12.5, "2026-09-01");
        receipt(&conn, "r1", &r);
        receipt(&conn, "r2", &r);
        assert_eq!(resolve(&conn, "r1", &r, 5).unwrap(), None);
        assert_eq!(resolve(&conn, "r2", &r, 5).unwrap(), None);
        // Both still see it as a candidate.
        assert!(candidates(&conn, "r1", &r).unwrap()[0].passes);
    }

    #[test]
    fn another_currency_is_only_a_suggestion() {
        let conn = conn();
        txn(&conn, "t1", 12.5, ms(2026, 9, 1), "Spar");
        let mut r = reading(12.5, "2026-09-01");
        r.currency = "HRK".into();
        receipt(&conn, "r1", &r);
        assert_eq!(resolve(&conn, "r1", &r, 5).unwrap(), None);
        let c = &candidates(&conn, "r1", &r).unwrap()[0];
        assert!(!c.exact_amount && !c.passes);
    }

    #[test]
    fn the_card_number_settles_a_shop_whose_name_differs() {
        let conn = conn();
        txn(
            &conn,
            "t1",
            9.99,
            ms(2026, 9, 2),
            "Paiement par carte X6510 PBZ*KIOSK 01/09",
        );
        txn(&conn, "t2", 9.99, ms(2026, 9, 3), "Prélèvement SFR");
        let r = reading(9.99, "2026-09-01");
        receipt(&conn, "r1", &r);
        // t2 posts two days after the receipt with no purchase day, so it is in
        // the window too: two pass, nothing is linked.
        assert_eq!(resolve(&conn, "r1", &r, 5).unwrap(), None);
        conn.execute("DELETE FROM transactions WHERE id = 't2'", [])
            .unwrap();
        assert_eq!(resolve(&conn, "r1", &r, 6).unwrap().as_deref(), Some("t1"));
    }

    #[test]
    fn a_cash_receipt_is_not_put_on_a_card_payment() {
        let conn = conn();
        txn(
            &conn,
            "t1",
            3.2,
            ms(2026, 9, 2),
            "Paiement par carte X6510 SPAR ZAGREB 01/09",
        );
        let mut r = reading(3.2, "2026-09-01");
        r.payment_method = Some("cash".into());
        r.card_last4 = None;
        receipt(&conn, "r1", &r);
        assert_eq!(resolve(&conn, "r1", &r, 5).unwrap(), None);
    }

    #[test]
    fn an_undone_link_is_not_made_again() {
        let conn = conn();
        txn(&conn, "t1", 7.63, ms(2026, 9, 3), "Lidl");
        let mut r = reading(7.63, "2026-09-03");
        r.merchant = "LIDL HRVATSKA d.o.o. k.d.".into();
        r.brand = Some("Lidl".into());
        receipt(&conn, "r1", &r);
        assert_eq!(resolve(&conn, "r1", &r, 5).unwrap().as_deref(), Some("t1"));
        let linked = effects::for_receipt(&conn, "r1").unwrap().pop().unwrap();
        effects::undo(&conn, &linked.id, 6).unwrap();
        assert_eq!(link_of(&conn, "r1"), (None, None));
        assert_eq!(resolve(&conn, "r1", &r, 7).unwrap(), None);
        assert!(candidates(&conn, "r1", &r).unwrap()[0].rejected);
    }

    #[test]
    fn a_receipt_on_its_own_draft_moves_to_the_real_transaction() {
        let conn = conn();
        let r = reading(4.0, "2026-09-01");
        receipt(&conn, "r1", &r);
        conn.execute(
            "INSERT INTO transactions (id, type, amount, currency_code, amount_base, rate_used, date, note, receipt_id, is_draft, created_at, updated_at)
             VALUES ('draft', 'expense', 4.0, 'EUR', 4.0, 1.0, ?1, 'Parsed from receipt', 'r1', 1, 1, 1)",
            params![ms(2026, 9, 1)],
        )
        .unwrap();
        conn.execute(
            "UPDATE receipts SET transaction_id = 'draft' WHERE id = 'r1'",
            [],
        )
        .unwrap();
        txn(&conn, "t1", 4.0, ms(2026, 9, 1), "Spar");
        assert_eq!(resolve(&conn, "r1", &r, 5).unwrap().as_deref(), Some("t1"));
        let deleted: Option<i64> = conn
            .query_row(
                "SELECT deleted_at FROM transactions WHERE id = 'draft'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deleted, Some(5));
    }

    #[test]
    fn a_manual_link_is_kept() {
        let conn = conn();
        txn(&conn, "t1", 4.0, ms(2026, 9, 1), "Spar");
        txn(&conn, "t2", 99.0, ms(2026, 9, 1), "Other");
        let r = reading(4.0, "2026-09-01");
        receipt(&conn, "r1", &r);
        crate::receipt_links::link(&conn, "r1", "t2", MANUAL, 5).unwrap();
        assert_eq!(resolve(&conn, "r1", &r, 6).unwrap().as_deref(), Some("t2"));
    }
}
