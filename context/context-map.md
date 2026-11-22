# Context Map

## Overview
This file indexes all durable context artifacts for the Maia project.

Every plan in `context/plans/` carries a `**Status:**` line directly under its
title. That line — not the checkboxes, and not this table — is the authoritative
statement of whether the work shipped. The checkboxes track the individual
tasks; a plan can be `completed` with a task still unchecked if that task was
verified only by inspection or is a live-deploy step.

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
| `ui-rebuild/shell-and-screens.md` | **Superseded.** Describes the pre-rewrite monolith UI; keep only for the settings-editor and file-backed-notes sections until they are rewritten |
| `statements/account-tree.md` | The account tree: roots (institution, kind, source) and leaves, the `cash`/`savings`/`securities` vocabulary, how an import finds or makes a root, setting an identity without breaking the tree, the desktop's view of it, and where the code lives |
| `statements/entry-lifecycle.md` | Statement entry status model: which statuses are user decisions and which are derived, what a live statement is and what it claims, the staging/re-read branches, retrying a failed entry, the review's empty state, what a parser records, where a file sits (`staged`/`parsed`/`discarded`), and the known gaps |
| `statements/trades.md` | Trades and positions: what each parser keeps (signed cash + side/symbol/quantity/price, identity unchanged), staging and re-reading a trade read without details, booking a trade as one transfer cash leaf ↔ securities leaf with a server-only `trades` row, average-cost positions and realised gain, opening positions, and where the code lives |
| `receipts/matching.md` | Receipts: never an expense; the phone only sends; the v2 reading; the rules every matcher follows (one candidate, source auto/ai/manual, effects, remembered undos); stores by tax number, learned name and brand with rename/new/same recommendations; transactions by exact amount, purchase day, card and shop; lines priced from learned names; Improve with AI; the endpoints, the desktop page and where the code lives |
| `waybar-agenda.md` | `GET /agenda` response shape + copy-pasteable Waybar `custom` module config and `curl`/`jq` one-liner |

The FitFat offline-first sync contract lives in the sibling repo, not here:
`mobile/context/sync/sync-contract.md` (it mirrors `~/Projects/fitfat/docs/api/openapi.yaml`).

## Plans

### In progress
| Plan | Status | Description |
|---|---|---|
| `settings-nix-and-models` | implemented, not committed or deployed | Settings rework. Non-secrets are set in `services.maia` and shown read-only in Settings (the editable `settingsFile` was removed 2026-11-22). Secrets (sync key, model keys, T212) live only in the overlay. All module paths have defaults: systemd `dataDir` for app state, XDG for user files. Several models (Gemini + OpenAI-compatible: Ollama, OpenRouter) with per-task fallback chains (`receipt_ocr`, `receipt_matching`, `statement_extraction`), and the model that produced each result is recorded (migration v18) and shown. The rollout order matters: save the sync key in Settings before rebuilding |
| `component-architecture-split` | not started | Make the desktop and the Chrome extension clients of the server instead of direct database readers, and extract `maia-ui/` and `chrome_ext/` into their own repos. The desktop Tauri crate still depends on the `maia` crate and still reads SQLite itself |

### Not started
| Plan | Description |
|---|---|
| `knowledge-base-similarity-search` | A hand-rolled TF-IDF index over notes and captured URLs, a cosine-similarity "find related" feature, a free-text search view, and a desktop knowledge-base screen |
| `meeting-recording` | A `meetings` table, server-side microphone capture, background transcription through the existing `ai` crate, and a desktop Meetings screen |
| `outlook-calendar-sync` | Microsoft Entra OAuth (PKCE, no client secret), a background token refresh and event sync, an events endpoint and a desktop Settings section |
| `remote-backup` | `GET`/`PUT /remote-backup/settings`, a staging-tree assembly, a scheduled upload task, a status endpoint and a desktop Settings section; `rclone` is not currently packaged |
| `day-to-day-recording` | Deliberately deferred idea capture for continuous lifelogging. Not an implementation plan — it has no task stack by design |

