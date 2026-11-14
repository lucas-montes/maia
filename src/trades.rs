//! Trades: what a booked transfer between a broker's cash and securities
//! accounts bought or sold.
//!
//! The transfer is an ordinary transaction, at the cash the trade moved; the
//! trade's details live beside it in the server-only `trades` table, keyed by
//! the transaction. A transaction with no row here is a movement. Positions
//! are derived from these rows, never stored.

use rusqlite::{Connection, OptionalExtension, params};

use crate::statements::draft::{TradeDetails, TradeSide};

impl TradeSide {
    /// The value stored in `trades.side`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "buy" => Some(Self::Buy),
            "sell" => Some(Self::Sell),
            _ => None,
        }
    }
}

/// Records what `transaction_id` traded. Booking the same line again rewrites
/// the row rather than failing, the way the booked transaction itself is
/// rewritten.
pub fn record_trade(
    conn: &Connection,
    transaction_id: &str,
    trade: &TradeDetails,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO trades
            (transaction_id, side, symbol, quantity, price, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(transaction_id) DO UPDATE SET
            side=excluded.side, symbol=excluded.symbol,
            quantity=excluded.quantity, price=excluded.price,
            updated_at=excluded.updated_at",
        params![
            transaction_id,
            trade.side.as_str(),
            trade.symbol,
            trade.quantity,
            trade.price,
            now
        ],
    )?;
    Ok(())
}

/// What `transaction_id` traded, or `None` when it is a movement.
pub fn trade_of(conn: &Connection, transaction_id: &str) -> rusqlite::Result<Option<TradeDetails>> {
    conn.query_row(
        "SELECT side, symbol, quantity, price FROM trades WHERE transaction_id=?1",
        params![transaction_id],
        |row| {
            let side: String = row.get(0)?;
            let symbol: String = row.get(1)?;
            let quantity: Option<f64> = row.get(2)?;
            let price: Option<f64> = row.get(3)?;
            // The table's CHECK keeps `side` to the two known values.
            Ok(TradeSide::parse(&side).map(|side| TradeDetails {
                side,
                symbol,
                quantity,
                price,
            }))
        },
    )
    .optional()
    .map(Option::flatten)
}

/// What a securities account held before the trades the statements show, as
/// the user stated it: the holding at the end of `as_of`, and what it cost.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningPosition {
    pub quantity: f64,
    /// The total paid for `quantity`, not a price per share.
    pub cost_basis: f64,
    /// Epoch milliseconds. Trades dated after this day count; the ones on or
    /// before it are already in the holding.
    pub as_of: i64,
}

/// One holding of one securities account, derived from its trades at
/// average cost.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub account_id: String,
    pub account_name: String,
    pub symbol: String,
    /// The currency of the trades' cash, when there were any.
    pub currency_code: Option<String>,
    /// Shares held now; zero for a position that was sold out, negative when
    /// the trades sold more than they and the opening position account for.
    pub quantity: f64,
    /// What the shares held now cost. `None` once a sale took shares nothing
    /// says the cost of -- more than were held, or a trade with no quantity --
    /// until an opening position says what was there.
    pub cost_basis: Option<f64>,
    pub average_price: Option<f64>,
    /// Proceeds of the sales less the average cost of the shares sold. Not
    /// income: it is shown beside the position, never booked.
    pub realised_gain: Option<f64>,
    /// The trades counted, which excludes those on or before the opening
    /// position's day.
    pub trades: usize,
    pub opening: Option<OpeningPosition>,
}

/// Below this a quantity is zero: share counts come from floats, and a
/// position sold out must not show 1e-15 shares at an absurd average price.
const EPSILON: f64 = 1e-9;

