//! Financial scenario simulations: deriving a spending/income baseline from
//! real transaction history, then projecting net worth forward under a named
//! scenario's assumptions.
//!
//! Everything in this module is pure and DB-agnostic on purpose. The server
//! builds its inputs from SQL rows (see `handlers::finance_scenarios`); the
//! desktop binary (`maia-ui`), which never has a connection to the server's
//! writer, instead asks the server over HTTP and never calls this module
//! directly. Keeping the math itself free of `rusqlite` means it is testable
//! with plain hand-built fixtures and have exactly one implementation shared
//! by both the HTTP handlers and their tests.

use std::collections::BTreeMap;

use chrono::{DateTime, Months, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Baseline derivation
// ---------------------------------------------------------------------------

/// The minimal shape the baseline needs from one `transactions` row.
///
/// `amount_base` is always a non-negative magnitude in the server's base
/// currency (see `statements::store::create_transaction`) — direction comes
/// entirely from `type_`, never from the sign of the amount.
#[derive(Debug, Clone)]
pub struct TransactionForBaseline {
    /// `"income"`, `"expense"` or `"transfer"`.
    pub type_: String,
    pub amount_base: f64,
    pub category: Option<String>,
    pub date_ms: i64,
}

/// Average monthly income and average monthly spend-by-category over a
/// trailing window, in the base currency.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    pub monthly_income: f64,
    /// Category -> average monthly spend. A category with no transactions in
    /// the window simply has no entry; a missing/blank category on a real
    /// expense row is folded into `"Uncategorized"` rather than dropped.
    pub category_monthly: BTreeMap<String, f64>,
    /// The trailing window this baseline was computed over, carried along so
    /// callers (and the UI) can show it rather than hide it — a baseline from
    /// three months of history is a different thing to trust than one from a
    /// full year, per the plan's own risk note.
    pub window_months: u32,
}

impl Baseline {
    /// Total average monthly expense across every category — the "no
    /// changes" monthly spend figure.
    pub fn total_monthly_expense(&self) -> f64 {
        self.category_monthly.values().sum()
    }
}

/// Category used for an expense transaction with no (or blank) category.
pub const UNCATEGORIZED: &str = "Uncategorized";

/// Derives a [`Baseline`] from transaction history.
///
/// The window is `window_months` calendar months immediately before
/// `now_ms`, and the average divides by the full window length rather than by
/// however many of those months actually had transactions — "average monthly
/// spend over the trailing window" reads most naturally as a window-wide
/// average, and it keeps a thin/seasonal history visibly thin (a near-empty
/// window produces a near-zero average) instead of letting a single busy
/// month stand in for a whole year.
///
/// Transfers are excluded: they move money between the user's own accounts
/// and are not income or spend.
pub fn derive_baseline(
    transactions: &[TransactionForBaseline],
    window_months: u32,
    now_ms: i64,
) -> Baseline {
    let window_months = window_months.max(1);
    let window_start_ms = DateTime::<Utc>::from_timestamp_millis(now_ms)
        .and_then(|dt| dt.checked_sub_months(Months::new(window_months)))
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(i64::MIN);

    let mut total_income = 0.0;
    let mut category_totals: BTreeMap<String, f64> = BTreeMap::new();

    for t in transactions {
        if t.date_ms < window_start_ms || t.date_ms > now_ms {
            continue;
        }
        match t.type_.as_str() {
            "income" => total_income += t.amount_base,
            "expense" => {
                let category = t
                    .category
                    .as_deref()
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .unwrap_or(UNCATEGORIZED)
                    .to_string();
                *category_totals.entry(category).or_insert(0.0) += t.amount_base;
            }
            // Transfers (and anything unrecognized) do not move the baseline.
            _ => {}
        }
    }

    let months = f64::from(window_months);
    Baseline {
        monthly_income: total_income / months,
        category_monthly: category_totals
            .into_iter()
            .map(|(category, total)| (category, total / months))
            .collect(),
        window_months,
    }
}

// ---------------------------------------------------------------------------
// Scenario assumptions
// ---------------------------------------------------------------------------

/// One category override: "this category's monthly cost becomes X", flatly
/// replacing whatever the baseline computed — not an adjustment on top of it.
/// Matches the relocation framing the feature is built around ("rent becomes
/// €1200 instead of €2000").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryOverride {
    pub category: String,
    pub monthly_amount: f64,
}

