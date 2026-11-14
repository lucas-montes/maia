# Context Map

## Overview
This file indexes all durable context artifacts for the Maia project.

## Core Context Files
| File | Purpose |
|---|---|
| `overview.md` | Project overview, architecture, key decisions |
| `glossary.md` | Domain-specific terms and definitions |
| `context-map.md` | This file — index of all context |

## Domain Context Files
| File | Purpose |
|---|---|
| `nix/packaging.md` | How the `server` flake packages (`sync-server`, `frontend`, `maia-desktop`) are built: crane deps caching, pnpm frontend, crane-tauri app, utoipa-swagger-ui download vendoring, build-environment gotchas |
| `ui-rebuild/shell-and-screens.md` | UI shell, core screen implementations, settings editor, and notes file-backed records |
| `sync/sync-contract.md` | FitFat offline-first sync contract (mirrors `~/Projects/fitfat/docs/api/openapi.yaml` and `context/sync/sync-contract.md`) |
| `statements/account-tree.md` | The account tree: roots (institution, kind, source) and leaves, the `cash`/`savings`/`securities` vocabulary, how an import finds or makes a root, setting an identity without breaking the tree, the desktop's view of it, and where the code lives |
| `statements/entry-lifecycle.md` | Statement entry status model: which statuses are user decisions and which are derived, what a live statement is and what it claims, the staging/re-read branches, retrying a failed entry, the review's empty state, what a parser records, where a file sits (`staged`/`parsed`/`discarded`), and the known gaps |
| `waybar-agenda.md` | `GET /agenda` response shape + copy-pasteable Waybar `custom` module config and `curl`/`jq` one-liner |

