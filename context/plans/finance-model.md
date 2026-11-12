# Finance model: accounts as a tree, trades as movements, statements re-derived

> Server + desktop only; mobile later. Replaces the 2026-11-11 long version of this plan and
> `statements-reimport-after-discard.md`.

## The problems
1. **A discarded statement still claims its entries.** Staging marks an entry `duplicate` if its
   `dedupe_key` exists in *any* other statement — including discarded ones
   (`src/statements/store.rs:148-156`). Re-dropping a discarded file therefore stages every entry as
   `duplicate`: no checkboxes, nothing to import, and the file is moved to `parsed/` as if imported.
   The `duplicate` verdict also never goes away: re-staging only clears `pending`/`informational`
   (`store.rs:99-105`), and an unchanged file hits the "already ingested" no-op (`store.rs:55-69`).
   *Live DB was repaired by hand on 2026-11-11 (51 Firstrade entries pending again); the code is
   unchanged, so the trap is still armed.*
2. **Accounts are flat and mix two axes.** `type ∈ {bank, savings, investment, other}` mixes the
   institution kind and the product kind, and there is no "Crédit Agricole contains Eko, Livret A,
   LDD" or "Firstrade contains Cash + Securities".
3. **Trades are thrown away.** The parser keeps symbol/side/qty/price in metadata but zeroes the
   amount (`firstrade.rs:96`), and the review shows trades as "informational, never imported".

## Rules
- **Entry status:** `imported`/`skipped` are user decisions and survive everything.
  `pending`/`informational`/`duplicate`/`error` are derived and are recomputed on every re-read.
- **Only live statements claim content:** discarded (`deleted_at`) and `ignored` statements never
  make another entry a duplicate.
- **`parsed/` means imported:** a file moves there only when every entry is imported or skipped.
- **Accounts are two levels:** a root `account_roots(id, name, kind ∈ bank|broker|neobank, source)`
  and leaves `accounts.root_id → account_roots ON DELETE CASCADE`. `accounts.type` keeps its name,
  with new values `cash | savings | securities`. Transactions attach to leaves; a root's balance is
  the sum of its leaves. Roots are server-only (never pushed to or pulled by the phone).
- **The identity stays on the leaf:** `(source, external_id)` + `idx_accounts_external` are
  unchanged. A leaf's source must equal its root's. The securities leaf has no external id; it is
  found as "the securities leaf of this root".
- **The phone push stops writing `type`** (`handlers/push.rs:1456`), otherwise a sync reverts the
  rename. The pull still sends it.
- **A trade is a movement, not income or expense.** A gain on a sale is not income; dividends and
  interest are.

## Decision — a trade books its cash leg; positions are derived
Decided 2026-11-12. The rejected option booked a trade as one movement with **two amounts**
(`amount` leaving cash, `to_amount` entering securities at book value) and refused to book a sale
until its cost basis was known, so the INTC sales would have waited on an opening-position seed.

Chosen: book the **cash leg as a fact** and derive everything else.
- Each trade becomes one transfer between the cash leaf and the securities leaf, at the cash amount
  the broker reports. One amount, no `to_amount`, nothing to refuse, so every Firstrade trade can be
  imported today.
- The trade details go in a server-only `trades` table (`transaction_id, symbol, side, quantity,
  price`), or as columns on `transactions` plus a `kind = 'trade'` discriminator.
- Positions (quantity, average cost, realised gain) are **derived** from the trades and an optional
  `opening_positions` seed. The securities leaf's balance is shown from the positions (at cost),
  not from summing its transfers.
- Missing cost basis (INTC) affects only the positions/gain display, which says "unknown", and
  never blocks the import.

## Tasks
### Stage 1 — unblock statements (small, do first)
- [x] T1 Duplicate lookup joins `bank_statements` and requires `deleted_at IS NULL AND status <> 'ignored'` (status:done)
  - Completed: 2026-11-12
  - Goal: only a live statement claims content, so a discarded or ignored statement's lines never turn a fresh entry into a `duplicate`.
  - Boundaries: In — the duplicate lookup in `src/statements/store.rs`, its `save_statement` doc comment. Out — re-staging semantics (T2), the "already ingested" no-op (T3), repairing rows already stored as `duplicate` (T6), schema, UI.
  - Note: the lookup is the only one in the codebase. `preview.rs::describe_duplicates` reads already-stored `duplicate_of` links and does no lookup.

- [x] T2 Re-stage deletes `pending, informational, duplicate, error` lines (cascades to
      `transfer_matches`, which `match_pending` rebuilds right after) (status:done)
  - Completed: 2026-11-12
  - Goal: a re-read recomputes every derived verdict instead of keeping the stale one.
  - Boundaries: In — the re-stage `DELETE` in `src/statements/store.rs` and its doc comment. Out — the "already ingested" no-op (T3), `collect_file_states` (T4), `error` lines being selectable (T5), the v12 repair migration (T6).
  - Note: `imported`/`skipped` lines are never deleted, so user decisions survive. `transfer_matches` cascades on the line delete and `match_pending` runs right after the batch (`statements/mod.rs`), so matches rebuild themselves.

