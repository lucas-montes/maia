# Plan: Remote Storage

**Status:** Proposed
**Depends on:** system-daemon-wiring (for daemon backbone)

## Summary
Add configuration and background logic to sync the SQLite database and watched files to remote storage for backup and cross-device access.

## Tasks

### T01 — Remote storage configuration
- Add `[remote_storage]` section to `maia.json` config
- Fields: `type` ("s3" | "nextcloud" | "local"), `endpoint`, `bucket`/`path`, `access_key` (optional, env fallback), `sync_interval_minutes`
- Load and validate in `Config` struct

### T02 — Sync abstraction layer
- Define `StorageBackend` trait: `fn upload(path, data)`, `fn download(path) -> data`, `fn list(prefix) -> Vec<path>`
- Implement `S3Backend` (using `aws-sdk-s3` or `rust-s3` crate)
- Implement `LocalBackend` (simple rsync/cp to another local path)
- `NextcloudBackend` can be a future task (WebDAV-based)

### T03 — Database sync background task
- Add a periodic task in daemon main loop (tokio interval)
- On each tick: check if DB file has changed (mtime), upload if so
- On startup: optionally download latest DB from remote (config: `sync_on_start: true`)
- Use temp files to avoid partial uploads

### T04 — File sync for watched directories
- When a file is processed (receipt, bank statement), also upload the original to remote storage
- Same directory structure mirrored remotely
- Track sync state in a local `sync_manifest` table (file_hash, remote_path, last_synced_at)

### T05 — Sync status CLI
- CLI: `sync status` — show last sync time, pending files, remote storage type
- CLI: `sync now` — force immediate sync
- CLI: `sync log` — show recent sync activity

### T06 — Conflict resolution
- Simple strategy: last-writer-wins based on mtime
- Keep local backup of overwritten files for N days (configurable)
- Log conflicts for manual review

### T07 — Validation & cleanup
- Test S3 upload/download with a real or mock S3-compatible store
- Test sync on file creation
- Test sync on timer interval
- Test restart with remote DB download
- Verify no data loss on partial upload (temp file + rename pattern)

## Done checks
- Config loads remote storage settings
- Files sync to remote on a configurable interval
- Database is backed up to remote
- `sync status` shows current state
- Daemon can restore database from remote on restart