## Plans (proposed)
| Plan | Status | Description |
|---|---|---|
| `health-calorie-tracking` | proposed | AI receipt → nutrition, meal logging, nutrition goals |
| `health-exercise-tracking` | proposed | Workout sessions, set logging, body weight, progress charts |
| `finance-expenses-income` | proposed | Tx/investment CLI, bank/investment CSV parsing, receipt→expense link |
| `finance-budget-simulation` | proposed | Budget tracking, auto-categorization, Monte Carlo sim integration |
| `system-daemon-wiring` | proposed | DB init, main loop, socket handler, file watcher fixes (FOUNDATIONAL) |
| `system-waybar` | superseded | Socket-era daemon sketch; replaced by `waybar-and-notifications` (HTTP `/agenda`) |
| `waybar-and-notifications` | completed (T03 unverified end-to-end) | `GET /agenda` + Waybar module doc + background `notify-send` due-task notifier + systemd D-Bus session wiring |
| `system-remote-storage` | proposed | S3/local sync for DB and watched files |
| `tasks-sqlite-port` | proposed | Replace JSON todo storage with SQLite, wire new CLI, notifications |
| `todo-txt-import` | proposed | Watch todo.txt files, parse tasks, save them, and notify on due dates |
| `calendar-google` | proposed | Google Calendar OAuth, event fetch/create, reminders |
| `personal-meetings` | proposed | Meeting start/stop, notes, audio recording, markdown export |
| `ui-visualization` | proposed | Embedded HTTP server + web dashboard with charts |
| `ui-rebuild` | completed | Sober responsive desktop UI rebuild with files, SQLite indexing, and saved URLs |
| `kb-chrome-extension` | proposed | Wire Chrome → native host → daemon → SQLite pipeline |
| `kb-search` | proposed | FTS5 full-text search, vector embeddings, knowledge graph |
| `fitfat-sync-server` | completed | Standalone `sync-server` crate (Axum+rusqlite) + full FitFat OpenAPI + Tauri 0.0.0.0:3030 deterministic LAN embedding |
| `maia-headless-server` | completed | Headless `sync-server` binary (`serve`/`migrate`/`status`) + `PRAGMA user_version` migrations with snapshots + env config + NixOS service `services.maia` |
| `maia-desktop-split` | completed | Desktop app stops hosting the server and becomes a client of it; config moves from `MAIA_*` env vars to a Nix-generated `/etc/maia/config.json` plus a per-user overlay; `maia-desktop` packaged and installed by `services.maia.desktop.enable` |
| `desktop-entry-and-state-dir` | completed | Ship the `.desktop` inside the package (`/etc/applications` is off `XDG_DATA_DIRS`, so rofi never saw it); drop the fatal `ReadWritePaths` for a not-yet-created `StateDirectory` |
| `service-user-ownership` | completed | Service runs as the desktop user instead of a `maia` account; drops the group, setgid directory and umask coordination, and reverts an `ExecStartPre` reorder that deadlocked the unit |
| `fitfat-visualization` | completed | HTTP-leveraged `GET /workouts|/meals|/body-metrics` + Chart.js 4 canvases + per-chart filters + Settings QR auth token |
| `fix-fitfat-posts` | completed | Fix all 12 FitFat POSTs (camelCase + `deleted[]`), DB nuke to v30 single `maia.db`, UUID v7, missing GET pulls (templates/notes/tasks/goals/transactions/budget-accounts/receipts/tags), utoipa at `/docs`, WebKit WebVTT gst-plugins-bad |
| `ui-unified-tabs` | completed | Grouped collapsible sidebar (Training/Nutrition/Health/Planning/Finance) + 13 tabs, virtual scroll, lazy media, per-tab sync |
| `ui-vanilla-rewrite` | completed | `maia-ui` monolith rewritten as modular vanilla TS (daisyUI dashboard shell, `shell/` + `views/` per screen, no framework) |
| `sync-ui-bugfixes` | completed | Desktop reads TEXT-uuid notes + `done/taskStatus` filtering, clickable workout detail, labeled set columns; `POST /tags` + `GET /templates` with nested sets; mobile pushes tags/stores/foods |
| `statement-intake-and-account-merge` | completed | One statement per account per dropped file (file sources adopt CA's `<source>:<account>:<source_hash>` key, month buckets stay feed-only), fixing a 34-statements-for-1-account review and an overlapping-export data loss; `POST /statements/preview` = the real import run in a rolled-back transaction, reporting accounts to create vs reuse across all files, transfer pairs and duplicate merges, and FX errors before the click; account ids stop being derived from `(source, external_id)` (`Uuid::now_v7()` + the partial unique index instead of `stable_uuid(ACCOUNT_NS, …)`), so an imported account is an ordinary editable row and `PUT /accounts/:id/identity` can change its number without stranding transactions or touching the phone sync surface; unmatched identities become suggestions, never silent reuse; `staged/`+`discarded/` lifecycle roots so `parsed/` means imported, placement derived by a `reconcile_files` pass; user-owned intake directory routed by file name; preview-led, account-grouped file review |
| `statement-review-ui-fixes` | superseded (T03/T04 shipped) | File-grouping diagnosis (T01/T02) answered — `sourcePath` was always reaching the client. Checkbox alignment and select-all (T03/T04, also the successor plan's T10) are done: the accounts table's cells are top-aligned (a two-line cell was centring its checkbox ~9px below the account name) and the list row's offset corrected; select-all, the per-account checkbox and the rendered checkboxes all read `src/lib/statementReview.ts::selectableLineIds`, so a selection can no longer include entries the page shows no checkbox for |
| `statements-file-review` | completed | Statements grouped by dropped file and defaulted to awaiting approval; file review page lists accounts (existing vs created on import), entries by account, flattened transfer rows naming both accounts; checkbox multi-select with atomic batch import (`POST /statements/import`) and batch soft delete (`POST /statements/delete`); `GET /statements/file` one-snapshot endpoint; drop form places CSVs in the dump folder without triggering a scan |
| `finance-model` | in progress (Stage 1 + Stage 2 complete; T9–T13 done) | Three stages. **Statements:** an entry's `imported`/`skipped` status is a user decision; `pending`/`informational`/`duplicate`/`error` are derived and recomputed on every re-read; discarded and ignored statements never claim content; `parsed/` requires every entry imported or skipped; migration v12 repairs duplicates claimed by discarded statements; Firstrade warns on unreadable rows. **Account tree:** `account_roots` (kind `bank`/`broker`/`neobank`, server-only) with leaves `accounts.root_id`, `type` renamed to `cash`/`savings`/`securities`, identity kept on the leaf, phone push stops writing `type`. **Trades:** booked as their cash leg (one transfer cash leaf ↔ securities leaf at the broker's amount) with symbol/side/quantity/price kept; positions, average cost and realised gain derived, with an optional opening-position seed; a gain is never income. Server and desktop only. Entry lifecycle documented in `context/statements/entry-lifecycle.md` |

## Decisions
| Decision | Date | Summary |
|---|---|---|

## External References
- `ROADMAP.md` — High-level feature roadmap
- `STORYLINE.md` — Historical development notes
- `README.md` — Usage guide
