//! The normalized shape every statement source lowers into.
//!
//! Sources keep their own internal representation while parsing (a CA file has
//! sections and movements, a T212 sync has transactions, dividends and orders),
//! but they all produce [`StatementDraft`]s. That is what the store knows how to
//! persist, so idempotency, duplicate detection and account resolution live in
//! one place instead of once per provider.

use serde::{Deserialize, Serialize};

use super::SourceId;

/// What the source says the line is, independent of how the user will classify
/// it. `Trade` is informational for now: brokerage fills are shown in the
/// review screen and carry their cash amount and [`TradeDetails`], but are not
/// yet booked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Debit,
    Credit,
    TransferIn,
    TransferOut,
    Transfer,
    Fee,
    Interest,
    Dividend,
    Trade,
    Other,
}

impl LineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debit => "debit",
            Self::Credit => "credit",
            Self::TransferIn => "transfer_in",
            Self::TransferOut => "transfer_out",
            Self::Transfer => "transfer",
            Self::Fee => "fee",
            Self::Interest => "interest",
            Self::Dividend => "dividend",
            Self::Trade => "trade",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "debit" => Self::Debit,
            "credit" => Self::Credit,
            "transfer_in" => Self::TransferIn,
            "transfer_out" => Self::TransferOut,
            "transfer" => Self::Transfer,
            "fee" => Self::Fee,
            "interest" => Self::Interest,
            "dividend" => Self::Dividend,
            "trade" => Self::Trade,
            _ => Self::Other,
        }
    }

    /// Informational lines are visible in the review screen but are never
    /// imported, so they must not keep a statement stuck in `staged`.
    pub fn is_informational(self) -> bool {
        matches!(self, Self::Trade)
    }

    pub fn is_transfer(self) -> bool {
        matches!(self, Self::TransferIn | Self::TransferOut | Self::Transfer)
    }
}

/// The default transaction type the source suggests. Always overridable per
/// line at import time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedType {
    Expense,
    Income,
    Transfer,
}

impl SuggestedType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Expense => "expense",
            Self::Income => "income",
            Self::Transfer => "transfer",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "expense" => Some(Self::Expense),
            "income" => Some(Self::Income),
            "transfer" => Some(Self::Transfer),
            _ => None,
        }
    }
}

/// One statement header plus its lines. A CA file with three account sections
/// produces three drafts; a T212 sync produces one draft per month it touched.
#[derive(Clone, Debug)]
pub struct StatementDraft {
    pub source: SourceId,
    pub institution: String,
    pub account_name: String,
    pub account_number: Option<String>,
    pub currency_code: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub opening_balance: Option<f64>,
    pub closing_balance: Option<f64>,
    pub source_path: Option<String>,
    pub stored_path: Option<String>,
    pub source_hash: String,
    pub source_key: String,
    pub parsed: serde_json::Value,
    pub warnings: Vec<String>,
    pub lines: Vec<StatementLineDraft>,
}

#[derive(Clone, Debug)]
pub struct StatementLineDraft {
    /// Provider-side identity when it exists (T212 transaction id, order/fill
    /// id). CA rows do not have one, so they rely on the fingerprint.
    pub source_ref: Option<String>,
    pub posted_at: i64,
    pub description: String,
    /// Signed in the statement's own convention: debit negative, credit
    /// positive. `transactions.amount` is always positive, so import takes the
    /// absolute value.
    pub amount: f64,
    pub currency_code: String,
    pub balance_after: Option<f64>,
    pub category_hint: Option<String>,
    pub line_kind: LineKind,
    pub suggested_type: Option<SuggestedType>,
    pub suggested_category: Option<String>,
    pub metadata: serde_json::Value,
    /// What a `Trade` line bought or sold; `None` for every other kind. The
    /// line's `amount` is the cash the trade moved (negative for a buy,
    /// positive for a sale), so the trade can be booked as a movement.
    pub trade: Option<TradeDetails>,
    /// Identity within one statement. `UNIQUE(statement_id, fingerprint)`.
    pub fingerprint: String,
    /// Identity across overlapping statements. A re-import of the same line in
    /// a different file is stored as a `duplicate` rather than imported twice.
    pub dedupe_key: String,
}

