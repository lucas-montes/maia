# Plan: Wire Chrome Extension to Daemon

**Status:** Proposed
**Depends on:** system-daemon-wiring (for socket handler)

## Summary
The Chrome extension and native messaging host already work end-to-end (browser events → `chrome/` binary → files). This plan completes the pipeline by wiring the daemon side: when Chrome sends data (bookmarks, reading list items, page content), the daemon stores it in SQLite and makes it queryable via CLI.

## Tasks

### T01 — Chrome data database tables
- Create `chrome_bookmarks` table: id, title, url, folder, added_at
- Create `chrome_reading_list` table: id, title, url, added_at, read (bool)
- Create `chrome_pages` table: id, url, title, content_path (file path to saved HTML/text), saved_at
- Derive `Crud` for models where useful
- Add migrations in `initialize_schema()`

### T02 — Implement `handle_message(Chrome)` in daemon socket
- The `ClientMessage::Chrome` variant already exists in the protocol
- Implement the handler: parse the `BrowserAction` payload (ProcessUrl, ProcessUrlAndContent)
- Save bookmarks/reading list items to their respective tables
- Save page content file path + metadata to `chrome_pages` table
- Return acknowledgement

### T03 — Knowledge CLI for Chrome data
- CLI: `kb bookmarks list` — show saved bookmarks
- CLI: `kb reading-list` — show reading list items
- CLI: `kb pages list` — show saved page content
- CLI: `kb search <query>` — basic text search across titles and URLs (SQL LIKE)

### T04 — Bridge native messaging host to daemon
- The `chrome/` binary currently saves to `db.json` and content files
- Option A: Have `chrome/` also send data to the daemon socket (requires socket client in chrome crate)
- Option B: Have the daemon watch the `db.json` file and import on change
- Option C: Replace `chrome/` with a direct daemon socket connection
- Recommend Option A: Add socket client to `chrome/`, send data both to files (backward compat) AND daemon

### T05 — Auto-tagging and categorization
- Configurable rules: `{ "domain": "github.com", "tag": "dev" }`
- When a bookmark/page is saved, auto-apply tags
- Tags stored in a `chrome_tags` join table

### T06 — Deduplication
- Before inserting a bookmark/page, check if URL already exists
- If exists: update title, add new tag, bump timestamp
- Log duplicate detection

### T07 — Browser action feedback
- The native messaging protocol supports returning responses
- After successfully saving to daemon, return a success message
- The Chrome extension can then show a notification in-browser: "Saved to Maia!"

### T08 — Validation & cleanup
- Test full pipeline: Chrome extension → native host → chrome binary → daemon socket → SQLite
- Test CLI queries return correct data
- Test deduplication (same URL twice)
- Test auto-tagging rules
- Verify `db.json` still works as fallback

## Done checks
- Bookmark from Chrome appears in `kb bookmarks list` within seconds
- Reading list items are queryable
- Page content is saved and referenced in database
- Deduplication prevents duplicates
- Auto-tagging works by domain rules
