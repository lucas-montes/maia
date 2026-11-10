# Statements: one account per file, cross-file account matching, an honest import preview, and a user-owned intake folder

## Change summary
Dropping one Firstrade CSV made the review screen offer to create **34 accounts**; two Trading 212
exports offered **20**. In both cases the correct answer is **one**. The cause is that the brokerage
parsers split a file into one statement per month, and the review UI equates "statement" with
"account" — a mapping that only holds for Crédit Agricole, where one statement really is one account
section of the export.

Confirmed against `/var/lib/maia/maia.db` (read-only):

| source | statements | distinct `account_number` | lines |
|---|---|---|---|
| `credit_agricole` | 3 | 3 | 98 |
| `firstrade` | 34 | **1** | 66 |
| `trading212` | 20 | **1** | 721 |

Nothing was actually mis-created in the ledger: `resolve_account` (`src/statements/store.rs:1221`)
derives the account id from `stable_uuid(ACCOUNT_NS, "account:<source>:<external_id>")`, and all 34
Firstrade statements carry `external_id='firstrade'` (hardcoded at `src/statements/firstrade.rs:26`),
all 20 T212 ones `external_id='csv'` (`src/statements/t212_csv.rs:329`). Import would have created
exactly one account per source. The defect is that the UI *promises* 34 — `fileHtml` renders one
"will be created on import" row per statement (`maia-ui/src/views/statements.ts:391-420`) — which is
indistinguishable from the thing going wrong, and left no way to say "merge these into one" or "use
this existing account instead".

**The generalised defect is that the review screen guesses at what import will do.** It reads
statement rows and renders its own interpretation, while `import_statement` resolves accounts,
collapses transfer pairs, suppresses duplicates and converts currency on its own terms. The two can
disagree — 34 versus 1 is just the visible case. So the fix is not only to count accounts correctly:
the screen must be driven by the import's *own* answer.

The month buckets also lose data. `source_key` is month-scoped for the file sources
(`trading212:csv:csv:<month>`, `firstrade:firstrade:<month>`), so two exports whose ranges overlap
write the *same* bucket, and the re-stage branch of `save_statement`
(`src/statements/store.rs:100-105`) deletes every `pending` line of that bucket before inserting its
own. This already happened:

```
trading212:csv:csv:2026-02 | from_2025-02-03_to_2026-02-03_….csv | created …672 | 3 lines | 2026-02-01 → 2026-02-03
trading212:csv:csv:2026-03 | from_2026-02-03_to_2026-09-04_….csv | created …674 | 40 lines
```

`from_2026-02-03_to_2026-09-04` staged first and filled `2026-02` with the whole month;
`from_2025-02-03_to_2026-02-03` staged after and replaced it with its 3 lines, repointing the
bucket's `source_path` to itself. Roughly three weeks of February 2026 T212 activity silently
disappeared from review. Line-level `dedupe_key` would have handled the overlap correctly as
`duplicate` rows — the bucket collision preempts it.

Three adjacent problems, reported in the same work stream:

- **Account identity cannot be repaired by hand.** `accounts.source`/`external_id` are deliberately
  server-only (`src/db.rs:862-866`: `pull_accounts` and `push_budget_accounts` both use explicit
  column lists so a phone can never see or clear them), and the desktop edits accounts through
  `POST /budget-accounts` — that same sync endpoint, whose `INSERT … ON CONFLICT DO UPDATE` names
  only `name`/`type`/`opening_balance`/`note` (`src/handlers/push.rs:1456`). The accounts view
  *displays* `externalId` read-only (`maia-ui/src/views/accounts.ts:256`) but cannot set it. So a
  hand-created "Firstrade" account can never be told which provider account it is, and statements can
  never snap onto it.
- **Discarding a staged statement leaves its file in `parsed/`.** `delete_statements`
  (`src/handlers/statements.rs:358`) is a pure DB soft-delete and never touches `stored_path`, while
  `ingest_file` (`src/statements/watch.rs:284`) moves a file to `parsed/` as soon as the *parser*
  succeeded. So `parsed/` currently means "did not crash", not "accepted", and the rejected
  `fristrade.csv` sits in `parsed/firstrade/` looking imported.
- **Getting files into the dump tree takes too many drags.** `dump_dir`/`parsed_dir`/`failed_dir`
  are config-settable (`src/config.rs:380`) but deliberately deployment-owned and rendered
  *(managed)* in Settings, for the reason stated at `src/config.rs:368`: the service reads, moves and
  grants access to those paths, so a per-user overlay must not redirect them. That rationale stands
  and points at the right shape — a *separate*, user-owned **intake** folder that the service drains
  into the managed tree, leaving the lifecycle roots managed.

