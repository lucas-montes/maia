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
  - Boundaries: In — the duplicate lookup in `src/statements/store.rs`, its `save_statement` doc comment. Out — re-staging semantics (T2), the "already ingested" no-op (T3), repairing rows already stored as `duplicate` (T6), schema, UI.
  - Note: the lookup is the only one in the codebase. `preview.rs::describe_duplicates` reads already-stored `duplicate_of` links and does no lookup.

- [x] T2 Re-stage deletes `pending, informational, duplicate, error` lines (cascades to
      `transfer_matches`, which `match_pending` rebuilds right after) (status:done)
  - Completed: 2026-11-12
  - Boundaries: In — the re-stage `DELETE` in `src/statements/store.rs` and its doc comment. Out — the "already ingested" no-op (T3), `collect_file_states` (T4), `error` lines being selectable (T5), the v12 repair migration (T6).
  - Note: `imported`/`skipped` lines are never deleted, so user decisions survive. `transfer_matches` cascades on the line delete and `match_pending` runs right after the batch (`statements/mod.rs`), so matches rebuild themselves.

- [x] T3 "Already ingested" no-op only when the statement has a pending/imported/skipped entry;
      otherwise re-read. Must stay a no-op for unchanged T212 feed months. (status:done)
  - Completed: 2026-11-12
  - Boundaries: In — the no-op condition in `save_statement` and its doc comment. Out — `collect_file_states` (T4), `error` entries being selectable (T5), the v12 repair migration (T6), schema, UI.
  - Note: the file sources put the bytes in the `source_key`, so for a file "same `source_key`" means "same bytes" and the no-op is the only path a re-dropped file takes. The T212 feed is the exception: its key is the month bucket and the hash covers the whole payload.
  - Note: the predicate is the plan's literal list. See the deviations below for what that costs.

- [x] T4 `collect_file_states` (`watch.rs`): a file is imported only when every entry is
      imported/skipped. (status:done)
  - Completed: 2026-11-12
  - Boundaries: In — the "unsettled" test in `collect_file_states` and the two doc comments that state the rule. Out — `error` entries being selectable (T5), the v12 repair migration (T6), the review's empty state (T7), schema, UI.
  - Note: the test is the complement of the rule (`status NOT IN ('imported','skipped')`), so it stays correct as statuses are added. The placement is derived, so a file that gains an unsettled entry walks back to `staged/` on the next pass.

- [x] T5 `error` entries are selectable and counted (`statementReview.ts`, `handlers/statements.rs:61`). (status:done)
  - Completed: 2026-11-13
  - Boundaries: In — `statementReview.ts` (an `isImportable` helper: pending **or** error; both the ordinary-entry and the transfer-pair branch), `handlers/statements.rs` (`errorCount`), `api.ts` (the type), the desktop display of the count, the bulk-import path, and the counterpart rule in `store.rs`. Out — the review's empty state (T7), the parser (T8), schema.
  - Note: the server already accepted an `error` line for import — `import_statement` short-circuits only `imported`/`informational`/`duplicate` — so the work was in the review, which offered it no checkbox.
  - Note: a failed **transfer pair** is one thing that failed, so the rule in `store.rs` that decides whether a counterpart can still be paired now accepts `error` as well as `pending`. Without it a retry booked the pair as two separate entries and dismissed the match, and money between two of the user's own accounts read as an expense and an income.

- [x] T6 Migration v12: `duplicate` entries whose `duplicate_of` is in a discarded statement →
      `pending`, clear `duplicate_of`. (status:done)
  - Completed: 2026-11-13
  - Boundaries: In — migration v12 (`src/db.rs`), the rule in `src/statements/store.rs`, and the shared `refresh_statement_status` signature. Out — T7, T8, any schema beyond the version bump.
  - Note: the migration refreshes the statements that held the released entries, so one whose badge said "imported" does not keep saying it — the review's "awaiting" filter would otherwise hide it.
  - Note: it releases the claim of an **ignored** statement as well as a discarded one, because the rule the lookup follows is that only a *live* statement claims anything.
  - Note: `LATEST_VERSION` went to 12, which is one version more than there are snapshots kept, so the three snapshot tests that hard-coded `v1` now use the version just below the latest instead.
