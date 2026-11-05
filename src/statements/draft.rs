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
/// it. `Trade` is informational: brokerage fills are shown in the review screen
/// but never become transactions.
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
    /// Identity within one statement. `UNIQUE(statement_id, fingerprint)`.
    pub fingerprint: String,
    /// Identity across overlapping statements. A re-import of the same line in
    /// a different file is stored as a `duplicate` rather than imported twice.
    pub dedupe_key: String,
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