Also resolved by this investigation: **T01/T02 of `statement-review-ui-fixes` are answered.**
`sourcePath` does reach the client (`src/handlers/statements.rs:103`, non-null on every row) and
`groupStatements` works. The list looked ungrouped because a single file card held 34 rows. That
plan's T03 (checkbox alignment) and T04 (select-all) are still open and live in the same file, so
they are carried here and that plan is superseded.

## Decisions

### Parsing
1. **One statement per account per dropped file; drop the month split for file sources.** This is not
   a new convention — it is Crédit Agricole's existing one. `ca.rs:141` already keys statements
   `<source>:<account_number>:<source_hash>` (the committed test asserts
   `credit_agricole:11112222333:abc`). Firstrade and the T212 CSV adopt the same key, so "the file is
   the unit" holds for every file source and re-dropping identical bytes stays the no-op it is today.
2. **Month buckets survive only in the Trading 212 API feed** (`src/statements/t212.rs:492`,
   `trading212:<account_id>:<month>`). A feed has no file and legitimately re-syncs the current
   month, which is exactly the case the month key was designed for. `month_key`/`month_bounds` stay
   exported from `t212.rs` for it and lose their `firstrade.rs` / `t212_csv.rs` callers.
3. **Overlapping exports resolve at line level, not bucket level.** Two files that cover the same
   dates produce two statements; the later file's repeated lines land as `duplicate` via the existing
   `dedupe_key` path (`store.rs:142-150`) — visible in review, not importable, never silently
   deleted. This is the mechanism that was already built and was being bypassed.
4. **Period becomes the file's real span**: `period_start`/`period_end` = min/max `posted_at` of the
   statement's lines, instead of synthetic month bounds.

### Accounts and identity
5. **An account created by an import is an ordinary account.** Its id is an internal identifier,
   minted when the row is created and derived from nothing. Today `resolve_account` computes it as
   `stable_uuid(ACCOUNT_NS, "account:<source>:<external_id>")` (`store.rs:1245`), which welds the id
   to the provider identity and makes the identity effectively unchangeable — change the number and
   the next import looks somewhere else. That coupling is removed: a created account gets a fresh
   `Uuid::now_v7()`, the project's convention everywhere else, and `ACCOUNT_NS` is retired (that one
   line is its only consumer; `src/statements/mod.rs:37` is the only other mention).
6. **Resolution is fetch-then-create, in that order.** The exact `(source, external_id)` lookup
   (`lookup_external_account`, `store.rs:1317`) already runs *before* any id is minted, so reusing an
   existing account is already the first thing tried — dropping the deterministic id changes only the
   miss path. The idempotency the deterministic id was providing comes from the partial unique index
   `idx_accounts_external` (`src/db.rs:878`) instead, which is stronger: it is enforced by the
   database rather than by two call sites agreeing on a hash.
7. **A to-be-created account is described, not pre-named with an id.** The preview reports the name
   and the `default_account_type` guess for an identity with no account yet; it does not promise an
   id, because the id does not exist until the row does. (This replaces an earlier draft of this plan
   that had the preview name the deterministic id — an artefact of the coupling being removed.)
8. **Identity is `(source, external_id)` and is resolved once for every staged statement, not once
   per file.** The preview groups every statement — across all files, staged at any time — under one
   target account per identity, and reports that account as created **once**, listing the files and
   statements that feed it. Two Firstrade drops, or a January and a February CA export, read as one
   account with two source files rather than two proposals.
9. **Every account field is editable afterwards and the id never changes** — name, type, opening
   balance, note, and the provider identity. An import must never overwrite a user's edit, which
   today holds by construction: `resolve_account` only ever INSERTs (`ON CONFLICT DO NOTHING`), and
   `save_statement` writes `bank_statements.account_name` — the statement's own copy of the name, not
   the account's. Add a test pinning that rather than relying on it continuing to be true.
10. **Identity edits go through a desktop-only endpoint.** New `PUT /accounts/:id/identity` taking
    `{source, externalId}` (either nullable, to clear). It must **not** go through
    `/budget-accounts`: extending that `ON CONFLICT DO UPDATE` column list would put identity on the
    phone sync surface and let a phone push clear it, which is precisely what `src/db.rs:862-866` set
    out to prevent. `idx_accounts_external` is honoured with a **409 naming the conflicting
    account**, never a raw `SQLITE_CONSTRAINT`.
11. **Re-pointing an identity is harmless and needs no guard.** Because the id is independent of the
    identity, moving a number keeps the account row, its name and every transaction already imported
    onto it; only where the *next* import looks changes. Nothing can be stranded, so there is no
    refusal, no warning and no transaction-reassignment step. This is a direct consequence of
    decision 5 and the reason it is worth making.