/// Every position of every live securities account, from the live trades
/// and the opening positions. Ordered by account, then symbol.
///
/// Average cost: a buy adds its cash to the cost; a sale takes the average
/// cost of the shares it sold off the cost, and the difference between that
/// and its proceeds is the realised gain. A trade's cash is the transfer's
/// amount, so fees are in the cost of a buy and out of the proceeds of a
/// sale.
pub fn positions(conn: &Connection) -> rusqlite::Result<Vec<Position>> {
    use std::collections::BTreeMap;

    let mut by_key: BTreeMap<(String, String), Position> = BTreeMap::new();

    let mut stmt = conn.prepare(
        "SELECT o.account_id, a.name, o.symbol, o.quantity, o.cost_basis, o.as_of
         FROM opening_positions o JOIN accounts a ON a.id = o.account_id
         WHERE a.deleted_at IS NULL",
    )?;
    let openings = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            OpeningPosition {
                quantity: r.get(3)?,
                cost_basis: r.get(4)?,
                as_of: r.get(5)?,
            },
        ))
    })?;
    for opening in openings {
        let (account_id, account_name, symbol, opening) = opening?;
        by_key.insert(
            (account_id.clone(), symbol.clone()),
            Position {
                account_id,
                account_name,
                symbol,
                currency_code: None,
                quantity: opening.quantity,
                cost_basis: Some(opening.cost_basis),
                average_price: None,
                realised_gain: Some(0.0),
                trades: 0,
                opening: Some(opening),
            },
        );
    }

    // The securities account is the side the shares are in: where a buy's
    // cash went, where a sale's came from. A buy before a sale on the same
    // day, so a day's round trip does not sell shares it has not bought yet.
    let mut stmt = conn.prepare(
        "SELECT a.id, a.name, tr.symbol, tr.side, tr.quantity, t.amount,
                t.currency_code, t.date
         FROM trades tr
         JOIN transactions t ON t.id = tr.transaction_id
         JOIN accounts a ON a.id =
             CASE tr.side WHEN 'buy' THEN t.to_account_id ELSE t.account_id END
         WHERE t.deleted_at IS NULL AND a.deleted_at IS NULL
         ORDER BY t.date, tr.side = 'sell', t.id",
    )?;
    let trades = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<f64>>(4)?,
            r.get::<_, f64>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, i64>(7)?,
        ))
    })?;
    for trade in trades {
        let (account_id, account_name, symbol, side, quantity, cash, currency, date) = trade?;
        let position = by_key
            .entry((account_id.clone(), symbol.clone()))
            .or_insert_with(|| Position {
                account_id,
                account_name,
                symbol,
                currency_code: None,
                quantity: 0.0,
                cost_basis: Some(0.0),
                average_price: None,
                realised_gain: Some(0.0),
                trades: 0,
                opening: None,
            });
        if position
            .opening
            .as_ref()
            .is_some_and(|o| date <= end_of_day(o.as_of))
        {
            continue;
        }
        position.trades += 1;
        position.currency_code.get_or_insert(currency);
        let cash = cash.abs();
        let Some(quantity) = quantity.map(f64::abs) else {
            // A trade with no share count: the holding is no longer known.
            position.cost_basis = None;
            position.realised_gain = None;
            continue;
        };
        match TradeSide::parse(&side) {
            Some(TradeSide::Buy) => {
                position.quantity += quantity;
                position.cost_basis = position.cost_basis.map(|cost| cost + cash);
            }
            Some(TradeSide::Sell) => {
                let held = position.quantity;
                match position.cost_basis {
                    Some(cost) if held + EPSILON >= quantity && held > EPSILON => {
                        let sold_cost = cost / held * quantity;
                        position.cost_basis = Some(cost - sold_cost);
                        position.realised_gain =
                            position.realised_gain.map(|gain| gain + cash - sold_cost);
                    }
                    // More shares sold than anything says were held: what the
                    // rest cost, and so what the sale gained, is unknown.
                    _ => {
                        position.cost_basis = None;
                        position.realised_gain = None;
                    }
                }
                position.quantity -= quantity;
            }
            None => {}
        }
        if position.quantity.abs() < EPSILON {
            position.quantity = 0.0;
            if position.cost_basis.is_some() {
                position.cost_basis = Some(0.0);
            }
        }
    }

    Ok(by_key
        .into_values()
        .map(|mut position| {
            position.average_price = match position.cost_basis {
                Some(cost) if position.quantity > EPSILON => Some(cost / position.quantity),
                _ => None,
            };
            position
        })
        .collect())
}

/// The last millisecond of the UTC day `ms` falls in: an opening position is
/// the holding at the end of its day.
fn end_of_day(ms: i64) -> i64 {
    const DAY: i64 = 86_400_000;
    ms.div_euclid(DAY) * DAY + DAY - 1
}

/// States what a securities account held of `symbol` before its trades,
/// replacing what was stated before.
pub fn set_opening_position(
    conn: &Connection,
    account_id: &str,
    symbol: &str,
    opening: &OpeningPosition,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO opening_positions
            (account_id, symbol, quantity, cost_basis, as_of, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(account_id, symbol) DO UPDATE SET
            quantity=excluded.quantity, cost_basis=excluded.cost_basis,
            as_of=excluded.as_of, updated_at=excluded.updated_at",
        params![
            account_id,
            symbol,
            opening.quantity,
            opening.cost_basis,
            opening.as_of,
            now
        ],
    )?;
    Ok(())
}