- [x] T7 Review explains an empty state ("every entry duplicates another file — re-drop to re-read"). (status:done)
  - Completed: 2026-11-13
  - Boundaries: In — `statementReview.ts` (a `reviewEmptiness` helper: what can still be imported, and how many entries duplicate another file), the file review and the statement review in `statements.ts`, and the summary's "Nothing selected" message. Out — the server, the schema, the entry tables themselves.
  - Note: the summary used to say "Nothing selected. Tick the entries to import" when there was nothing to tick. It now says there is nothing to import, and the explanation below it says why.

- [x] T8 Firstrade parser warns on an unreadable date row instead of `continue` (`firstrade.rs:273`);
      the statement records rows read / kept / skipped and the review shows it. (status:done)
  - Completed: 2026-11-13
  - Boundaries: In — `firstrade.rs` (the CSV reader keeps an undatable row so the parser can warn; the statement records the counts in its `parsed_json`), `statementReview.ts` (a `parsedRowsSummary` helper) and the two reviews that show it. Out — the other parsers, the entry tables, the schema.
  - Note: the row is still dropped — it has no date, so it cannot become an entry. What changed is that it is now counted and named in a warning, instead of disappearing with no trace.
  - Note: the warning was already there for a row that reached the parser; the reader dropped the rows before that, so it never fired.

### Stage 2 — account tree (migration v13)
- [x] T9  `account_roots` + `accounts.root_id`; map `bank→cash, investment→cash, other→cash`;
      create "Crédit Agricole" (bank) and re-parent the 4 existing accounts. (status:done)
  - Completed: 2026-11-13
  - Boundaries: In — migration v13 (`src/db.rs`), `SourceId::kind` and `root_for_source` (`statements/mod.rs`), `default_account_type` (`statements/store.rs`). Out — the import creating roots (T10), the identity check (T11), the phone push (T12), the desktop view (T13).
  - Note: root ids are minted, never derived, and roots are found by `source` — so the root the migration creates and the root the import would create are the same row. `account_roots.source` has a partial unique index, like `idx_accounts_external`: a root the user made has no source, and two of those must not collide.
  - Note: `default_account_type` moved to the new vocabulary in this task rather than T10. Leaving it would have meant the migration remapped `bank` to `cash` and the next import wrote `bank` straight back.
  - Note: `updated_at` moves on a remapped account, because the pull sends `type` and the phone has to learn the new value.
  - Note: **`credit` is left alone** — it is a card, not a product, and the new vocabulary has no word for it. T13's type editor offers cash/savings/securities, so an existing `credit` account will need a decision there.
- [x] T10 Statement import resolves/creates the root by source (Firstrade → broker root with a
      `cash` leaf); `default_account_type` becomes the kind rule (CA: livret/ldd/épargne → savings). (status:done)
  - Completed: 2026-11-13
  - Boundaries: In — `resolve_root` and `lookup_root` (`statements/store.rs`), the account insert's `root_id`, and `default_account_type` as the kind rule. Out — the identity check (T11), the phone push (T12), the desktop view (T13), the securities leaf (Stage 3).
  - Note: the root is found by its source, so the one T9's migration created is the one the import finds — there is one root per institution, and the accounts are its leaves.
  - Note: `default_account_type` keeps its `(source, name)` signature and derives the kind inside it, so the preview's fallbacks — which must not create anything — read the same rule. The kind rule: a bank has savings products (livret/ldd/épargne) and everything else is cash; a broker and a bank without branches are cash.
  - Note: the securities leaf is not created here — it is Stage 3 (T16).