12. **Setting an identity snaps staged statements onto that account.** After an identity edit, re-run
    the resolution `save_statement` already performs at stage time (`store.rs:211-227`) for every live
    statement matching the new `(source, external_id)`, so their `account_id` fills in and the next
    preview says "uses existing account" without the user touching the statements.
13. **A miss is a proposal, not a guess.** Automatic reuse stays exact-match only — the server never
    silently decides that an account named "Firstrade" is the Firstrade export. Instead, when an
    identity has no account, the preview lists candidate existing accounts (same `source`, or an
    identity-less account whose name matches the statement's `account_name`/institution) as
    *suggestions*. Accepting one calls decision 10's endpoint, which writes the identity onto the
    chosen account — so a confirmed suggestion becomes an exact match from then on, and the
    suggestion machinery never has to be trusted twice.
14. **No invented account fields.** `accounts` has no currency column (`src/db.rs:545-554`); currency
    lives on the statement and the transaction. Do not add one as part of this plan.

### The import preview
15. **The preview is the real import, executed in a transaction that is rolled back.** `POST
    /statements/preview` takes the same `BatchImportRequest` body as `/statements/import`, runs the
    identical `store::import_statement` path (it already takes `&Transaction`, `store.rs:621`), and
    rolls back. Promise equals outcome *by construction*. A parallel estimator was rejected: a second
    implementation of "what import will do" is exactly the drift that produced this plan.
16. **The outcome describes, not just counts.** `ImportOutcome` (`store.rs:523`) today reports
    `created`/`skipped`/`errors` plus per-line rows. It grows to carry: accounts to be created (id,
    name, type, feeding files) versus reused; **merges** made explicit in both flavours — duplicate
    `dedupe_key` groups (which line imports, which are suppressed) and transfer pairs collapsing two
    lines into one transaction naming both accounts; informational trade lines that are never
    imported; and per-line validation errors.
17. **The preview surfaces FX failures before the click.** `rate_on_or_before` (`store.rs:28-35`)
    fails an import with "no USD/EUR FX rate on or before <date>" — and Firstrade is entirely USD, so
    this is a live hazard on the very file that prompted this work. Because the preview runs the real
    path, those errors appear in the preview per line, attributed to the missing rate and date.
18. **A rolled-back dry run must have no effect outside its transaction.** The preview handler calls
    the store only — never the file reconcile of decision 20, never the notifier. Asserted by a test,
    not by inspection.

### File lifecycle
19. **`parsed/` means imported.** Two new roots join the tree: `staged/` (parsed, awaiting review)
    and `discarded/` (statements deleted without being accepted). A file's root is derived from the
    state of its statements, not from the event that last touched it.
20. **The root is reconciled, not imperatively moved.** A single `reconcile_files(state)` walks every
    distinct `stored_path` and relocates the file to where the DB says it belongs:

    | statement state for that `stored_path` | root |
    |---|---|
    | any live statement with pending lines | `staged/<source>/` |
    | every live statement fully imported/skipped | `parsed/<source>/` |
    | every statement soft-deleted | `discarded/<source>/` |
    | parse never succeeded | `failed/<source>/` (unchanged, `park` owns it) |

    This mirrors `notes_mirror::reconcile()`, the pattern the project already uses and documents for
    derived-from-DB filesystem state. It is called at startup, after an import commits and after a
    delete commits — and because it derives rather than remembers, it migrates today's mislabelled
    `parsed/` files on first boot for free, and self-heals a file moved by hand.
21. **Deletion stays soft and the file is kept.** A discarded file in `discarded/` can be dropped
    back in; the revive branch of `save_statement` (`store.rs:55-60`) already treats a re-drop of a
    deleted statement as recovery. Hard-deleting the bytes was rejected for that reason.

### Intake folder
22. **The intake directory is user-owned, read from the `~/.config/maia/config.json` overlay**, beside
    the Trading 212 feed settings that the Settings form already owns (`src/config.rs:388-400`). The
    four lifecycle roots stay deployment-owned and *(managed)* in the UI. The service only ever *reads
    and moves out of* intake — it never parses in place, so pointing it at a directory the user also
    uses for other things stays safe.
23. **The source comes from the file name, not from a subfolder and not from sniffing the content.**
    The user renames exports to carry the source (`firstrade-2026-11.csv`, `t212_feb_aug.csv`,
    `ca-20261027.csv`). A new `SourceId::from_file_name` splits the stem on `[-_. ]` and matches whole
    tokens against the aliases `SourceId::parse` already accepts (`credit_agricole`,
    `credit-agricole`, `ca`, `trading212`, `trading_212`, `t212`, `firstrade`), longest token first.
    Whole-token matching is the point: `cash-flow.csv` must not read as Crédit Agricole because it
    starts with `ca`.
24. **An unrecognised file in intake is left completely alone** — not parked, not logged as an error,
    not moved. Intake is shared space; the service's only licence there is to pick up files it can
    name. Ambiguous names (two different source tokens) are also left alone.
25. **A recognised file is moved — not copied — into `<dump>/<source>/`** and the normal pipeline
    takes over from there. Moving is what makes intake self-clearing, which is the whole point of the
    feature; the existing `move_file` (`watch.rs:377`) already handles the cross-filesystem `EXDEV`
    case, which matters here because intake will often be on a different mount than the state dir.

### Review UI
26. **The file review leads with the preview, not with rows.** The page opens with what the import
    will do — "2 files · 1 account to create · 51 transactions · 3 transfer pairs merged · 7
    duplicates suppressed · 15 informational · 0 errors" — and the per-account and per-entry detail
    sits underneath. The summary is the preview response, re-fetched whenever the selection or an
    account override changes, so the number on screen is always the number the button will produce.
27. **Accounts are grouped by identity, never by statement**, carrying the span of all their
    statements, the files that feed them, the total pending count and the resolution state stated
    **once**: "uses existing account X", or "will be created as X (investment)".
28. **Each account row gets an existing-account picker.** No server change is needed:
    `ImportRequest.accountId` (`store.rs:489`) is already honoured as `statement_account_override`
    (`store.rs:967`) and `StatementImport` flattens it, so the UI sends the chosen id for each
    statement in that account group. Tombstoned accounts are filtered out of the options rather than
    rejected after the fact by `ensure_account_live`.
29. **Merges are shown as merges.** A transfer pair renders as one row naming both accounts
    (`date · out → in · −X → +Y`), and a duplicate group renders as one row stating which file the
    surviving line came from — rather than two unexplained rows the user has to reconcile mentally.
30. **Entry sections are per account**, with month subheadings when an account spans months — the
    information the month split was carrying is kept as *presentation*, which is where it belonged.
31. **The list's group header counts distinct accounts**, not statements
    (`maia-ui/src/views/statements.ts:144`), so one file reads "1 account · 51 entries pending".
32. **Account identity is editable from the review flow too**, not only from the accounts screen —
    the picker's companion action is "set this file's number on an existing account", which calls
    `PUT /accounts/:id/identity` and re-previews. This is the shortest path out of the state the user
    hit.

## Success criteria
- [x] Dropping the committed `fixtures/statements/firstrade.csv` stages **one** statement, whose
      period spans the whole file, and the file review shows **one** account row
- [x] Dropping both `t212` exports stages **two** statements (one per file, one account each), the
      overlapping February lines appear as `duplicate` on the second, and no line is deleted by the
      second staging — the regression test asserts the February line count directly
- [x] Two files of the same source and account produce **one** "will be created" account in the
      preview, listing both files as its sources
- [x] An account created by an import has a `Uuid::now_v7()` id, not a derived one, and a re-import
      of the same file reuses that row by `(source, external_id)` lookup rather than by id
- [x] `PUT /accounts/:id/identity` sets a number on a hand-created account; a conflicting number
      returns 409 naming the other account; a phone pull/push round trip neither reads nor clears it
- [x] Editing an imported account's name, type, opening balance, note **and** number leaves its id
      unchanged, keeps every transaction attached to it, and survives a subsequent import of the same
      file untouched — including the case where the number is moved to a different account
- [x] After setting the number, previously staged statements of that identity report "uses existing
      account" with no further user action
- [x] An identity with no matching account lists candidate existing accounts as suggestions; the
      server never reuses an account that was not matched exactly or confirmed by the user
- [x] `POST /statements/preview` returns, for the same body `/statements/import` accepts: accounts to
      create vs reuse, transactions to create, transfer pairs merged (naming both accounts),
      duplicates suppressed (naming the surviving file), informational lines, and per-line errors —
      and importing immediately afterwards produces exactly those numbers
- [x] A missing USD/EUR FX rate shows in the preview as a per-line error before the import is clicked
- [x] A preview leaves the database byte-identical (asserted by a test, including no account created)
- [x] `parsed/` only ever holds files whose statements are all imported; a freshly staged file sits in
      `staged/`, and discarding its statements moves it to `discarded/`
- [ ] First boot after the change relocates the existing mislabelled `parsed/firstrade/fristrade.csv`
      and the two `parsed/trading212/*.csv` to `discarded/` (their statements are soft-deleted)
      — *the mechanism is tested; this is the deployed run of it, see Remaining manual steps*
- [x] A user-set intake directory is editable from Settings (not *(managed)*), and a file named
      `firstrade-….csv` dropped there is moved into `dump/firstrade/` — *the move and the Settings
      field are tested; "and staged with no further interaction" needs the live watcher, see
      Remaining manual steps*
- [x] `cash-flow.csv` and `statement.csv` in intake are untouched after a full scan
- [x] Entry checkboxes align with their row content, and select-all works on the list and inside one
      file review (inherited `statement-review-ui-fixes` T03/T04)
- [x] `cargo test -p maia` (the crate is `maia`, not `sync-server` — the plan inherited a stale
      name), `cargo test -p maia-ui`, `npx tsc --noEmit`, `npx vite build`, `npx vitest run`,
      `nix build .#maia-desktop` all green

## Constraints and non-goals
**Constraints:**
- The desktop stays a thin client; the server remains the only database writer, and the preview is
  computed server-side so the UI never re-derives import semantics
- `accounts.source`/`external_id` stay off the phone sync surface: `pull_accounts` and
  `push_budget_accounts` keep their explicit column lists untouched
- No schema change is expected — `source_key` is a value, the lifecycle roots are config plus
  filesystem, and `source`/`external_id` already exist. Confirm before adding a migration
- Deletion stays soft and keeps imported transactions
- The four lifecycle roots stay deployment-owned; only intake is overlay-owned
- `SourceId::parse`'s alias set is the single source of truth for source naming; `from_file_name`
  consumes it rather than restating it

**Non-goals:**
- No mobile changes — statement review is desktop-only, and account identity stays invisible to it
- No content sniffing, and no `<intake>/<source>/` subfolder convention (decided: file name only)
- No currency column on `accounts`
- No per-entry delete (skip still excludes a line)
- No change to the Trading 212 API feed's month buckets
- No parameterising of the hardcoded Firstrade/T212-CSV account identity (see Risks)
- No automatic FX rate fetching to satisfy the preview's USD lines — reporting the gap is this plan's
  scope, filling it is not
- No closing of the utoipa statements documentation gap

## Assumptions
- Existing staged Firstrade/T212 statements are expendable: all 54 are already soft-deleted, so
  re-keying `source_key` strands nothing the user wants. **Verify before starting** — if any live
  statement of a file source exists by then, re-dropping the file is the recovery path, not a
  migration.
- The user will rename exports before dropping them into intake; the dump tree's `<source>/`
  subfolders remain the no-rename path for anyone who prefers dragging.
- "Entries about to be merged" means both kinds of collapse the import already performs — duplicate
  `dedupe_key` suppression and transfer-pair linking. If a third kind is wanted (e.g. merging two
  lines the user judges to be the same despite different fingerprints), that is a separate plan.

## Task stack

- [x] T01: One statement per account per file (status:done)
  - Goal: Firstrade and T212-CSV adopt CA's `<source>:<account>:<source_hash>` key and stop
    splitting by month.
  - Boundaries: In — `firstrade.rs` (drop `by_month`, span from line dates, `source_key`),
    `t212_csv.rs` (same, drop `group_by_month`), keep `month_key`/`month_bounds` for the feed.
    Out — `t212.rs` feed keys, `ca.rs`, the store, the UI.
  - Note: collapsing also fixes an incidental bug — `firstrade.rs:143` clones the whole `warnings`
    vector onto *every* month statement, so one bad row currently reports N times.

- [x] T02: Regression test for the overlapping-export data loss (status:done)
  - Goal: prove the February clobber cannot recur.
  - Boundaries: In — a store-level test staging two overlapping file drafts in either order and
    asserting both keep their lines, with the repeats marked `duplicate`. Out — fixing `dedupe_key`
    itself (it already works; it was being bypassed).

- [x] T03: Decouple the account id from the provider identity (status:done)
  - Goal: an imported account is an ordinary row whose id means nothing, so every field becomes
    editable without moving where the next import looks.
  - Boundaries: In — `resolve_account` mints `Uuid::now_v7()` instead of
    `stable_uuid(ACCOUNT_NS, …)`, `ACCOUNT_NS` retired from `src/statements/mod.rs:37`, idempotency
    carried by `idx_accounts_external`, a test that a re-import reuses the row by lookup and a test
    that an edited name/type survives it. Out — the identity endpoint (T04), any schema change.
  - Note: this must land before T04 — editing identity is only safe once the id stops depending on
    it. Check the existing `ON CONFLICT(id) DO NOTHING` insert still behaves under a random id (the
    conflict target that matters becomes the partial unique index, not the primary key).

- [x] T04: Editable account identity (status:done)
  - Goal: a hand-made account can be told which provider account it is, and an imported one can be
    corrected.
  - Boundaries: In — `PUT /accounts/:id/identity`, the `idx_accounts_external` 409 naming the
    conflicting account, re-resolution of live statements' `account_id` after an edit, a test proving
    pull/push still neither read nor clear the columns, a test that moving an identity strands no
    transactions, desktop command + accounts-view edit fields for number/source.
    Out — `/budget-accounts`, the phone, any currency field, account-to-account merging.

- [x] T05: Import preview endpoint (status:done)
  - Goal: one server answer to "what will this import do?", identical to what import does.
  - Boundaries: In — `POST /statements/preview` sharing `BatchImportRequest`, transaction rollback,
    `ImportOutcome` extended with accounts-to-create/reuse, duplicate groups, transfer pairs,
    informational lines and per-line errors; cross-file account grouping by `(source, external_id)`;
    candidate suggestions for an unmatched identity; a no-side-effects test. Out — new import
    semantics; the preview must change *nothing* about how importing behaves.

- [x] T06: `staged/` and `discarded/` roots with derived placement (status:done)
  - Goal: `parsed/` means imported.
  - Boundaries: In — `config.rs` (`statements_staged_dir`, `statements_discarded_dir`, defaults
    beside the dump root like the existing two), `watch.rs` (`Destination`, `destination_root`,
    `create_trees`, `lifecycle_roots` → 5 roots, `ingest_file` lands in `staged/`), new
    `reconcile_files` + its `repoint_rows` calls, hooks after import and delete commit.
    Out — hard deletion, any new HTTP endpoint, the preview path (decision 18).

- [x] T07: User-owned intake directory (status:done)
  - Goal: drop a correctly named file anywhere convenient and have it picked up.
  - Boundaries: In — `config.rs` overlay-merged `statements_intake_dir` + accessor,
    `SourceId::from_file_name` with whole-token matching, intake drain in `watch.rs` (inotify watch +
    sweep), move-not-copy into `<dump>/<source>/`, leave-alone for unrecognised and ambiguous names,
    Settings field (editable, not `managed`) plus the two new managed roots shown read-only.
    Out — parsing in place, sniffing, subfolder routing.

- [x] T08: Preview-led file review (status:done)
  - Goal: the page states what will happen, in the import's own words.
  - Boundaries: In — the summary header from the preview response, re-fetch on selection and
    override change, accounts grouped by identity with files listed and one resolution label,
    merge rows for transfer pairs and duplicate groups, per-line error display, distinct-account
    count in the list header. Out — the single-statement detail page's own layout.

- [x] T09: Account overrides and identity edits from the review (status:done)
  - Goal: fix a wrong account proposal without leaving the screen.
  - Boundaries: In — per-account existing-account picker (tombstoned accounts filtered) wired into
    the batch import's per-statement `accountId`, the "set this number on an existing account" action
    calling T03's endpoint, re-preview after either. Out — creating an account from scratch here (the
    accounts screen owns that).