/// Removes a stated opening position; returns whether there was one.
pub fn clear_opening_position(
    conn: &Connection,
    account_id: &str,
    symbol: &str,
) -> rusqlite::Result<bool> {
    let removed = conn.execute(
        "DELETE FROM opening_positions WHERE account_id=?1 AND symbol=?2",
        params![account_id, symbol],
    )?;
    Ok(removed > 0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const DAY: i64 = 86_400_000;

    /// A broker with a cash and a securities account.
    fn broker(conn: &Connection) {
        for (id, name, type_) in [
            ("cash", "Firstrade", "cash"),
            ("sec", "Firstrade Securities", "securities"),
        ] {
            conn.execute(
                "INSERT INTO accounts (id, name, type, opening_balance, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 0.0, 1, 1)",
                params![id, name, type_],
            )
            .unwrap();
        }
    }

    /// Books a trade the way the import does: a transfer between the two
    /// accounts at the cash it moved, with its details beside it.
    fn book(
        conn: &Connection,
        id: &str,
        day: i64,
        side: TradeSide,
        symbol: &str,
        quantity: f64,
        cash: f64,
    ) {
        let (from, to) = match side {
            TradeSide::Buy => ("cash", "sec"),
            TradeSide::Sell => ("sec", "cash"),
        };
        conn.execute(
            "INSERT INTO transactions
                (id, type, amount, currency_code, amount_base, rate_used, account_id,
                 to_account_id, date, created_at)
             VALUES (?1, 'transfer', ?2, 'USD', ?2, 1.0, ?3, ?4, ?5, 1)",
            params![id, cash, from, to, day * DAY],
        )
        .unwrap();
        let trade = TradeDetails {
            side,
            symbol: symbol.to_string(),
            quantity: Some(quantity),
            price: Some(cash / quantity),
        };
        record_trade(conn, id, &trade, 1).unwrap();
    }

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-6)
    }

    #[test]
    fn a_position_is_held_at_average_cost_and_a_sale_realises_against_it() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        broker(&conn);
        book(&conn, "b1", 1, TradeSide::Buy, "VOO", 2.0, 200.0);
        book(&conn, "b2", 2, TradeSide::Buy, "VOO", 1.0, 130.0);
        book(&conn, "s1", 3, TradeSide::Sell, "VOO", 1.0, 150.0);

        let positions = positions(&conn).unwrap();
        assert_eq!(positions.len(), 1);
        let voo = &positions[0];
        assert_eq!(voo.account_name, "Firstrade Securities");
        assert_eq!(voo.quantity, 2.0);
        assert!(close(voo.cost_basis, 220.0), "{voo:?}");
        assert!(close(voo.average_price, 110.0), "{voo:?}");
        assert!(close(voo.realised_gain, 40.0), "{voo:?}");
        assert_eq!(voo.trades, 3);
        assert_eq!(voo.currency_code.as_deref(), Some("USD"));
    }

    /// A position sold out keeps its gain and shows no shares, not float dust
    /// at an absurd average price.
    #[test]
    fn a_position_sold_out_shows_its_gain_and_nothing_held() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        broker(&conn);
        book(&conn, "b1", 1, TradeSide::Buy, "NVDA", 3.0, 355.23);
        book(&conn, "s1", 2, TradeSide::Sell, "NVDA", 3.0, 359.75);
        let nvda = &positions(&conn).unwrap()[0];
        assert_eq!(nvda.quantity, 0.0);
        assert_eq!(nvda.cost_basis, Some(0.0));
        assert_eq!(nvda.average_price, None);
        assert!(close(nvda.realised_gain, 4.52), "{nvda:?}");
    }

    /// The export starts after the first purchase, so a sale takes shares no
    /// trade bought: what they cost is unknown, and says so, rather than a
    /// gain computed from nothing.
    #[test]
    fn selling_shares_no_trade_bought_leaves_the_cost_unknown() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        broker(&conn);
        book(&conn, "s1", 1, TradeSide::Sell, "INTC", 60.0, 2_636.47);
        book(&conn, "b1", 2, TradeSide::Buy, "INTC", 25.0, 533.20);
        let intc = &positions(&conn).unwrap()[0];
        assert_eq!(intc.quantity, -35.0);
        assert_eq!(intc.cost_basis, None);
        assert_eq!(intc.realised_gain, None);
        assert_eq!(intc.average_price, None);
    }

    /// An opening position says what was there: the sale realises against it,
    /// and the trades on or before its day are already in it.
    #[test]
    fn an_opening_position_supplies_what_the_trades_do_not() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        broker(&conn);
        book(&conn, "old", 1, TradeSide::Buy, "INTC", 100.0, 2_000.0);
        book(&conn, "s1", 3, TradeSide::Sell, "INTC", 60.0, 2_636.47);
        book(&conn, "b1", 4, TradeSide::Buy, "INTC", 25.0, 533.20);
        let opening = OpeningPosition {
            quantity: 100.0,
            cost_basis: 2_000.0,
            as_of: DAY + 5,
        };
        set_opening_position(&conn, "sec", "INTC", &opening, 1).unwrap();

        let intc = &positions(&conn).unwrap()[0];
        assert_eq!(intc.trades, 2, "the buy on the opening's day is in it");
        assert_eq!(intc.quantity, 65.0);
        assert!(close(intc.realised_gain, 2_636.47 - 1_200.0), "{intc:?}");
        assert!(close(intc.cost_basis, 800.0 + 533.20), "{intc:?}");
        assert_eq!(intc.opening, Some(opening));

        assert!(clear_opening_position(&conn, "sec", "INTC").unwrap());
        assert_eq!(positions(&conn).unwrap()[0].opening, None);
        assert!(!clear_opening_position(&conn, "sec", "INTC").unwrap());
    }

    /// A trade the user deleted is not money that moved.
    #[test]
    fn a_deleted_trade_is_not_counted() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        broker(&conn);
        book(&conn, "b1", 1, TradeSide::Buy, "VOO", 1.0, 500.0);
        book(&conn, "b2", 2, TradeSide::Buy, "VOO", 1.0, 520.0);
        conn.execute("UPDATE transactions SET deleted_at=5 WHERE id='b2'", [])
            .unwrap();
        let voo = &positions(&conn).unwrap()[0];
        assert_eq!(voo.quantity, 1.0);
        assert_eq!(voo.trades, 1);
    }
    fn transaction(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO transactions
                (id, type, amount, currency_code, amount_base, rate_used, date, created_at)
             VALUES (?1, 'transfer', 696.72, 'USD', 696.72, 1.0, 1, 1)",
            params![id],
        )
        .unwrap();
    }

    fn voo() -> TradeDetails {
        TradeDetails {
            side: TradeSide::Buy,
            symbol: "VOO".to_string(),
            quantity: Some(1.0),
            price: Some(696.7199),
        }
    }

    #[test]
    fn a_transaction_with_no_trade_is_a_movement() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        transaction(&conn, "t1");
        assert_eq!(trade_of(&conn, "t1").unwrap(), None);
    }

    #[test]
    fn a_recorded_trade_is_read_back_and_rewritten_on_rebooking() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        transaction(&conn, "t1");
        record_trade(&conn, "t1", &voo(), 10).unwrap();
        assert_eq!(trade_of(&conn, "t1").unwrap(), Some(voo()));

        let corrected = TradeDetails {
            quantity: Some(2.0),
            ..voo()
        };
        record_trade(&conn, "t1", &corrected, 20).unwrap();
        assert_eq!(trade_of(&conn, "t1").unwrap(), Some(corrected));
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM trades", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    /// The trade belongs to its transaction: removing the transaction removes
    /// it, so no trade is left describing money that is not there.
    #[test]
    fn a_trade_goes_with_its_transaction() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        transaction(&conn, "t1");
        record_trade(&conn, "t1", &voo(), 10).unwrap();
        conn.execute("DELETE FROM transactions WHERE id='t1'", [])
            .unwrap();
        assert_eq!(trade_of(&conn, "t1").unwrap(), None);
    }

    /// A trade cannot describe a transaction that does not exist, nor a side
    /// the positions would not know how to count.
    #[test]
    fn the_table_refuses_an_orphan_or_an_unknown_side() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        assert!(record_trade(&conn, "missing", &voo(), 10).is_err());
        transaction(&conn, "t1");
        assert!(
            conn.execute(
                "INSERT INTO trades (transaction_id, side, symbol, created_at)
                 VALUES ('t1', 'short', 'VOO', 1)",
                [],
            )
            .is_err()
        );
    }
}
