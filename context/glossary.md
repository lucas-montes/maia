# Glossary

| Term | Definition |
|---|---|
| **Crud** | Proc-macro in `maia-macros` that derives `create`, `find`, `update`, `delete`, `list` for SQLite models |
| **StructuredOutput** | Proc-macro in `ai-derive` that generates Gemini API schema for structured JSON extraction |
| **MET** | Metabolic Equivalent of Task — measures energy cost of physical activities |
| **WatchedDir** | A directory the daemon monitors via inotify for new files (receipt images, bank statements, etc.) |
| **Receipt** | AI-extracted structured data from a receipt image (store, date, total, products) |
| **BMR/TDEE** | Basal Metabolic Rate / Total Daily Energy Expenditure — calorie calculators |
| **Knowledge Base** | System for storing notes, bookmarks, and links with similarity search (not yet implemented) |
| **Native Messaging Host** | Chrome extension companion binary that receives browser events and saves them locally |
| **Saved URL** | A link captured from the Chrome tool or added manually, stored as a DB-only record with title, URL, source, tags, and is_new flag. No snapshot files or note/goal linking — opened externally via `tauri-plugin-opener`. |
| **Sync Server** | Standalone crate `sync-server` (Axum+rusqlite+tokio) implementing FitFat OpenAPI; embedded in Tauri on deterministic `0.0.0.0:3030` for LAN sync |
| **SyncAck** | Push response `{server_time}` (OpenAPI `SyncAck`); client ignores body, `204` also accepted |
| **server_time** | Epoch-ms cursor returned by every Pull (`server_time`); persisted as next `since` per resource |
| **since cursor** | Epoch-ms query param `?since=<ms>`; server returns only rows where `updated_at > since` and `deleted_at > since` for `deleted[]` |
| **Bearer API key** | `Authorization: Bearer <apiKey>` on every sync request; single key from `maia.json` `sync_api_key` (default `fitfat-sync-key`) |
| **FitFat Sync** | Offline-first delta sync per `~/Projects/fitfat/docs/api/openapi.yaml` and `context/sync/sync-contract.md` §11 MVP (Pull `since`, Push camelCase Drift `toJson`) |
| **FitFat Visualization** | `maia-ui` `ViewId="fitfat"` reading `sync-server` HTTP (`GET /workouts|/meals|/body-metrics`) with per-chart `since` filters, Chart.js, no duplicate Tauri DB logic |
| **Per-Chart Filter** | Each FitFat chart owns its `since`/`date` state (`data-fitfat-filter`), `fetch(...?since=<ms>)` re-fetches only that chart |
| **QR Auth Payload** | Settings QR `{"url":"http://<lan-ip>:3030","apiKey":"<Bearer>","version":1}` via `qrcode` canvas + copy, not logged |