- [x] T10: Inherited UI fixes — checkbox alignment and select-all (status:done, 2026-11-10)
  - Goal: finish `statement-review-ui-fixes` T03/T04 in the file they already belong to.
  - Boundaries: In — `lineRow`/table cell alignment, a working select-all-pending inside a file
    review and select-all-pending on the list, indeterminate states kept in sync. Out — the grouping
    diagnosis (answered above).
  - Outcome: `lineRow` was already aligned; the misalignment was the accounts table's default
    `vertical-align: middle` over a two-line cell (~9px, ~25px zoomed) and the list row's `mt-2`.
    Select-all existed at both levels but selected every `pending` line including ones rendered
    without a checkbox; selection now comes from `src/lib/statementReview.ts::selectableLineIds`,
    which the render consumes too. Measurements and screenshots in `statement-review-ui-fixes`.
  - Note for T08/T09: a pending line whose transfer counterpart is already settled still has **no**
    checkbox anywhere, so it cannot be imported from this screen. Honest now (select-all no longer
    pretends to pick it), but the preview-led rewrite should give it a control.

- [x] T11: Docs and packaging (status:done)
  - Boundaries: In — mark `statement-review-ui-fixes` superseded, register this plan in
    `context/context-map.md`, add the lifecycle/intake/preview model to `context/overview.md`, full
    test and `nix build` runs. Out — utoipa.