### Completed
| Plan | Description |
|---|---|
| `maia-headless-server` | The headless `maia` binary (`serve`/`migrate`/`status`) + `PRAGMA user_version` migrations with snapshots + env config + the NixOS service `services.maia` |
| `maia-desktop-split` | The desktop app stops hosting the server and becomes a client of it; config moves from `MAIA_*` env vars to a Nix-generated `/etc/maia/config.json` plus a per-user overlay; `maia-desktop` packaged and installed by `services.maia.desktop.enable` |
| `fitfat-sync-server` | The standalone sync-server crate (Axum+rusqlite) + the full FitFat OpenAPI + the Tauri 0.0.0.0:3030 deterministic LAN embedding |
| `fitfat-lan-ip` | The QR shows the real LAN address, not `0.0.0.0` or `127.0.0.1`; local fetches still use loopback |
| `sync-ui-bugfixes` | The desktop reads TEXT-uuid notes + `done`/`taskStatus` filtering, a clickable workout detail, labeled set columns; `POST /tags` + `GET /templates` with nested sets; the mobile pushes tags/stores/foods |
| `statement-intake-and-account-merge` | One statement per account per dropped file, fixing a 34-statements-for-1-account review and an overlapping-export data loss; `POST /statements/preview` = the real import run in a rolled-back transaction; account ids are no longer derived from `(source, external_id)`; unmatched identities become suggestions; `staged/`+`discarded/` lifecycle roots; preview-led, account-grouped file review. The mislabelled `parsed/firstrade/fristrade.csv` relocation is confirmed on the deployed box |
| `finance-model` | Three stages. **Statements:** an entry's `imported`/`skipped` status is a user decision; `pending`/`informational`/`duplicate`/`error` are derived and recomputed on every re-read; `parsed/` requires every entry imported or skipped. **Account tree:** `account_roots` (kind `bank`/`broker`/`neobank`, server-only) with leaves `accounts.root_id`, `type` renamed to `cash`/`savings`/`securities`, identity kept on the leaf, the phone push stops writing `type`. **Trades:** booked as their cash leg (one transfer cash leaf ↔ securities leaf at the broker's amount) with symbol/side/quantity/price kept; positions, average cost and realised gain derived, with an optional opening-position seed; a gain is never income. Server and desktop only. Deployed 2026-11-17 (database at v17); one manual step open on the live box: the INTC opening-position seed. Documented in `context/statements/entry-lifecycle.md`, `context/statements/account-tree.md` and `context/statements/trades.md` |
| `financial-scenario-simulations` | A `finance_scenarios` table, baseline derivation from the transaction history, a deterministic month-by-month projection engine, scenario CRUD over HTTP and a desktop Simulations screen with an overlaying chart |
| `desktop-task-calendar` | A read-only desktop calendar: month grid, week/day drill-down, client-side recurrence expansion (ported from the phone) and an up-next panel |
| `desktop-visualization-expansion` | An Insights view with six independent charts — per-exercise progression, aggregate training volume, calories, macros, spending by category and account balance — each on its own period toggle |
| `ingredient-price-tracking` | `ingredient_prices.source_url`, the desktop receipt line-item list, the ingredient price history and evolution chart, period price-per-100g aggregation, price-per-nutrient comparison, per-meal cost estimates and the food-spend rollup. Its "matching is always manual" rule is superseded by `receipt-matching` |
| `receipt-ocr-pipeline` | `POST /receipt-pictures/upload` (multipart) with a background Gemini vision job, `GET /receipt-pictures/:id/parse`, the `ParsedReceipt` schema, image serving, archive/restore and the mobile HTTP OCR service |
| `statements-file-review` | Statements grouped by dropped file and defaulted to awaiting approval; file review lists accounts (existing vs created on import) and entries by account; checkbox multi-select with atomic batch import and batch soft delete; `GET /statements/file`; the drop form places CSVs in the dump folder without triggering a scan |
| `ui-rebuild` | The sober responsive desktop UI rebuild with files, SQLite indexing and saved URLs |
| `ui-vanilla-rewrite` | The `maia-ui` monolith rewritten as modular vanilla TS (daisyUI dashboard shell, `shell/` + `views/` per screen, no framework) |
| `tauri-desktop` | The Tauri desktop shell itself: built, Nix-packaged as `maia-desktop` and installed by `services.maia.desktop.enable`. The generic filesystem command surface it proposed was never built |
| `tauri-desktop-files` | File-backed notes and receipts in the desktop: the receipt browser, the thumbnails, the archive/restore. The notes side is now read-only from the database |
| `receipt-matching` | Server-side matching, auto only when exactly one candidate fits (mutual), recorded as `auto` with Undo and remembered rejections: **receipt ↔ store** through server-only `store_aliases` (normalised merchant, whole-token fallback); **receipt ↔ transaction** by exact amount in the receipt's currency and the purchase day (the `dd/mm` in a card description, else a posting window), triggered after a parse, after an import and by a backfill; the desktop stops making drafts and gets "Book as expense"; **line ↔ ingredient** through per-store `product_aliases` learned from manual matches, with `receipt_line_matches` recording which line made which price. Migrations v16–v17 landed (`src/matching/`, `store_aliases`, `product_aliases`, `receipt_match_rejections`, `receipt_line_matches`); deployed 2026-11-17, idle until the first receipt arrives. Documented in `context/receipts/matching.md` |
| `desktop-entry-and-state-dir` | The service owns `/var/lib/maia` from first boot via `StateDirectory`. The `.desktop` entry was closed without building — Maia is launched without rofi, and the `2775` / group-`maia` permission target was dropped |
| `fitfat-server-health` | Per-request logging with latency and a masked auth hint, a log ring buffer and a `get_server_diagnostics` command. No diagnostics view and no Test Connection button |
| `fitfat-visualization` | The charts live in the Insights view and the pairing QR moved to the Sync view |
| `fix-fitfat-posts` | The 12 POSTs, the missing pulls and the v30 nuke shipped. `src/openapi.rs` keeps empty `paths()` / `components()`, so `/docs` serves a document with zero paths |
| `notes-markdown-mirror` | The mirror, the watcher, `GET /notes/audio/:id` and the mobile editor shipped. The scripted end-to-end walkthrough (T07) was dropped |
| `ui-polish-fixes` | The settings form editor and the zoom shipped; the datepicker and the task/goal form edits became moot when the read-only rewrite removed those forms |
| `fx-rates-and-firstrade-pdf` | **FX:** a server loop (`src/fx.rs`, every 6h, first tick at boot) backfills ECB daily rates from Frankfurter for every currency a dated row uses, back to its oldest row, one year per request, writing only changed rows; `POST /fx-rates/refresh`; the server is the only writer, so `POST /fx-rates` is gone and the phone list is read-only. **Firstrade PDFs** (`src/statements/firstrade_pdf.rs`): `pdftotext -layout`, the period and cash read in Rust, only the activity tables sent to the model, a reconciliation against the closing cash as a warning, the reading cached as `<hash>.extract.json`, rows lowered through the CSV path (net dividends, security-only moves informational). Not deployed; the model step is unverified because no `model_api` key is configured |
| `ui-unified-tabs` | The grouped sidebar and 24 of the 25 views shipped. There is no `maia-ui/src/views/experiments.ts`; the phone dropped the feature in schema v37, so the tab is gone on purpose |
| `waybar-and-notifications` | `GET /agenda` + the Waybar module doc + the background `notify-send` due-task notifier + the systemd D-Bus session wiring. The D-Bus fix is not verified end-to-end against the live `maia.service` |

