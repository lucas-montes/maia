# Statements: review by dropped file, with bulk import and delete

## Change summary
The Statements screen listed one row per *statement*, but the parser stages **one statement per account** from a CA export, so dropping a single file produced several rows with no visible connection between them. The detail page then split lines into three vaguely labelled sections ("Transfers" as nested cards, "Trades", "Lines"), which made it hard to tell what each entry represented.

The list now defaults to what is **awaiting approval** and groups statements under the **file** they were staged from. Clicking a file opens a file review page that answers "what will this import create?": the accounts (existing vs created on import), the entries to add grouped by account, transfers flattened to one out → in row naming both accounts, and informational trades. Selection is shared across the file so a batch import covers every account in one server transaction, and statements can be soft-deleted in bulk.

Related sibling feature from the same work stream: the "Add statement…" drop form (`804ca27`) writes picked CSVs into the dump folder without triggering a scan.

Commits: `804ca27` (drop form), `fc0936b` (rustfmt-only split of `maia-ui/src-tauri/lib.rs`), `61cd503` (this feature).

## Decisions

### Grouping and defaults
1. **Group by `source_path` (the dropped file), not by period or source.** A file is the unit the user actually handled; one CA export legitimately stages several statements (one per account section), and grouping by file is what makes those read as one import. Statements without a source path (Trading 212 feed syncs) stand alone under a pseudo-group keyed by statement id.
2. **Default list view is "Awaiting approval" = `staged` + `partial` + `error`.** Parse failures are surfaced as "needs attention" rather than hidden. The list endpoint filters by a single status, so the multi-status view is filtered client-side; "All statuses" and the individual statuses remain available for history.

### File review page
3. **The file review is the primary review surface; per-account detail remains reachable.** Clicking an account row opens the existing single-statement detail; back returns to the file review (not the list), because the file page is the context the user came from.
4. **Accounts are shown before entries**, each with an explicit status: "uses existing account X" when `account_id`/`resolvedAccountName` is set, "**will be created on import**" otherwise. This replaces the old "Budget account: created on first import" wording, which did not say what would happen or to which account.
5. **Transfers are flattened to one row**: `date · out-account → in-account · −X → +Y · confidence · effect`, with the explanation that an automatic pair imports as one transfer transaction and a suggested one stays two entries unless accepted. The old two-column nested cards were the "nested entries without being clear what each represents". The server joins both statements so a match now carries `debitAccountName` and `creditAccountName`.
6. **Trades stay informational and are never imported** (T212 only), labelled as such on both pages.

### Selection and bulk actions
7. **One shared entry selection across the whole file.** An entry checkbox toggles that line; an account checkbox toggles all pending entries of that account (indeterminate when partial). The list adds statement checkboxes and a file checkbox (indeterminate when partial) for cross-file bulk actions.
8. **Bulk import uses the existing `POST /statements/import` batch endpoint** — one server transaction across all accounts, with `autoLink: true` so matched transfer pairs import as single transactions. Looping the single-statement endpoint was rejected: partial failure would leave a file half-imported.
9. **Delete is statement-level, batched, and soft** via the new `POST /statements/delete` (one transaction; imported transactions are kept). Individual entries are *not* deletable: the server has no line-delete concept, so "skip" is the way to exclude an entry without losing the statement.

### Server surface
10. **`GET /statements/file?path=…` returns every statement of one file in one snapshot** (statement + lines + matches), rather than the desktop fetching each statement detail (N+1) and risking a torn view. The detail assembly was extracted into `statement_payload`, shared by the single-statement and file endpoints.
11. **The list endpoint now carries `sourcePath`** so the client can group without a second call. No schema change.
12. **Statements routes are not added to the utoipa OpenAPI doc** — the existing statements handlers are not documented there either; this feature does not widen that gap.

### Drop form (sibling feature, `804ca27`)
13. **Placement only, no scan trigger.** Picked files are copied (never moved) into `<dump dir>/<source>/` and the server's inotify watcher (with the 15-minute sweep as backstop) stages them. The desktop command neither calls `POST /statements/scan` nor refreshes the list.
14. **No new dependencies for the file picker.** A webview `<input type="file">` sends `{name, base64}` through a Tauri command; the dump directory comes from the shared `sync_server::Config` (the desktop already loads the same config), and the URL query value for the file endpoint is percent-encoded by a small hand-rolled helper (`url_query_value`) instead of adding a crate.
15. **Filenames are reduced to their final path component** (POSIX and Windows separators), extensionless/dot-only names are refused, and collisions get the same `-2`/`-3` suffix the server uses.