## Deviations from the plan
- **Firstrade's fingerprint keys on the row's day, not the month.** The plan said to drop the month
  split; it did not say what to do about `firstrade.rs`'s fingerprint, which *included* the month and
  so stopped being unique once a whole file became one statement — two identical rows a month apart
  would have collided under `UNIQUE(statement_id, fingerprint)` and the second would have been
  dropped silently. The day is the finest grain the export carries and is stable across re-drops.
  Pinned by `identical_rows_in_different_months_keep_distinct_fingerprints`.
- **`SourceId::from_file_name` matches adjacent *runs* of tokens, not only single tokens.** Whole-
  token matching alone cannot see `credit-agricole`, because the separator joining the two words is
  one of the separators the stem is split on. Runs are tried longest-first, which is what decision 23
  meant by "longest token first".
- **The preview describes an account it created inside the rollback as if it did not exist**, because
  from the user's side it does not: no id, and suggestions still listed. The first implementation
  reported the minted id and dropped the suggestions, which two tests caught.
- **`reconcile_files` excludes `status='error'` rows explicitly.** The plan reasoned that a parked
  file "has no statement rows at all, so it will not appear in a `stored_path` walk — confirm that is
  true rather than assuming it". It is not true: `record_failure` writes an `error` row carrying
  `stored_path`, so a file parked in `failed/` after a failed re-parse *would* have been dragged into
  `discarded/` on every boot. Excluded by the query and pinned by
  `an_error_statement_does_not_move_its_parked_file`.