- [x] T11 Identity endpoint refuses a leaf whose source differs from its root's. (status:done)
  - Completed: 2026-11-13
  - Boundaries: In — the source/root check in `PUT /accounts/:id/identity` (`handlers/accounts.rs`) and the `root_id` it writes. Out — the phone push (T12), the desktop view and moving a leaf (T13).
  - Note: a leaf with **no** root — one the phone made — joins the root of the source it is given, so the tree stays complete rather than refusing the one account the desktop can actually give an identity to.
  - Note: clearing an identity leaves the root alone: the account is still that institution's, it just has no provider number any more.
- [x] T12 Phone push stops writing `type`. (status:done)
  - Completed: 2026-11-13
  - Boundaries: In — the pushed type in `src/handlers/push.rs` (optional, and applied only when the server knows it), the phone's push (`mobile/lib/src/sync/data_push_service.dart`), and the desktop's type editor list. Out — the pull, which still sends the type ✓.
  - Note: the phone does not push the type at all — the pull sends it down — and the server ignores a pushed type it does not know, so a phone still carrying the old vocabulary cannot revert the rename either.
  - Note: the desktop's editor now offers only `cash`/`savings`/`securities`. It has to: the server ignores an unknown type, so offering `bank` would be an edit that silently does nothing. That is one line of T13 done early.
  - Note: **the mobile app was changed**, against this plan's "server + desktop only" header, because the human chose it: the alternative was a desktop-only endpoint the desktop could use instead of the phone's push surface.
- [x] T13 Desktop accounts view: roots with leaves under them, root total, type editor offers
      cash/savings/securities, create/rename root, move a leaf (same source only). (status:done)
  - Completed: 2026-11-14
  - Goal: the accounts screen shows the tree — Crédit Agricole {Eko, Livret A, LDD} — not a flat list.
  - Boundaries: In — three desktop-only server endpoints (`POST /account-roots`, `PUT /account-roots/:id`, `PUT /accounts/:id/root`), the desktop's `list_account_roots` command and `rootId` on `list_accounts`, the tree in the accounts view, and `maia-ui/src/lib/accountTree.ts`. Out — the phone, the server's roots API, schema.
  - Note: the roots are read from the database by the desktop's own command, not over the sync API — roots are server-only state the phone must neither see nor push ✓.
  - Note: a leaf can only go under a root of its own source, and a leaf with a provider number cannot be unfiled — the same rule the identity endpoint follows ✓.
  - Note: the type editor's vocabulary was narrowed in T12, so what is left here is the tree ✓.
  - Note: a leaf whose root is gone (the two lists came from different moments) is kept aside rather than dropped — an account that vanishes from the view is worse than one that looks unfiled ✓.

### Stage 3 — trades (migration v14)
- [x] T14 Parsers keep the trade amount + structured `{side, symbol, quantity, price}`
      (`firstrade.rs:96`, `t212_csv.rs:286`). (status:done)
  - Completed: 2026-11-14
  - Boundaries: In — `draft.rs` (`TradeSide`, `TradeDetails`, `StatementLineDraft.trade`), the three trade parsers (Firstrade, T212 CSV, and the T212 API feed, which also staged trades at 0), and the store writing the details into `metadata_json` under `"trade"` with `store::stored_trade` to read them back. Out — the schema (T15), booking (T16), the review's display (T17).
  - Note: the amount is the cash the trade moved, signed by the side (buy negative, sale positive), because exports disagree on the sign. Firstrade falls back to quantity × price when the row has no amount; T212 does not, because its price is in the instrument's currency and its Total in the account's.
  - Note: a trade's **identity keeps the zero amount** it always had, so a trade staged before keeps its line id and dedupe key, and an older export of the same trades still reads as a duplicate.
  - Note: for T16 — trades already staged keep amount 0 and no details until their statement is read again, and the "already ingested" no-op (T3) means re-dropping the Firstrade file does **not** re-read it (it holds pending entries). T16 needs a way to re-read it, or the booking must refuse a trade with no stored details.
  - Note: for T16 — `match_pending` matches any `pending` line by amount. Once trades become pending with real amounts, it must skip `line_kind = 'trade'`, or a buy could pair with a deposit of the same sum.
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
- **T3 uses the plan's literal predicate** (`pending`, `imported`, `skipped`), not the stronger "read
  it again whenever a `duplicate` verdict could be stale" the human declined. Two consequences, both
  deliberate: a statement holding *both* a `pending` entry and a stale `duplicate` entry stays a
  no-op, so the stale duplicate survives; and a T212 feed month of nothing but trades holds no
  entry the user can act on, so it is read again on every sync — nothing is lost, the entries come
  back with the same ids, but the statement is touched.
