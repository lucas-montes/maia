# Maia — Project Overview

## What is Maia?
A personal productivity suite with AI-powered features for managing todos, notes, expenses, calorie tracking, exercise, and knowledge base. Runs as a background daemon with CLI tools, Chrome extension, and file watchers.

## Architecture
- **Daemon** (root binary): Background service with file watchers (inotify) and Unix socket listener for CLI communication
- **CLI tools**: Per-domain CLIs in `todo/`, `knowledge/` crates that communicate with daemon via socket
- **Chrome extension + native messaging host**: Browser integration for bookmarks, reading list, context menus
- **Shared library**: Database models, CRUD macros, AI schema definitions
- **AI integration**: Gemini API for structured data extraction (receipts from images)

## Key Constraints
- Workspace: Rust edition 2024
- Database: SQLite via rusqlite
- Storage: Local-first (files + SQLite), remote storage planned
- Daemon uses Unix domain sockets for IPC (`/tmp/maia.sock`)

## Current State
The project has significant scaffolding but much is not yet wired together (April 2026). See `ROADMAP.md` for the full feature wishlist. See `context/plans/` for active implementation plans.