- **`settle_files` takes the pooled connection by value.** The plan flagged that a second connection
  inside a *preview* would deadlock the one-wide test pool; the same applies to the reconcile hooks
  in the import and delete handlers, which run after a commit while the handler still holds its
  connection. Taking it by value makes dropping it first the only way to call the function.
- **`read_config` now resolves the lifecycle roots.** The managed statement directories were rendered
  read-only in Settings but were only ever populated from the overlay, so on a default install their
  boxes were blank. Adding two more blank boxes would have been worse; the command returns the
  resolved paths instead, and its overlay merge became per-key so `statements.intake_dir` and the
  Trading 212 feed settings still come through.
- **Per-line type/category edits moved out of the DOM.** Not in the plan, but the debounced
  re-preview made a pre-existing bug reachable in a new way: every selection change rebuilds the page
  and reset those fields, and a preview landing 250ms later could now do it mid-typing. They live in
  a `lineEdits` map, with focus and caret restored after a rebuild.
- **T09 shipped inside T08's commit.** The account picker, the "it is that account" action and the
  re-preview after either are not separable from the preview-led review they live in.
- **The `maia-ui` crate name in the success criteria was stale** (`sync-server` → `maia`), and the
  plan's own `cargo test -p maia-ui` refers to the desktop Tauri crate, which is a different thing
  from the frontend's `vitest` suite. Both were run.

