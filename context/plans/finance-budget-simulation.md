# Plan: Budget & Financial Simulation

**Status:** Proposed
**Depends on:** finance-expenses-income (for transaction data source)

## Summary
Build budget tracking on top of expense/income data, and integrate the existing Monte Carlo financial simulation tool into the CLI.

## Tasks

### T01 — Budget model and database tables
- Create `budgets` table: id, name, category, planned_amount, currency, period (weekly/monthly/yearly), start_date, end_date
- Create `budget_entries` table: id, budget_id, transaction_id (FK to transactions)
- Derive `Crud` for both models
- Add migrations in `database.rs`

### T02 — Budget CLI
- CLI: `budget create <name> <amount> <period> [category]`, `budget list`, `budget show <id>`, `budget delete <id>`
- Wire through socket to daemon CRUD handlers
- Show spent vs remaining when listing

### T03 — Auto-categorize transactions to budgets
- Add configurable rules: `{ "pattern": "carrefour|auchan", "budget": "groceries" }`
- When transactions are created, match against rules and auto-assign to budget
- CLI: `budget rules list|add|remove`

### T04 — Budget progress & alerts
- CLI: `budget progress [month]` — show all budgets with spent/planned/remaining
- Daemon notification when a budget exceeds configurable threshold (e.g., 80%, 100%)
- Weekly budget summary notification (optional)

### T05 — Integrate financial simulation into CLI
- Move `shared/src/finances/simulation.rs` into `finances/` crate as a library module
- Expose as CLI command: `sim run [--amount N] [--years N] [--return P] [--inflation P]`
- Keep the standalone binary target for direct use
- Output: summary stats + optional CSV

### T06 — Simulation scenario saving
- Add `simulation_scenarios` table: id, name, params (JSON), results (JSON), created_at
- CLI: `sim save <name>` (save last run), `sim list`, `sim show <name>`, `sim compare <a> <b>`
- Allows iterating on financial plans

### T07 — Validation & cleanup
- Test budget CLI commands end-to-end
- Test auto-categorization rules
- Test simulation runs with various parameters
- Verify notification triggers at thresholds

## Done checks
- Budget CRUD works through daemon
- Transactions auto-assign to budgets by configurable rules
- `budget progress` shows meaningful overspend/underspend per category
- `sim run` executes Monte Carlo simulation from CLI
- Scenarios can be saved and compared
