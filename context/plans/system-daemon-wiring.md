# Plan: Wire the Daemon

**Status:** Proposed
**Depends on:** none (foundational)

## Summary
This is the foundation plan. The daemon main loop, socket handler, and database initialization are all commented out or stubbed. Everything else depends on this working.

## Tasks

### T01 — Database schema initialization
- Add `initialize_schema()` to `Database` in `shared/src/database.rs`
- Run `CREATE TABLE IF NOT EXISTS` for ALL existing models (receipts, purchases, aliment, meals, exercises, seances, body_weights, transactions, investments)
- Use the `table_name()` from `Crud` derive or hardcoded list
- Make it idempotent (IF NOT EXISTS)

### T02 — Uncomment and fix daemon main loop
- In `src/main.rs`, uncomment the `tokio::join!(monitor_dirs(...), listen_socket(...))` block
- Ensure both tasks are properly spawned with correct state sharing
- Add error handling so one task failing doesn't kill the other
- Add graceful shutdown on SIGINT/SIGTERM

### T03 — Fix socket listener (`src/socket.rs`)
- Ensure the Unix socket at `/tmp/maia.sock` is properly created (clean up stale socket file on startup)
- Implement the length-prefixed JSON read/write loop
- Dispatch incoming messages to typed handlers

### T04 — Implement `handle_message()` dispatcher
- Replace `todo!()` in the message handler with a dispatch table
- Match on `ClientMessage` variants: Goal, Task, Knowledge, Chrome
- For now, each variant calls a placeholder handler that acknowledges receipt
- Later plans will flesh out each handler

### T05 — Fix file watcher (`src/watcher.rs`)
- Ensure `monitor_dirs()` loop runs correctly (fix any bugs in the commented-out code)
- Test that dropping a receipt image in a watched dir triggers `handle_receipt_file`
- `handle_bank_file` and `handle_investment_file` remain stubs but should log instead of panic

### T06 — Config-driven watched directories
- Ensure `Config::watched_dirs()` correctly returns configured paths
- If no paths are configured, skip watcher initialization (don't crash)
- Log which dirs are being watched at startup

### T07 — Logging and observability
- Add structured log events at key points: daemon start/stop, file detected, message received, handler called
- Add a health check endpoint via socket (`ping` → `pong` with uptime, db size, watched dirs)
- Ensure tracing output is readable in both JSON (production) and human (dev) modes

### T08 — Validation & cleanup
- Start daemon, verify it begins watching configured dirs
- Send a test message via socket, verify acknowledgement
- Drop a file in a watched dir, verify event is logged
- Stop daemon with SIGINT, verify clean shutdown
- Remove all `todo!()` and commented-out dead code
- Verify `cargo build` with no warnings

## Done checks
- Daemon starts, watches dirs, listens on socket
- `initialize_schema()` creates all tables (idempotent)
- `handle_message()` dispatches without panic
- File watcher triggers on new files
- Clean shutdown works
- `cargo build` clean, no warnings