## Evidence
- `cargo test -p maia`: 222 passed, 1 ignored (was 184 before this plan; 38 added).
  - T01: `parses_the_committed_fixture` (1 statement, 2026-06-15 → 2026-10-15),
    `a_multi_month_file_is_one_statement_keyed_by_its_bytes`,
    `identical_rows_in_different_months_keep_distinct_fingerprints`.
  - T02: `overlapping_exports_keep_every_line_when_the_later_file_stages_second` and
    `..._in_the_other_order_too`, both asserting the February line count directly. **Verified to
    bite**: temporarily restoring the month-scoped `source_key` makes both fail, the second with
    exactly the shape of the original data loss. The two exports are synthetic overlapping CSVs
    rather than the user's own pair, which are not in the repository.
  - T03: `an_imported_account_gets_a_minted_id_and_is_reused_by_identity`,
    `an_edited_account_survives_a_later_import`,
    `the_same_identity_resolved_twice_in_one_transaction_creates_one_account`.
  - T04: 7 handler tests, including `a_conflicting_number_is_refused_with_the_account_that_holds_it`
    (409 naming it), `moving_a_number_to_another_account_strands_no_transactions` (the whole
    clear-then-set move) and `a_phone_round_trip_neither_reads_nor_clears_the_identity`.
  - T05: 10 tests, including `a_preview_leaves_the_database_untouched` (every row of `accounts`,
    `transactions`, `bank_statements`, `bank_statement_lines`, `transfer_matches` compared before and
    after), `importing_afterwards_produces_the_previewed_numbers`, and
    `the_payload_keys_are_the_ones_the_desktop_reads` — the only thing connecting the Rust payload to
    `maia-ui/src/lib/api.ts`'s `ImportPreview`.
  - T06: 6 reconcile tests, including the `parsed/` → `discarded/` migration, idempotency, self-heal
    after a hand-move, and the two `failed/` cases.
  - T07: 4 intake tests plus 2 on `from_file_name`; `an_unrecognised_file_in_intake_is_left_completely_alone`
    drops five files and a subdirectory it must not touch and asserts all six survive.
- `cargo test -p maia-ui`: 28 passed, including the invoked-vs-registered command cross-check, which
  covers the new `preview_statements` and `set_account_identity`.