## Success criteria
- [x] The list defaults to awaiting-approval statements and groups them by dropped file
- [x] One CA file shows as one group with one row per account
- [x] Clicking a file shows accounts (existing vs created on import) and entries grouped by account
- [x] Transfers render as one row per pair, naming both accounts, without nested cards
- [x] Entry/account/file checkboxes support multi-select import and delete
- [x] Import and delete are atomic per action (one server transaction)
- [x] No scan is triggered by the drop form; the watcher stages files
- [x] `cargo test -p sync-server`, `cargo test -p maia-ui`, `tsc --noEmit`, `vite build`, `nix build .#maia-desktop` all green

## Constraints and non-goals
**Constraints:**
- Parsing and staging behaviour is unchanged; only the review surface and the two new endpoints are new
- The desktop remains a thin client; the server stays the only database writer
- Deletion is soft and keeps imported transactions

**Non-goals:**
- No phone UI changes
- No per-entry delete (skip instead)
- No drag-and-drop onto the window (the file input covers the flow)
- No changes to the utoipa statements documentation gap
- No grouping by month/period within a source

## Task stack

- [x] T01: Drop-statement form (status:done)
  - Completed: 2026-11-03
  - Commit: `804ca27`
  - Goal: place picked CSVs in the correct dump folder without triggering a scan.
  - Boundaries: In — `add_statement_files` command, base64 decode, traversal-proof placement, collision suffix, source validation, last-source memory, `<details>` form in `maia-ui/src/views/statements.ts`. Out — staging/parsing, scanning, list refresh.

- [x] T02: Server endpoints for file snapshots and batch delete (status:done)
  - Completed: 2026-11-05
  - Commit: `61cd503`
  - Goal: one snapshot per dropped file and atomic multi-statement delete.
  - Boundaries: In — `statement_payload` extraction, `GET /statements/file`, `POST /statements/delete`, `sourcePath` in the list, `debitAccountName`/`creditAccountName` in matches, route registration, `ErrorBody: Debug` for test assertions. Out — schema changes, utoipa.

- [x] T03: File-grouped list and file review page (status:done)
  - Completed: 2026-11-05
  - Commit: `61cd503`
  - Goal: files first, then accounts and entries with clear labels.
  - Boundaries: In — awaiting-approval default, `groupStatements`, file/statement/entry checkboxes, `fileHtml`, flattened `transferRowsHtml`, `runFileImport`, `bulkImportSelected`, `deleteAccounts`, atom state (`fileReview`, `selectedStatements`, `lastDropSource`). Out — parser and server behaviour.

- [x] T04: Tests and packaging (status:done)
  - Completed: 2026-11-05
  - Commit: `61cd503`
  - Goal: prove the grouping, the cross-statement transfer payload and the batch delete.
  - Boundaries: In — two handler tests (`a_file_snapshot_covers_every_account_of_the_file`, `a_batch_delete_removes_only_live_statements`), full crate test runs, frontend type-check/build, `nix build .#maia-desktop`. Out — end-to-end tests against a live service.

## Deviations from the plan
- **`sourceFile` was dropped from the list payload.** The plan mentioned a filename field; the client derives it from `sourcePath`, so only `sourcePath` was added.
- **The file endpoint takes a full path, not a filename.** Filenames are not unique across dump folders (`dump/credit_agricole/export.csv` vs `dump/trading212/export.csv`), so `source_path` equality is the grouping key and the display name is derived.
- **`statement_payload` was extracted as a free function** taking `&rusqlite::Connection`, not a method on `AppState`, so both handlers and the tests can call it directly.
- **A rustfmt-only commit (`fc0936b`) precedes the feature.** The desktop `lib.rs` had never been formatted; mixing 1000+ lines of style churn into the feature diff would have buried it. `maia-ui/src-tauri/api.rs` was reverted afterwards: rustfmt follows `mod` declarations from the crate root, which would have pulled unrelated churn into the same commit.
- **Batch delete reports the ids it actually deleted** rather than a count, so the UI can tell when a file review has no accounts left and fall back to the list.

## Evidence
- `cargo test -p sync-server`: 129 passed, 1 ignored (was 127 + 1; two handler tests added).
- `cargo test -p maia-ui`: 18 passed, including the invoked-vs-registered command cross-check for `get_statement_file`, `import_statements`, `delete_statements`.
- `npx tsc --noEmit` and `npx vite build` clean (402 kB js).
- `nix build .#maia-desktop` succeeds offline.
- Manual flow verified against the code path: drop CA fixture → watcher stages one statement per account → one file group → review page lists both accounts and their entries, transfer pairs across accounts show both account names.

## Remaining manual steps
1. `nix flake update maia` in `~/.dotfiles` after each push — done for `61cd503`.
2. `sudo nixos-rebuild switch --flake ~/.dotfiles#<host>` to pick up the new desktop build.
3. Add the committed `sync-server/fixtures/statements/t212.csv` through the drop form (source Trading 212) and confirm it stages without pressing Scan, then check the file review page shows the account as "will be created on import".