## Dropped plans
These files were removed because their subject is gone, or a newer plan replaced
them. The intent is recorded here so it is not lost.

| Plan | Why it was dropped |
|---|---|
| `system-waybar` | Targeted the socket-based daemon architecture, which the HTTP `maia` service replaced. Replaced by `waybar-and-notifications` |
| `finance-budget-simulation` | Closed 2026-11-17 without being built: budget tracking and auto-categorization were dropped, and the plan's `maia budget` CLI/socket host no longer exists. The scenario half shipped as `financial-scenario-simulations`. Re-open only if budget tracking is wanted again — it needs re-specing against HTTP |
| `sqlite-schema` | Drafted a schema that contradicted the implementation (integer autoincrement keys instead of UUID v7, a `migrations` table instead of `PRAGMA user_version`). The schema is `V1_SCHEMA` + `MIGRATIONS` in `src/db.rs` |
| `kb-chrome-extension` | Proposed `chrome_bookmarks` / `chrome_reading_list` / `chrome_pages` tables behind the never-built socket daemon. The extension now posts straight to the `/urls` resource (`urls.content_path`, `GET /urls`, `GET /urls/:id/content`). Its residual auto-tagging and dedup ideas were never picked up |
| `statement-review-ui-fixes` | Its file-grouping diagnosis was answered by `statement-intake-and-account-merge` T01, and its T03/T04 shipped there |
| `personal-meetings` | Superseded by `meeting-recording`, which covers the same ground against the current architecture (server-side capture, transcription, a desktop screen) |
| `calendar-google` | Built entirely on the `maia calendar` CLI over the socket daemon, which no longer exists. Calendar sync is now specced as `outlook-calendar-sync` (Microsoft Entra OAuth over HTTP); Google Calendar as a second provider would need a fresh plan |
| `todo-txt-import` | Built on the daemon watch loop and the `completed_at` completion signal. Neither exists: the server now watches directories through the notes-mirror and statement watchers, and tasks use `done` / `task_status`. A todo.txt importer would need re-specing against those |
| `kb-search` | Proposed FTS5, vector embeddings and a knowledge graph over CLI commands and tables that were never built. Replaced by `knowledge-base-similarity-search`, which takes the same goal with a hand-rolled implementation |

## Decisions
| Decision | Date | Summary |
|---|---|---|
| The phone only reads finance | 2026-11-14 | The phone pulls accounts and transactions and never pushes either. Receipts go the other way: the phone only sends them, makes no draft from one, and drops its copy once the server has parsed it |
| A trade books its cash leg | 2026-11-12 | A trade is a movement, not income or expense: one transfer between the broker's cash and securities leaves plus a server-only `trades` row. Positions, average cost and realised gain are derived from that |

## External References
- `ROADMAP.md` — High-level feature roadmap
- `STORYLINE.md` — Historical development notes
- `README.md` — Usage guide
- `../mobile/context/sync/sync-contract.md` — the FitFat offline-first sync contract