- [ ] T3 "Already ingested" no-op only when the statement has a pending/imported/skipped entry;
      otherwise re-read. Must stay a no-op for unchanged T212 feed months.
- [ ] T4 `collect_file_states` (`watch.rs`): a file is imported only when every entry is
      imported/skipped.
- [ ] T5 `error` entries are selectable and counted (`statementReview.ts`, `handlers/statements.rs:61`).
- [ ] T6 Migration v12: `duplicate` entries whose `duplicate_of` is in a discarded statement →
      `pending`, clear `duplicate_of`.
- [ ] T7 Review explains an empty state ("every entry duplicates another file — re-drop to re-read").
- [ ] T8 Firstrade parser warns on an unreadable date row instead of `continue` (`firstrade.rs:273`);
      the statement records rows read / kept / skipped and the review shows it.

### Stage 2 — account tree (migration v13)
- [ ] T9  `account_roots` + `accounts.root_id`; map `bank→cash, investment→cash, other→cash`;
      create "Crédit Agricole" (bank) and re-parent the 4 existing accounts.
- [ ] T10 Statement import resolves/creates the root by source (Firstrade → broker root with a
      `cash` leaf); `default_account_type` becomes the kind rule (CA: livret/ldd/épargne → savings).
- [ ] T11 Identity endpoint refuses a leaf whose source differs from its root's.
- [ ] T12 Phone push stops writing `type`.
- [ ] T13 Desktop accounts view: roots with leaves under them, root total, type editor offers
      cash/savings/securities, create/rename root, move a leaf (same source only).

### Stage 3 — trades (migration v14)
- [ ] T14 Parsers keep the trade amount + structured `{side, symbol, quantity, price}`
      (`firstrade.rs:96`, `t212_csv.rs:286`).
- [ ] T15 Schema: `transactions.kind ∈ movement|trade` + trade details (columns or `trades` table).
- [ ] T16 Import books a selected trade as cash leaf ↔ securities leaf (securities leaf auto-created
      under the root); the review proposes it, so there are no 15 manual decisions.
- [ ] T17 Transactions view labels each row: `Bought 7 VOO @ …`, `Transfer → Livret A`, `Dividend`,
      `Interest`, expense.
- [ ] T18 Positions (Rust, average cost): qty, cost basis, avg price, realised gain per symbol, plus
      an `opening_positions` seed and its desktop form.

### Docs
- [ ] T19 `context/overview.md` + `context-map.md`.

## Done when
- Discard → re-drop a file stages its entries as `pending`; an all-duplicate file stays in `staged/`.
- Accounts view shows Crédit Agricole {Eko, Livret A, LDD} and Firstrade {Cash, Securities}.
- All 66 Firstrade rows are importable: 51 income + 15 trades as movements. No gain appears as
  income, and the positions view shows VOO/NVDA/INTC (INTC cost "unknown" until seeded).
- `cargo test`, `npx tsc --noEmit`, `npx vitest run`, `nix build .#maia-desktop` all green.

## Deviations from the plan
- **T1 and T2 were implemented together.** They are two halves of one rule — a stale derived verdict
  survives unless the derived line is deleted *and* the lookup stops honouring dead statements — and
  the human approved the combined session. Each is still a separate, independently tested change.
- **T1 alone does not repair rows already stored as `duplicate`.** A line that a discarded statement
  claimed before this change keeps its verdict until migration v12 (T6) runs.

## Evidence (T1 + T2, 2026-11-12)
- `cargo test`: 229 passed, 0 failed, 1 ignored. The four new tests in `src/lib.rs::statement_tests`
  (`a_line_seen_only_in_a_discarded_statement_is_not_a_duplicate`,
  `a_line_seen_only_in_an_ignored_statement_is_not_a_duplicate`,
  `a_restage_replaces_a_stale_duplicate_line_with_a_pending_one`,
  `a_restage_replaces_an_errored_line_with_a_pending_one`) all fail against the previous
  `store.rs` and pass with it — checked by stashing the change and re-running.
- `a_line_seen_in_another_statement_is_stored_as_duplicate` (the pre-existing live-statement case)
  stayed green throughout: a live statement still claims its lines.
- `cargo clippy --all-targets`: no new warnings and no new errors. The 3 `clippy::erasing_op`
  errors in `src/finance_scenarios.rs` are pre-existing and untouched by this change.
- `npx tsc --noEmit`: clean. `npx vitest run`: 39 passed.
- `cargo build`: succeeds. The new code is rustfmt-clean; the repository is not (37 files have
  pre-existing rustfmt drift, `src/finance_scenarios.rs` and `maia-ui/src-tauri` among them). That
  drift predates this change and was left alone — the file list is byte-identical with and without
  this change.

## Remaining for Stage 1
- **T3 is now the blocking task.** Re-dropping a file whose bytes are *unchanged* still hits the
  "already ingested" no-op, which returns before any line is re-read. A statement that is live and
  whose lines are all `duplicate` therefore stays that way until T3 lets it re-read. T1 and T2 make
  the re-read correct; T3 is what makes the re-read happen.
- T6 (migration v12) is still needed for the live database, which carries rows already marked
  `duplicate` by a statement that was discarded by hand on 2026-11-11.
