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

- [x] T3 "Already ingested" no-op only when the statement has a pending/imported/skipped entry;
      otherwise re-read. Must stay a no-op for unchanged T212 feed months. (status:done)
  - Completed: 2026-11-12
  - Goal: let a live statement be read again even when its bytes are unchanged, so a stale derived verdict can be recomputed.
  - Boundaries: In — the no-op condition in `save_statement` and its doc comment. Out — `collect_file_states` (T4), `error` entries being selectable (T5), the v12 repair migration (T6), schema, UI.
  - Note: the file sources put the bytes in the `source_key`, so for a file "same `source_key`" means "same bytes" and the no-op is the only path a re-dropped file takes. The T212 feed is the exception: its key is the month bucket and the hash covers the whole payload.
  - Note: the predicate is the plan's literal list. See the deviations below for what that costs.
- [x] T4 `collect_file_states` (`watch.rs`): a file is imported only when every entry is
      imported/skipped. (status:done)
  - Completed: 2026-11-12
  - Goal: `parsed/` means imported, so a file whose entries are all duplicates (or hold anything else that is not imported or skipped) waits in `staged/`.
  - Boundaries: In — the "unsettled" test in `collect_file_states` and the two doc comments that state the rule. Out — `error` entries being selectable (T5), the v12 repair migration (T6), the review's empty state (T7), schema, UI.
  - Note: the test is the complement of the rule (`status NOT IN ('imported','skipped')`), so it stays correct as statuses are added. The placement is derived, so a file that gains an unsettled entry walks back to `staged/` on the next pass.
- [x] T5 `error` entries are selectable and counted (`statementReview.ts`, `handlers/statements.rs:61`). (status:done)
  - Completed: 2026-11-13
  - Goal: an entry whose import failed can be retried, and the failure is visible before the file is opened.
  - Boundaries: In — `statementReview.ts` (an `isImportable` helper: pending **or** error; both the ordinary-entry and the transfer-pair branch), `handlers/statements.rs` (`errorCount`), `api.ts` (the type), the desktop display of the count, the bulk-import path, and the counterpart rule in `store.rs`. Out — the review's empty state (T7), the parser (T8), schema.
  - Note: the server already accepted an `error` line for import — `import_statement` short-circuits only `imported`/`informational`/`duplicate` — so the work was in the review, which offered it no checkbox.
  - Note: a failed **transfer pair** is one thing that failed, so the rule in `store.rs` that decides whether a counterpart can still be paired now accepts `error` as well as `pending`. Without it a retry booked the pair as two separate entries and dismissed the match, and money between two of the user's own accounts read as an expense and an income.

- [x] T6 Migration v12: `duplicate` entries whose `duplicate_of` is in a discarded statement →
      `pending`, clear `duplicate_of`. (status:done)
  - Completed: 2026-11-13
  - Goal: release the entries a dead statement was holding, so the live database stops carrying verdicts the code no longer agrees with.
  - Boundaries: In — migration v12 (`src/db.rs`), the rule in `src/statements/store.rs`, and the shared `refresh_statement_status` signature. Out — T7, T8, any schema beyond the version bump.
  - Note: the migration refreshes the statements that held the released entries, so one whose badge said "imported" does not keep saying it — the review's "awaiting" filter would otherwise hide it.
  - Note: it releases the claim of an **ignored** statement as well as a discarded one, because the rule the lookup follows is that only a *live* statement claims anything.
  - Note: `LATEST_VERSION` went to 12, which is one version more than there are snapshots kept, so the three snapshot tests that hard-coded `v1` now use the version just below the latest instead.
- [x] T7 Review explains an empty state ("every entry duplicates another file — re-drop to re-read"). (status:done)
  - Completed: 2026-11-13
  - Goal: a file whose every entry duplicates another file says so, instead of reading as a broken review.
  - Boundaries: In — `statementReview.ts` (a `reviewEmptiness` helper: what can still be imported, and how many entries duplicate another file), the file review and the statement review in `statements.ts`, and the summary's "Nothing selected" message. Out — the server, the schema, the entry tables themselves.
  - Note: the summary used to say "Nothing selected. Tick the entries to import" when there was nothing to tick. It now says there is nothing to import, and the explanation below it says why.

