# Plan: Expense & Income Tracking

**Status:** Proposed
**Depends on:** system-daemon-wiring

## Summary
Build an expense and income tracking system on existing `Transaction`/`Investment` models. CLI for manual entry, file watchers for bank/investment statement parsing, and future API integration hooks.

## Tasks

### T01 — Create `finances` crate scaffold
- Create `finances/Cargo.toml` with `shared`, `clap` dependencies
- Add `finances` to workspace members
- Set up module structure: `cli/`, `daemon/`, `parsers/`
- Wire feature flags: `cli`, `daemon`
- Move `shared/src/finances/` models into `finances/src/models/`

### T02 — Transaction CLI
- CLI: `tx add <amount> <description> [type] [date]`, `tx list`, `tx delete <id>`, `tx summary [month]`
- Transaction type: Income, Expense, Transfer
- Wire through socket to daemon CRUD handlers
- Summary shows totals by type for a given period

### T03 — Investment CLI
- CLI: `invest add <name> <amount> <currency>`, `invest list`, `invest delete <id>`
- Wire through socket to daemon CRUD handlers

### T04 — Bank statement file parser
- Implement `handle_bank_file` in `src/watcher.rs` (currently a TODO stub)
- Start with CSV parser (configurable column mapping)
- Parse → create `Transaction` entries
- Config: `bank_statements_path`, CSV format config

### T05 — Investment statement parser
- Implement `handle_investment_file` (currently a TODO stub)
- Support firstrade CSV export format
- Parse → create `Investment` entries
- Config: `investments_statements_path`

### T06 — Receipt → expense integration
- When a receipt is processed (purchases pipeline), auto-create a `Transaction` entry for the total
- Link receipt to transaction via foreign key
- Category mapping: receipt store → expense category (configurable)

### T07 — API integration hooks
- Define a `StatementProvider` trait (polling-based)
- Implement stubs for common APIs (Trade212, Plaid)
- Store API credentials in config (encrypted or env vars)
- _Note: Actual API integration is a future task_

### T08 — Validation & cleanup
- Test all CLI commands end-to-end
- Test CSV bank statement parsing with sample files
- Test receipt → transaction auto-creation
- Verify database migrations are idempotent

## Done checks
- `tx add/list/delete/summary` works through daemon
- `invest add/list/delete` works through daemon
- Dropping a CSV bank statement in watched dir creates transactions
- Processing a receipt auto-creates an expense transaction
