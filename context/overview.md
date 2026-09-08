# Maia — Project Overview

## What is Maia?
A personal productivity suite with AI-powered features for managing todos, notes, expenses, calorie tracking, exercise, and knowledge base. Runs as a background daemon with CLI tools, Chrome extension, and file watchers.

## Architecture
- **Sync Server** (`sync-server` crate): Standalone Axum + rusqlite (bundled) + tokio HTTP server implementing `~/Projects/fitfat/docs/api/openapi.yaml` (FitFat offline-first sync). Embedded in `maia-ui/src-tauri` via `tauri::async_runtime::spawn` on deterministic `0.0.0.0:3030` LAN port, Bearer `Authorization` auth, `server_time` epoch ms, `since` cursors, `deleted[]` tombstones, WAL+FK. Exposes `get_sync_port`/`get_sync_url` Tauri commands. See `context/plans/fitfat-sync-server.md`.
- **Tauri Desktop** (`maia-ui/src-tauri`): Desktop shell with file-backed notes/receipts, SQLite via rusqlite, and embedded sync-server lifecycle (replaces legacy daemon for sync).
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
- The project has significant scaffolding but much is not yet wired together (April 2026). See `ROADMAP.md` for the full feature wishlist. See `context/plans/` for active implementation plans.