- [x] T8 Firstrade parser warns on an unreadable date row instead of `continue` (`firstrade.rs:273`);
      the statement records rows read / kept / skipped and the review shows it. (status:done)
  - Completed: 2026-11-13
  - Goal: a row the parser cannot date is reported instead of vanishing, and the statement says how many rows it read, kept and dropped.
  - Boundaries: In — `firstrade.rs` (the CSV reader keeps an undatable row so the parser can warn; the statement records the counts in its `parsed_json`), `statementReview.ts` (a `parsedRowsSummary` helper) and the two reviews that show it. Out — the other parsers, the entry tables, the schema.
  - Note: the row is still dropped — it has no date, so it cannot become an entry. What changed is that it is now counted and named in a warning, instead of disappearing with no trace.
  - Note: the warning was already there for a row that reached the parser; the reader dropped the rows before that, so it never fired.

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
- **T3 uses the plan's literal predicate, not a stronger one.** The plan names `pending`,
  `imported` and `skipped`; a stronger rule ("re-read whenever a `duplicate` verdict could be
  stale") was considered and declined by the human. Two consequences follow, both deliberate:
  - A statement holding *both* a `pending` entry and a stale `duplicate` entry stays a no-op, so
    the stale duplicate survives. Only an all-duplicate statement is read again.
  - A T212 feed month whose entries are all informational — a month of trades and nothing else —
    holds no entry the user can act on, so it is read again on every sync. Nothing is lost (the
    entries come back with the same ids), but the statement is touched. A feed month with pending
    or imported entries stays a no-op, which is the constraint the plan states.
- **T4 reads "every entry is imported or skipped" literally, so `informational` holds a file in
  `staged/`.** A weaker rule (`pending`/`error`/`duplicate` pin the file, `informational` does not)
  was considered and declined by the human. Consequences: the Firstrade file (51 income + 15
  trades) stays in `staged/` until Stage 3 turns trades into movements, and a file whose entries
  are all trades waits in `staged/` with nothing the review offers to select. The rule is now
  written as the complement (`status NOT IN ('imported','skipped')`), so it stays correct as
  statuses are added.

## Evidence (T1–T5)

### T1 + T2 (2026-11-12, commit `b056627`)
- `cargo test`: 229 passed, 0 failed, 1 ignored. The four new tests in `src/lib.rs::statement_tests`
  (`a_line_seen_only_in_a_discarded_statement_is_not_a_duplicate`,
  `a_line_seen_only_in_an_ignored_statement_is_not_a_duplicate`,
  `a_restage_replaces_a_stale_duplicate_line_with_a_pending_one`,
  `a_restage_replaces_an_errored_line_with_a_pending_one`) all fail against the previous
  `store.rs` and pass with it — checked by stashing the change and re-running.
- `a_line_seen_in_another_statement_is_stored_as_duplicate` (the pre-existing live-statement case)
  stayed green throughout: a live statement still claims its lines.

### T3 (2026-11-12, commit `f0c75a1`)
- `cargo test`: 233 passed. `a_live_statement_whose_entries_all_duplicate_is_read_again` and
  `an_informational_only_feed_month_is_read_again` fail against the previous `store.rs`;
  `an_unchanged_statement_with_an_imported_entry_is_a_no_op` and `an_unchanged_feed_month_is_a_no_op`
  pass in both states — they are the guards the plan's feed constraint asks for.

### T4 (2026-11-12, commit `1cd3702`)
- `cargo test`: 235 passed. `a_file_whose_entries_all_duplicate_stays_staged` and
  `an_informational_entry_holds_its_file_in_staged` fail against the previous query and pass with it
  (checked by reverting only the SQL, keeping the tests). The nine existing placement tests stayed
  green.

### T5 (2026-11-13)
- `cargo test`: 237 passed, 0 failed, 1 ignored. `the_statement_list_counts_the_entries_that_failed`
  fails without the `errorCount` query; `a_failed_transfer_pair_is_retried_as_one_transfer` fails
  without the counterpart rule (the retry books an `expense` instead of a `transfer`).
- `npx tsc --noEmit`: clean. `npx vitest run`: 42 passed (3 new). The two new selection tests fail
  against the previous `statementReview.ts`.

### T6 (2026-11-13)
- `cargo test`: 238 passed, 0 failed, 1 ignored.
  `migration_v12_releases_the_entries_a_dead_statement_was_holding` fails when the migration is
  neutered: the entry stays `duplicate` with its claim intact.
- The three snapshot tests that used to name `v1` now name the version just below the latest; they
  pass, and the snapshot file list a migration produces is unchanged.
- `npx tsc --noEmit`: clean. `npx vitest run`: 42 passed.
- `cargo clippy --all-targets`: no new warnings and no new errors. The 3 `clippy::erasing_op`
  errors in `src/finance_scenarios.rs` are pre-existing and untouched by this change.
- `cargo build`: succeeds. The new code is rustfmt-clean; the repository is not (37 files have
  pre-existing rustfmt drift, `src/finance_scenarios.rs` and `maia-ui/src-tauri` among them). That
  drift predates this change and was left alone — the file list is byte-identical with and without
  this change.

### T7 (2026-11-13)
- `npx tsc --noEmit`: clean. `npx vitest run`: 46 passed (4 new, for `reviewEmptiness`).
- `cargo test`: 238 passed — no server change, so the Rust suite is untouched.

### T8 (2026-11-13)
- `cargo test`: 239 passed.
  `a_row_without_a_readable_date_is_counted_and_warned_about` and
  `the_statement_records_the_rows_it_read_kept_and_skipped` both fail against the previous reader
  (it dropped the row: "both rows were read — left: 1, right: 2"). The previous test, which pinned
  the silent drop, was replaced by these two.
- `npx tsc --noEmit`: clean. `npx vitest run`: 49 passed (3 new, for `parsedRowsSummary`).
- `cargo clippy --all-targets`: no new warnings and no new errors.
- The new code is rustfmt-clean.

## Remaining for Stage 1
- **Nothing — Stage 1 is complete.** Stage 2 starts with migration v13: the account tree
  (`account_roots` and `accounts.root_id`).