- **T4 reads "every entry is imported or skipped" literally, so `informational` holds a file in
  `staged/`.** The weaker rule the human declined would have let a file of nothing but trades reach
  `parsed/` with nothing imported. As it stands the Firstrade file (51 income + 15 trades) waits in
  `staged/` until Stage 3 turns trades into movements. The rule is written as the complement
  (`status NOT IN ('imported','skipped')`), so it stays correct as statuses are added.
- **T12 changed the mobile app**, against this plan's "server + desktop only" header: the human
  chose it over a desktop-only endpoint for the account save.

## Evidence (Stage 1, T1–T8)

Every task's new test was checked against the code it replaces — stashing the change or reverting
only the part under test — so each one fails without its fix. The counts below are the suite after
each task; the suite is green at the end of the stage (240 Rust, 49 frontend).

| Task | Test that fails without the fix |
|---|---|
| T1 + T2 | `a_line_seen_only_in_a_discarded_statement_is_not_a_duplicate`, `a_line_seen_only_in_an_ignored_statement_is_not_a_duplicate`, `a_restage_replaces_a_stale_duplicate_line_with_a_pending_one`, `a_restage_replaces_an_errored_line_with_a_pending_one` |
| T3 | `a_live_statement_whose_entries_all_duplicate_is_read_again`, `an_informational_only_feed_month_is_read_again` |
| T4 | `a_file_whose_entries_all_duplicate_stays_staged`, `an_informational_entry_holds_its_file_in_staged` |
| T5 | `the_statement_list_counts_the_entries_that_failed`, `a_failed_transfer_pair_is_retried_as_one_transfer` |
| T6 | `migration_v12_releases_the_entries_a_dead_statement_was_holding` |
| T7 | `reviewEmptiness` (4 new tests) |
| T8 | `a_row_without_a_readable_date_is_counted_and_warned_about`, `the_statement_records_the_rows_it_read_kept_and_skipped` |

Guards that must pass in **both** states, because they pin behaviour the change must not break:
`a_line_seen_in_another_statement_is_stored_as_duplicate` (a live statement still claims its lines),
`an_unchanged_statement_with_an_imported_entry_is_a_no_op` and `an_unchanged_feed_month_is_a_no_op`
(the plan's feed constraint), and the nine existing file-placement tests.

Also checked, for every task: `cargo clippy --all-targets` (no new warnings; the 3 `erasing_op`
errors in `src/finance_scenarios.rs` are pre-existing), `cargo build`, `npx tsc --noEmit` and
rustfmt (the new code is clean; 37 files have pre-existing drift).

Two findings worth keeping: **T3** exposed that all three file sources put the file's hash in
their `source_key`, so for a file "same `source_key`" means "same bytes" — the no-op was the only
path a re-dropped file could take (the T212 feed is the exception). **T6** bumped the schema past
the number of snapshots kept, breaking three tests that hard-coded `v1`.

## Stage 2 (the account tree)

Stage 1 is complete: every task above is done and the suite is green.

### T9–T13 (2026-11-13/14)

Each task's new test was checked against the code it replaces, so each fails without its fix.

| Task | Test that fails without the fix | Suite after |
|---|---|---|
| T9 | `migration_v13_builds_the_account_tree_and_remaps_the_types` | 240 Rust |
| T10 | `an_import_creates_the_root_and_parents_the_account_under_it`, `a_brokerage_import_makes_a_broker_root_with_a_cash_leaf`, `two_accounts_of_one_institution_share_one_root` | 243 Rust |
| T11 | `a_number_from_another_institution_is_refused` | 247 Rust |
| T12 | `a_push_cannot_revert_the_account_type` | 249 Rust |
| T13 | `a_leaf_moves_under_a_root_of_its_own_source`, `a_leaf_goes_under_a_root_that_shares_its_source_or_lacks_one` | 252 Rust |

**The real database was migrated on a copy** to check T9 end to end: v11 → v13 produced one root,
"Crédit Agricole" (kind `bank`, source `credit_agricole`), with all four accounts under it — "Eko
by CA" → cash, "Livret A" → savings, "LDD Solidaire" → savings, "Eko by CA (external)" → cash (the
deleted one). The live database itself was not touched.

