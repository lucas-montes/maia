# Maia — Project Overview

## What is Maia?
A personal productivity suite with AI-powered features for managing todos, notes, expenses, calorie tracking, exercise, and knowledge base. Runs as a background daemon with CLI tools, Chrome extension, and file watchers.

## Architecture
- **Sync Server** (`sync-server` crate): Standalone Axum + rusqlite (bundled) + tokio HTTP server implementing `~/Projects/fitfat/docs/api/openapi.yaml` (FitFat offline-first sync). Embedded in `maia-ui/src-tauri` via `tauri::async_runtime::spawn` on deterministic `0.0.0.0:3030` LAN port, Bearer `Authorization` auth, `server_time` epoch ms, `since` cursors, `deleted[]` tombstones, WAL+FK, UUID v7 TEXT PK. Single `maia.db` v30 (38 tables, no legacy INTEGER tables). Exposes `get_sync_port`/`get_sync_url` Tauri commands. See `context/plans/fitfat-sync-server.md`.
- **Tauri Desktop** (`maia-ui/src-tauri`): Desktop shell with SQLite via rusqlite (single `maia.db` v30, `sync_server::db::create_schema` only, no legacy `INTEGER AUTOINCREMENT` tables), UUID v7 (`Uuid::now_v7()`), and embedded sync-server lifecycle (replaces legacy daemon for sync).
- **Daemon** (root binary, deprecated): Background service with file watchers (inotify) and Unix socket listener — superseded by `sync-server` for FitFat sync; retained only for file-watcher legacy, not used by Tauri.
- **CLI tools**: Per-domain CLIs in `todo/`, `knowledge/` crates that communicate via socket (legacy)
- **Chrome extension + native messaging host**: Browser integration for bookmarks, reading list, context menus
- **Shared library**: Database models, CRUD macros, AI schema definitions
- **AI integration**: Gemini API for structured data extraction (receipts from images)

## Key Constraints
- Workspace: Rust edition 2024
- Database: SQLite via rusqlite
- Storage: Local-first (files + SQLite), remote storage planned
- Daemon uses Unix domain sockets for IPC (`/tmp/maia.sock`)

## Current State
- **FitFat Sync Server** (`fitfat-sync-server` plan) completed 2026-09-08: full OpenAPI implemented (Pull: `GET /exercises`, `/ingredients`, `/fx-rates`; Push: `POST /ingredients`, `/workouts`, `/templates`, `/notes`, `/tasks`, `/goals`, `/meals`, `/transactions`, `/budget-accounts`, `/receipt-pictures`; Backup: `POST /backup`, `GET /backup/latest`; Media: `GET /exercises/:id` jpg/mp4 via deterministic `0.0.0.0:3030` LAN). 16/16 `sync-server` tests pass, `cargo check --workspace` clean.
- **FitFat Visualization** (`fitfat-visualization` plan) completed 2026-09-08: HTTP-leveraged read (`GET /workouts`, `/meals`, `/body-metrics` + existing Pull) via `fetch` with Bearer (no duplicate Tauri DB logic), per-chart `since` filters, 4 Chart.js canvases (workouts/week bar, volume line, weight line, meals bar), FitFat tab `📊` in sidebar, Settings QR with `{"url","apiKey"}` auth token + copy. `cargo test --workspace` + `pnpm build` green.
- **Fix FitFat POSTs T01** (2026-09-08): `maia.db` nuked to single v30 (38 tables, TEXT PK UUID v7, `urls` TEXT PK, no legacy `INTEGER AUTOINCREMENT`), `uuid v7` (`Uuid::now_v7()`) in `maia-ui` + `sync-server`, `exercise_media` 3799 seeded, `cargo test -p sync-server` 17/17 pass.
- **Fix FitFat POSTs T02-T04** (2026-09-08): All 12 POSTs now accept FitFat `toJson()` camelCase + `deleted[]` → `deleted_at`, `noteAudio` multipart via `POST /notes`, `receipt-pictures` `localPath` fix, `cargo test -p sync-server` 17/17 pass.
- **Fix FitFat POSTs T03** (2026-09-08): Added missing `GET /templates|/notes|/tasks|/goals|/transactions|/budget-accounts|/receipt-pictures|/experiments|/tags` with `?since` + `deleted`, `server.rs` router updated, `pull.rs` 8 new handlers.
- **Unified UI Tabs** (2026-09-08): Grouped collapsible sidebar (Training/Nutrition/Health/Planning/Finance) + 13 tabs, virtual scroll, lazy media, `maia-ui` Tauri commands now read v30 TEXT PK (`list_notes`/`list_tasks`/`list_goals`/`list_urls`/`list_receipts` generic `Vec<Value>`), `get_counts` filters `deleted_at`.
- **Utoipa + WebKit** (2026-09-08): `sync-server` `utoipa 4` + `utoipa-swagger-ui 7` at `/docs` + `/api-docs/openapi.json`, `nix/devshells.nix` `gst-plugins-bad`/`gst-plugins-base` + `GST_PLUGIN_PATH` for WebVTT.
- The project has significant scaffolding but much is not yet wired together (April 2026). See `ROADMAP.md` for the full feature wishlist. See `context/plans/` for active implementation plans.