- `npx vitest run`: 33 passed. `npx tsc --noEmit` and `npx vite build` clean.
- `nix build .#maia-desktop` green.
- The rebuilt file review was rendered headlessly against the real built stylesheet and inspected:
  summary header, identity-grouped accounts with their suggestion buttons, the FX error section,
  merge rows, month subheadings and the sticky action bar all lay out correctly.

## Remaining manual steps
1. `nix flake update maia` in `~/.dotfiles`, then `sudo nixos-rebuild switch --flake ~/.dotfiles#<host>`
   to pick up the new server and desktop.
2. **On the first boot after that**, confirm `reconcile_files` relocated the real mislabelled files:
   `parsed/firstrade/fristrade.csv` and the two `parsed/trading212/*.csv` should move to
   `discarded/`, since their statements are soft-deleted. Nothing should be left in `parsed/` that
   has not been imported.
3. Set an intake directory in Settings (`~/Downloads` is the obvious one), drop a
   `firstrade-2026-11.csv` into it, and confirm it is moved into `dump/firstrade/` and staged without
   pressing Scan — the inotify watch on intake is the part no test can cover.
4. Re-drop the Firstrade and Trading 212 exports and confirm the review shows **one** account each,
   with the preview summary agreeing with what the import then does.
5. The all-USD Firstrade export will report missing USD/EUR FX rates in the preview. Filling them is
   deliberately out of this plan's scope — add the rates under FX rates before importing it.

## Risks / gotchas
- **The hardcoded account identity is a latent bug this plan does not fix.** Firstrade uses the
  literal `"firstrade"` as `account_number` and the T212 CSV uses `"csv"`, so two different accounts
  at the same broker would collapse into one ledger account — the mirror image of today's complaint.
  Right now it is the behaviour that makes "one account per file" work, so it stays; if a second
  account at either broker ever appears, this is where to look. Firstrade's export carries only an
  `Account Type` column (`Cash`/`Margin`), not an account number, so there is nothing better to key
  on from the file alone.
- **Dropping the deterministic account id is the load-bearing change; sequence it first.** While the
  id is `stable_uuid(ACCOUNT_NS, "account:<source>:<external_id>")`, editing a number silently
  re-points the next import at a *different* id, so an identity edit shipped before T03 would strand
  the account's transactions. T03 before T04 is not a preference. Also re-read the
  `ON CONFLICT(id) DO NOTHING` insert (`store.rs:1250`): with a random id, the primary key can no
  longer be the conflict target that matters, and the guard has to come from
  `idx_accounts_external` — verify with a test that stages the same identity twice in one
  transaction.
- **Account ids already in the database stay as they are.** The existing CA accounts hold v5 ids
  derived from their numbers (e.g. `111c13af-…` for `84890097587`). Nothing needs to change about
  them — the lookup path finds them by `(source, external_id)` either way — but do not write a
  migration that "fixes" their ids: transactions, statements and the phone all reference them.
- **The preview must stay genuinely side-effect free.** `import_statement` writes line statuses,
  `mark_error`s, account rows and transactions; all of it is inside the transaction, but anything the
  *handler* does around it (file reconcile, notifier, `refresh_statement_status` on a second
  connection) would escape the rollback. The pooled test connection is `max_size(1)` per
  `context/overview.md`, so a second connection inside a preview would also deadlock — which is a
  useful early warning, not a trap to work around.
- **A preview is not free on 721 lines.** The T212 pair matching loads every line of a statement to
  expand counterparts (`store.rs:631-639`); re-previewing on every checkbox click would be wasteful.
  Debounce the re-fetch, or scope the preview to the file being reviewed rather than everything
  staged.
- **`lifecycle_roots` is load-bearing beyond `create_trees`.** Growing it from 3 to 5 changes which
  directories the tree creation walks; check every caller and the inotify watch set so the service
  does not end up watching `staged/` or `discarded/` and re-ingesting its own moves.
- **Intake will often sit on a different filesystem** than the state dir (`~/Downloads` vs
  `~/.local/share`), so the `EXDEV` copy-then-unlink fallback in `move_file` is the normal path here,
  not the exotic one. It is tested (`watch.rs:624`), but the intake drain must not assume `rename`
  succeeded.
- **`reconcile_files` must be idempotent and must not fight `park`.** A file in `failed/` has no
  statement rows at all, so it will not appear in a `stored_path` walk — confirm that is true rather
  than assuming it, or a failed file could be dragged into `discarded/` on every boot.
- **Re-keying `source_key` strands existing rows under the old key.** Harmless today (everything
  affected is soft-deleted) but re-check at implementation time; the old rows are invisible in the
  awaiting-approval view either way.