/// Which way a trade went.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeSide {
    Buy,
    Sell,
}

impl TradeSide {
    /// The side a broker's action names (`Bought`, `Market sell`, `BUY`,
    /// `Reinvest`...), or `None` when it names neither.
    pub fn from_action(action: &str) -> Option<Self> {
        let lower = action.to_lowercase();
        if lower.contains("sell") || lower.contains("sold") {
            Some(Self::Sell)
        } else if ["buy", "bought", "reinvest"]
            .iter()
            .any(|needle| lower.contains(needle))
        {
            Some(Self::Buy)
        } else {
            None
        }
    }

    /// The sign a cash amount has for this side: a buy spends, a sale receives.
    pub fn cash_sign(self) -> f64 {
        match self {
            Self::Buy => -1.0,
            Self::Sell => 1.0,
        }
    }
}

/// The structured part of a trade line, kept so the trade can be booked and
/// the positions derived without re-reading the provider's raw row. The store
/// writes it into the line's `metadata_json` under `"trade"`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TradeDetails {
    pub side: TradeSide,
    pub symbol: String,
    /// Always positive: the side says which way the shares went.
    pub quantity: Option<f64>,
    pub price: Option<f64>,
}

impl TradeDetails {
    /// The cash a trade moved, signed by its side: the broker's figure when it
    /// gives one, otherwise quantity × price, otherwise zero. The broker's own
    /// sign is not trusted -- exports disagree on it -- the side is.
    pub fn cash_amount(&self, reported: Option<f64>) -> f64 {
        let magnitude = match reported {
            Some(value) if value != 0.0 => value.abs(),
            _ => match (self.quantity, self.price) {
                (Some(quantity), Some(price)) => (quantity * price).abs(),
                _ => 0.0,
            },
        };
        self.side.cash_sign() * magnitude
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOutcome {
    pub statement_id: String,
    pub source: String,
    pub account_name: String,
    pub account_number: Option<String>,
    pub inserted_lines: usize,
    pub duplicate_lines: usize,
    pub pending_lines: usize,
    pub informational_lines: usize,
    pub warnings: Vec<String>,
    /// Set when the exact same input was already ingested. The call is a no-op.
    pub skipped_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_side_is_read_from_every_broker_wording() {
        for action in ["Bought", "BUY", "Market buy", "Reinvest"] {
            assert_eq!(
                TradeSide::from_action(action),
                Some(TradeSide::Buy),
                "{action}"
            );
        }
        for action in ["Sold", "SELL", "Market sell"] {
            assert_eq!(
                TradeSide::from_action(action),
                Some(TradeSide::Sell),
                "{action}"
            );
        }
        assert_eq!(TradeSide::from_action("Dividend"), None);
    }

    /// Exports disagree on the sign of a trade's cash; the side decides it.
    #[test]
    fn a_trades_cash_is_signed_by_its_side() {
        let trade = |side| TradeDetails {
            side,
            symbol: "VOO".to_string(),
            quantity: Some(2.0),
            price: Some(100.0),
        };
        assert_eq!(trade(TradeSide::Buy).cash_amount(Some(201.5)), -201.5);
        assert_eq!(trade(TradeSide::Buy).cash_amount(Some(-201.5)), -201.5);
        assert_eq!(trade(TradeSide::Sell).cash_amount(Some(-199.0)), 199.0);
        assert_eq!(trade(TradeSide::Sell).cash_amount(None), 200.0);
        let unknown = TradeDetails {
            price: None,
            ..trade(TradeSide::Buy)
        };
        assert_eq!(unknown.cash_amount(Some(0.0)), 0.0);
    }
}