T12's phone change: `flutter analyze` on the changed file reports no issues, and the phone's suite
gives the same result with and without it (190 passed, 156 failed — the failures are pre-existing
and environmental: `libsqlite3.so` cannot be loaded in this shell, and they fail identically at
HEAD). T13's `accountTree` tests include one that caught a real bug: a leaf whose root was not in
the list was dropped from the view instead of being kept aside.

For every task: `cargo clippy --all-targets` (no new warnings), `cargo build`, `npx tsc --noEmit`,
and rustfmt (the new code is clean).

**One pre-existing failure, not from this work:**
`tests::receipt_ocr_parse_and_picture_endpoints` polls for 12 seconds for an upstream call whose
own timeout is 90, so it fails wherever the upstream call is slow. It fails identically at HEAD.

### Review fixes (2026-11-14)
A review of Stages 1–2 found four problems, fixed before Stage 3 because T16 depends on the first:
- **An import picks an institution, never an account** (decided by the human). The statement-level
  `accountId` override became `rootId`, and the unused per-line `accountId` is gone. The server
  picks the leaf: the statement's account, then the `(source, external_id)` match, then a new leaf.
  Any account the import reaches without a root is filed under the chosen one, which fixes a
  hand-picked account staying outside its institution, where T16 could never find its securities
  sibling. A hand-made root with no source is adopted; another institution's root is refused per
  entry. The review's "Import into" account dropdown became an "Institution" dropdown, and the
  preview reports `institution`/`institutionIsNew` per account.
- **A transfer pair ignored the user's choice.** `import_pair` resolved both sides with no
  override, so a pair booked onto different accounts than the rest of its statement. The choice is
  now batch-wide (`root_choices`), and each side uses its own statement's.
- **Clearing a provider number broke "a leaf's source equals its root's".** It now removes the
  number only. A filed leaf keeps its root's source.
- **Dead code and a deleted root.** The no-op `let _ = root_id;` is gone, and `resolve_root`
  revives a deleted root instead of trying to insert a second one (the unique index on the root's
  source covers deleted rows too).

Tests: `a_transfer_pair_goes_under_the_chosen_institution`,
`a_hand_picked_account_with_no_institution_is_filed_under_it`, `another_institutions_root_is_refused`,
`a_deleted_institution_is_revived_by_an_import`, `a_hand_made_institution_is_adopted_with_its_accounts`,
`an_institution_choice_is_what_the_preview_reports`, `the_preview_names_the_institution_it_would_create`,
and the two clear-identity tests updated to the new rule. 259 Rust tests (the receipt OCR test is
the known flaky one and passes on its own), 56 frontend tests, `tsc` clean. These tests were **not**
checked against the old code by stashing, unlike the earlier tasks' tests.

Not fixed: a phone-created account arrives as `cash`, because the phone no longer pushes a type.
Fixing it needs a phone change that sends the type on creation.

### Still to come
- **Stage 2 is complete; T14 is done.** Next is T15, migration v14: the trade columns on `transactions`.
