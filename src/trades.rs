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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

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