/// A scenario's full set of assumptions. This is exactly the shape stored as
/// `finance_scenarios.assumptions_json` (a JSON string, the same
/// store-as-text convention `receipts.parsed_json` uses) — so this struct is
/// both the wire format and the persisted format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assumptions {
    /// How many months forward to project.
    pub horizon_months: u32,
    /// Added to (or subtracted from) the baseline's average monthly income,
    /// from month 1 onward.
    #[serde(default)]
    pub income_delta_monthly: f64,
    /// Categories whose baseline monthly amount is replaced outright.
    #[serde(default)]
    pub category_overrides: Vec<CategoryOverride>,
    /// Annual inflation rate, as a percentage (e.g. `3.0` for 3%). Compounds
    /// monthly against expense categories *not* explicitly overridden.
    #[serde(default)]
    pub inflation_annual_pct: f64,
    /// Annual investment return, as a percentage. Compounds monthly against
    /// the running net worth balance, not any particular asset.
    #[serde(default)]
    pub investment_return_annual_pct: f64,
}

impl Assumptions {
    /// The "no changes" scenario: project the baseline forward unmodified
    /// over the given horizon. Used for the reference line in the chart.
    pub fn baseline_only(horizon_months: u32) -> Self {
        Assumptions {
            horizon_months,
            income_delta_monthly: 0.0,
            category_overrides: Vec::new(),
            inflation_annual_pct: 0.0,
            investment_return_annual_pct: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Projection
// ---------------------------------------------------------------------------

/// One simulated month's result. Month `0` is the starting point before any
/// simulated month has run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionPoint {
    pub month: u32,
    pub net_worth: f64,
    /// That month's total expense across every category, after overrides and
    /// any inflation growth applied so far — this is what "final monthly
    /// expense total" in the summary table reads off the last point.
    pub monthly_expense_total: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionSummary {
    pub ending_net_worth: f64,
    /// `ending_net_worth - starting_net_worth`: how much net worth the
    /// scenario adds (or removes) over the whole horizon, growth included.
    pub total_net_saved: f64,
    pub final_monthly_expense_total: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    pub series: Vec<ProjectionPoint>,
    pub summary: ProjectionSummary,
}

/// Runs a scenario's assumptions forward from `starting_net_worth`, month by
/// month, over `assumptions.horizon_months` months.
///
/// Semantics (deliberately simple — see the plan's non-goals: no tax, no
/// Monte Carlo, just compound growth):
/// - Category overrides and the income delta apply starting at month 1: the
///   first simulated month already reflects the scenario, it is not phased
///   in.
/// - Inflation (`annual_pct / 12` per month — an approximation of the
///   annual rate, not its exact monthly-equivalent, which is fine for a rough
///   planning tool) starts compounding *after* month 1, so a category's
///   month-1 amount is exactly its (possibly overridden) baseline value, and
///   growth shows up from month 2 onward. Overridden categories never grow:
///   they are assumed to already reflect their target-context cost.
/// - Investment return (`annual_pct / 12` per month) compounds the balance
///   carried over from the previous month, and that month's net cash flow
///   (income minus total expense) is added after growth is applied — i.e.
///   money saved during a month has not itself earned a month of return yet.
pub fn project(baseline: &Baseline, assumptions: &Assumptions, starting_net_worth: f64) -> Projection {
    let inflation_monthly = assumptions.inflation_annual_pct / 100.0 / 12.0;
    let investment_monthly = assumptions.investment_return_annual_pct / 100.0 / 12.0;

    let overridden: std::collections::HashSet<&str> = assumptions
        .category_overrides
        .iter()
        .map(|o| o.category.as_str())
        .collect();

    // Overrides replace the baseline outright; a category named in an
    // override that the baseline never saw is simply added.
    let mut categories: BTreeMap<String, f64> = baseline.category_monthly.clone();
    for o in &assumptions.category_overrides {
        categories.insert(o.category.clone(), o.monthly_amount);
    }

    let monthly_income = baseline.monthly_income + assumptions.income_delta_monthly;

    let mut net_worth = starting_net_worth;
    let mut series = Vec::with_capacity(assumptions.horizon_months as usize + 1);
    series.push(ProjectionPoint {
        month: 0,
        net_worth,
        monthly_expense_total: categories.values().sum(),
    });

    for month in 1..=assumptions.horizon_months {
        let expense_total: f64 = categories.values().sum();
        let net_change = monthly_income - expense_total;
        net_worth = net_worth * (1.0 + investment_monthly) + net_change;
        series.push(ProjectionPoint {
            month,
            net_worth,
            monthly_expense_total: expense_total,
        });

        // Grow non-overridden categories for next month.
        for (category, amount) in categories.iter_mut() {
            if !overridden.contains(category.as_str()) {
                *amount *= 1.0 + inflation_monthly;
            }
        }
    }

    let ending_net_worth = net_worth;
    let final_monthly_expense_total = series
        .last()
        .map(|p| p.monthly_expense_total)
        .unwrap_or(0.0);

    Projection {
        series,
        summary: ProjectionSummary {
            ending_net_worth,
            total_net_saved: ending_net_worth - starting_net_worth,
            final_monthly_expense_total,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(type_: &str, amount_base: f64, category: Option<&str>, date_ms: i64) -> TransactionForBaseline {
        TransactionForBaseline {
            type_: type_.to_string(),
            amount_base,
            category: category.map(str::to_string),
            date_ms,
        }
    }

    const DAY_MS: i64 = 86_400_000;
    const MONTH_MS: i64 = 30 * DAY_MS;

    #[test]
    fn baseline_averages_income_and_category_spend_over_the_window() {
        // "Now" is day 0; three trailing months of history, spaced a month
        // apart so each lands cleanly inside a 3-month window.
        let now_ms = 100 * MONTH_MS;
        let transactions = vec![
            tx("income", 1000.0, None, now_ms - 0 * MONTH_MS),
            tx("income", 1000.0, None, now_ms - 1 * MONTH_MS),
            tx("income", 1000.0, None, now_ms - 2 * MONTH_MS),
            tx("expense", 500.0, Some("rent"), now_ms - 0 * MONTH_MS),
            tx("expense", 500.0, Some("rent"), now_ms - 1 * MONTH_MS),
            tx("expense", 500.0, Some("rent"), now_ms - 2 * MONTH_MS),
            tx("expense", 100.0, Some("groceries"), now_ms - 0 * MONTH_MS),
            tx("expense", 200.0, Some("groceries"), now_ms - 1 * MONTH_MS),
            tx("expense", 300.0, Some("groceries"), now_ms - 2 * MONTH_MS),
            // A transfer between the user's own accounts must not move either
            // the income or the expense side of the baseline.
            tx("transfer", 9999.0, Some("rent"), now_ms),
        ];

        let baseline = derive_baseline(&transactions, 3, now_ms);

        assert_eq!(baseline.window_months, 3);
        assert!((baseline.monthly_income - 1000.0).abs() < 1e-9);
        assert!((baseline.category_monthly["rent"] - 500.0).abs() < 1e-9);
        assert!((baseline.category_monthly["groceries"] - 200.0).abs() < 1e-9);
        assert!((baseline.total_monthly_expense() - 700.0).abs() < 1e-9);
    }

    #[test]
    fn baseline_excludes_transactions_outside_the_trailing_window() {
        let now_ms = 100 * MONTH_MS;
        let transactions = vec![
            tx("income", 1000.0, None, now_ms),
            // Four months back is outside a 3-month window.
            tx("income", 5000.0, None, now_ms - 4 * MONTH_MS),
            tx("expense", 300.0, Some("rent"), now_ms),
            tx("expense", 9000.0, Some("rent"), now_ms - 4 * MONTH_MS),
        ];

        let baseline = derive_baseline(&transactions, 3, now_ms);

        assert!((baseline.monthly_income - 1000.0 / 3.0).abs() < 1e-9);
        assert!((baseline.category_monthly["rent"] - 300.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn baseline_folds_blank_category_into_uncategorized() {
        let now_ms = 100 * MONTH_MS;
        let transactions = vec![
            tx("expense", 100.0, None, now_ms),
            tx("expense", 50.0, Some(""), now_ms),
            tx("expense", 50.0, Some("   "), now_ms),
        ];

        let baseline = derive_baseline(&transactions, 1, now_ms);

        assert_eq!(baseline.category_monthly.len(), 1);
        assert!((baseline.category_monthly[UNCATEGORIZED] - 200.0).abs() < 1e-9);
    }

    /// Hand-computed: starting net worth 10,000; monthly income 3,000; one
    /// category overridden (rent 2,000 -> 1,200), groceries left at its
    /// baseline 500; no inflation; investment return 12%/yr (1%/mo).
    ///
    /// Each month: expense = 1,200 + 500 = 1,700 (flat, since inflation is 0
    /// and rent is overridden); net_worth = net_worth * 1.01 + (3000 - 1700).
    ///   month 1: 10000 * 1.01 + 1300 = 11400
    ///   month 2: 11400 * 1.01 + 1300 = 12814
    ///   month 3: 12814 * 1.01 + 1300 = 14242.14
    #[test]
    fn projection_matches_a_hand_computed_relocation_scenario() {
        let mut category_monthly = BTreeMap::new();
        category_monthly.insert("rent".to_string(), 2000.0);
        category_monthly.insert("groceries".to_string(), 500.0);
        let baseline = Baseline {
            monthly_income: 3000.0,
            category_monthly,
            window_months: 12,
        };
        let assumptions = Assumptions {
            horizon_months: 3,
            income_delta_monthly: 0.0,
            category_overrides: vec![CategoryOverride {
                category: "rent".to_string(),
                monthly_amount: 1200.0,
            }],
            inflation_annual_pct: 0.0,
            investment_return_annual_pct: 12.0,
        };

        let projection = project(&baseline, &assumptions, 10_000.0);

        assert_eq!(projection.series.len(), 4); // month 0..=3
        assert!((projection.series[0].net_worth - 10_000.0).abs() < 1e-9);
        assert!((projection.series[1].net_worth - 11_400.0).abs() < 1e-6);
        assert!((projection.series[2].net_worth - 12_814.0).abs() < 1e-6);
        assert!((projection.series[3].net_worth - 14_242.14).abs() < 1e-4);
        for point in &projection.series[1..] {
            assert!((point.monthly_expense_total - 1700.0).abs() < 1e-9);
        }
        assert!((projection.summary.ending_net_worth - 14_242.14).abs() < 1e-4);
        assert!((projection.summary.total_net_saved - 4_242.14).abs() < 1e-4);
        assert!((projection.summary.final_monthly_expense_total - 1700.0).abs() < 1e-9);
    }

    /// Hand-computed inflation case, no investment return: baseline
    /// groceries 200/mo, not overridden, inflation 12%/yr (1%/mo). Income
    /// delta -500 (lost income). No other categories.
    ///   month 1 expense = 200.00, net_worth = 0 + (2500 - 200) = 2300
    ///   month 2 expense = 202.00 (200 * 1.01), net_worth = 2300 + 2298 = 4598
    #[test]
    fn inflation_only_compounds_non_overridden_categories_from_month_two() {
        let mut category_monthly = BTreeMap::new();
        category_monthly.insert("groceries".to_string(), 200.0);
        let baseline = Baseline {
            monthly_income: 3000.0,
            category_monthly,
            window_months: 12,
        };
        let assumptions = Assumptions {
            horizon_months: 2,
            income_delta_monthly: -500.0,
            category_overrides: Vec::new(),
            inflation_annual_pct: 12.0,
            investment_return_annual_pct: 0.0,
        };

        let projection = project(&baseline, &assumptions, 0.0);

        assert!((projection.series[1].monthly_expense_total - 200.0).abs() < 1e-9);
        assert!((projection.series[1].net_worth - 2300.0).abs() < 1e-6);
        assert!((projection.series[2].monthly_expense_total - 202.0).abs() < 1e-6);
        assert!((projection.series[2].net_worth - 4598.0).abs() < 1e-4);
    }

    #[test]
    fn an_override_for_a_category_absent_from_the_baseline_is_simply_added() {
        let baseline = Baseline {
            monthly_income: 1000.0,
            category_monthly: BTreeMap::new(),
            window_months: 12,
        };
        let assumptions = Assumptions {
            horizon_months: 1,
            income_delta_monthly: 0.0,
            category_overrides: vec![CategoryOverride {
                category: "new_rent".to_string(),
                monthly_amount: 400.0,
            }],
            inflation_annual_pct: 0.0,
            investment_return_annual_pct: 0.0,
        };

        let projection = project(&baseline, &assumptions, 0.0);

        assert!((projection.series[1].monthly_expense_total - 400.0).abs() < 1e-9);
        assert!((projection.series[1].net_worth - 600.0).abs() < 1e-9);
    }

    #[test]
    fn the_baseline_only_scenario_is_truly_unmodified() {
        let assumptions = Assumptions::baseline_only(6);
        assert_eq!(assumptions.horizon_months, 6);
        assert_eq!(assumptions.income_delta_monthly, 0.0);
        assert!(assumptions.category_overrides.is_empty());
        assert_eq!(assumptions.inflation_annual_pct, 0.0);
        assert_eq!(assumptions.investment_return_annual_pct, 0.0);
    }
}
