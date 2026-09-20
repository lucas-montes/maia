# Fix FitFat POST Calls — Full Verification & Migration

## Change summary
Align `maia` `sync-server` with FitFat mobile `lib/src/sync/data_push_service.dart` + `docs/api/openapi.yaml` (12 POSTs). Fix `uuid v7`, unify `maia.db` to FitFat Drift v30 (38 tables, TEXT PK, epoch ms, JSON arrays, composite PKs), fix camelCase `TasksPush/NotesPush/...` + `deleted[]` → `deleted_at`, fix `receipt-pictures` `picturePath→localPath` multipart bug + add `note_audio` handling, add missing `GET` pulls for 6 push-only resources, generate OpenAPI via `utoipa`/`utoipa-axum` at `/api-docs/openapi.json` + `/docs`, delete legacy INTEGER `notes/tasks/goals/receipts/urls` schema, and add `gst-plugins-bad` for WebKit WebVTT.

## Success criteria
- [ ] `maia.db` is single v30 schema (no `INTEGER AUTOINCREMENT` legacy tables); `sqlite3 maia.db ".schema"` matches `fitfat/lib/src/database/tables.dart` v30; phone `POST /tasks` appears in desktop `list_tasks` (same TEXT PK table).
- [ ] `uuid = {v4,v7,serde}` + `Uuid::now_v7()` everywhere (`create_store`, `create_fx_rate`, `seed`, `sync-server`); `cargo check` green.
- [ ] All 12 FitFat POSTs verified: `POST /ingredients` (flat `IngredientContribution`), `/workouts` (`workouts+workoutExercises+exerciseSets`), `/templates` (`workoutTemplates+...`), `/notes` (`notes+noteTags+noteAudio` + multipart per-row), `/tasks` (`tasks+taskTags`), `/goals` (`goals+goalProgressEntries+goalTags`), `/meals` (`meals+mealIngredients`), `/transactions` (`transactions`), `/budget-accounts` (`accounts`), `/receipt-pictures` (multipart `receipt+picture` else `{receipts:[]}`), `/backup` (raw bytes), plus `note-audio` via `/notes` multipart — all accept FitFat `toJson()` camelCase and return `200 {server_time}` or `204`.
- [ ] `deleted[]` handled on every POST (set `deleted_at=server_time_ms()` for ids in `deleted`), and every `GET ?since` returns `deleted: string[]` via `deleted_at > since`.
- [ ] Missing pulls added: `GET /templates|/notes|/tasks|/goals|/transactions|/budget-accounts|/receipt-pictures|/body-metrics|/experiments|/tags` with `?since` + `deleted`.
- [ ] `utoipa` OpenAPI generated for all endpoints, served at `/api-docs/openapi.json` + Swagger UI at `/docs`, `curl /docs` 200, `curl /api-docs/openapi.json | jq .paths | length` >=12, used by FitFat `SettingsState` endpoint overrides.
- [ ] `cargo test -p sync-server` 17+ pass, `cargo check --workspace` green, `pnpm build` green, `curl -H "Authorization: Bearer $KEY" http://127.0.0.1:3030/tasks?since=0` returns phone tasks, `curl -H "Authorization: Bearer $KEY" -X POST http://127.0.0.1:3030/receipt-pictures -F receipt='{"id":"r1",...}' -F picture=@a.jpg` 200.
- [ ] WebKit warning gone: `flake.nix` includes `gst_all_1.gst-plugins-bad`, `nix develop` has `GST_PLUGIN_PATH`.

## Constraints and non-goals
**Constraints:**
- Stack: `sync-server` Axum 0.7 + `rusqlite 0.37 bundled` + `tokio` + `serde` + `chrono` + `uuid v7` + `utoipa 4`/`utoipa-axum 0.1`/`utoipa-swagger-ui`; `maia.db` only (delete `fitfat_sync.db` legacy), `0.0.0.0:3030` Bearer, `since`/`server_time`/`deleted[]` LWW per-row, `exercise_media/` + `receipt_pictures/` dirs.
- DB: break old schemas, no backward compat, delete `maia-ui/src-tauri` legacy `INTEGER` `CREATE TABLE` for `notes/tasks/goals/receipts/urls`, delete `shared/src/todo.rs` legacy, `sync-server/src/db.rs` is source of truth mirroring `fitfat` v30.
- Wire: PUSH `camelCase` (`caloriesPer100g`, `isArchived`, `createdAt`, `workoutExercises`, `taskStatus`, `carryOver`, `amountBase`, `openingBalance`, `localPath`), PULL `snake_case` (`created_at`, `updated_at`, `server_time`, `hasImage`), `Content-Type: application/json` for JSON, `multipart/form-data` for `receipt-pictures`/`note-audio`, `application/octet-stream` for `/backup`.
- Auth: `Authorization: Bearer <apiKey>` from `maia.json` (`sync_api_key` or `sync.api_key` else `fitfat-sync-key`), `GET /health` no auth, CORS `Any` + `AUTHORIZATION/CONTENT_TYPE`.

**Non-goals:**
- No mobile UI changes, no FTS5/vector, no S3, no field-level merge (LWW per row), no `PUT/DELETE` via HTTP (soft `deleted_at` only), no editing `body_metrics`/`experiments` on desktop (read-only), no `fx_rates` edit (add-only).

## Assumptions
- `fitfat/docs/api/openapi.yaml` + `lib/src/sync/data_push_service.dart` are source of truth for POST shapes; `lib/src/database/tables.dart` v30 is DB source of truth.
- Phone sends `POST /tasks` as `{tasks:[{id,date,title,done,taskStatus,carryOver,sortOrder,dueDate,...}], taskTags:[{tagId,taskId}], deleted:[]}` (if `deleted` missing, treat as `[]`).
- `maia.db` currently has 3799 `exercises` + `exercise_media/` seeded; will be wiped and recreated from v30 schema (no migration).

## Task stack (T01..T10)

- [x] T01: UUID v7 + DB nuke to v30 (status:done)
  - Task ID: T01
  - Goal: Replace `uuid v4` with `v7` and recreate `maia.db` as single FitFat v30 schema (38 tables, TEXT PK, epoch ms, JSON arrays, composite PKs), deleting legacy INTEGER tables.
  - Boundaries (in/out of scope): In - `maia-ui/src-tauri/Cargo.toml` + `sync-server/Cargo.toml` `uuid {v4,v7,serde}`, `maia-ui/src-tauri/src/lib.rs` `Uuid::now_v7()` for `create_store`/`create_fx_rate`, `sync-server/src/db.rs` `create_schema` exact v30 (drop `ALTER TABLE ADD COLUMN` legacy loop, add `urls` as TEXT PK, ensure `note_audio`, `experiment_checkins`, 13 junctions), `maia.db` file delete + `init_db` recreate, `seed.rs` keep. Out - POST handlers (T02+), utoipa (T06), WebKit (T07).
  - Done when: `cargo check --workspace` green, `sqlite3 maia.db "SELECT sql FROM sqlite_master WHERE type='table' AND name='tasks'"` shows `id TEXT PRIMARY KEY, date INTEGER, ... taskStatus`, no `INTEGER AUTOINCREMENT` tables, `cargo test -p sync-server` still 17 pass.
  - Verification notes (commands or checks): `cargo check -p maia-ui`; `cargo check -p sync-server`; `sqlite3 maia.db ".tables" | wc -l` >=38; `rg "now_v7" maia-ui/src-tauri/src/lib.rs`; `rm maia.db && cargo run -p sync-server --bin seed_now` (if exists) or `cargo test`.

- [x] T02: Fix POST JSON shapes + deleted[] for all 12 (status:done)
  - Task ID: T02
  - Goal: Make every `POST` accept FitFat `toJson()` camelCase + optional `deleted: string[]` and set `deleted_at`.
  - Boundaries (in/out of scope): In - `sync-server/src/handlers/push.rs` DTOs `#[serde(rename_all="camelCase", default)]` + `deleted: Vec<String> = []`, `receipts.rs` `picturePath→localPath` fix, `backup.rs` `application/octet-stream` check, `db.rs` `deleted_at` handling. Out - missing GET pulls (T03), utoipa (T06).
  - Done when: `POST /ingredients` flat object, `/workouts` 3 arrays, `/templates` 3 arrays, `/notes` 3 arrays (`notes+noteTags+noteAudio`), `/tasks` 2 arrays, `/goals` 3 arrays, `/meals` 2 arrays, `/transactions` 1 array, `/budget-accounts` 1 array, `/receipt-pictures` dual, `/backup` raw bytes all accept FitFat payloads and `deleted[]` sets `deleted_at`.
  - Verification notes (commands or checks): `curl -H "Authorization: Bearer test-key" -H "Content-Type: application/json" -d @fitfat_sample_tasks.json http://127.0.0.1:3030/tasks -w "%{http_code}"` 200; `sqlite3 maia.db "SELECT deleted_at FROM tasks WHERE id='...'"` not null after `deleted` push; `cargo test -p sync-server push_*`.

- [x] T03: Add missing GET pulls (status:done)
  - Task ID: T03
  - Goal: Add `GET /templates|/notes|/tasks|/goals|/transactions|/budget-accounts|/receipt-pictures|/body-metrics|/experiments|/tags` with `?since` + `deleted`.
  - Boundaries (in/out of scope): In - `sync-server/src/handlers/pull.rs` + `read.rs` new `pull_*` fns, `server.rs` router `GET` routes, `db.rs` `updated_at`/`deleted_at` indexes. Out - POST fixes (T02), utoipa (T06).
  - Done when: `GET /tasks?since=0` returns `{server_time, items:Task[], deleted:[]}` with `taskTags` nested or separate, similarly for others; `since` filters `updated_at > since`, `deleted` from `deleted_at > since`.
  - Verification notes (commands or checks): `curl -H "Authorization: Bearer test-key" "http://127.0.0.1:3030/tasks?since=0" | jq .items[0].taskStatus`; `curl .../templates?since=0 | jq .items`; `cargo test -p sync-server pull_*`.

- [x] T04: Fix receipt-pictures + note-audio multipart (status:done)
  - Task ID: T04
  - Goal: Fix `localPath` bug and handle `note_audio` multipart on `/notes`.
  - Boundaries (in/out of scope): In - `sync-server/src/handlers/receipts.rs` `parse_multipart` `name="receipt"`+`name="picture"`, `handlers/push.rs` `push_notes` handle `noteAudio` array + multipart `noteAudio` field, `db.rs` `note_audio` table, `fitfat/lib/src/sync/data_push_service.dart` `picturePath` → `localPath` doc. Out - other POSTs (T02), pulls (T03).
  - Done when: `POST /receipt-pictures` multipart with `receipt` JSON + `picture` bytes writes `receipt_pictures/<id>.jpg` and `remote_path`; `POST /notes` multipart with `noteAudio` JSON + `audio` bytes writes `note_audio` row; JSON fallback `{receipts:[]}` and `{noteAudio:[]}` still works.
  - Verification notes (commands or checks): `curl -H "Authorization: Bearer test-key" -F receipt='{"id":"r1","localPath":"/tmp/a.jpg",...}' -F picture=@a.jpg http://127.0.0.1:3030/receipt-pictures -w "%{http_code}"` 200; `sqlite3 maia.db "SELECT count(*) FROM note_audio"`.

- [x] T05: Unify maia-ui Tauri commands to v30 (status:done)
  - Task ID: T05
  - Goal: Make `list_*` Tauri commands read new TEXT PK v30 tables (epoch ms) so phone pushes appear on desktop.
  - Boundaries (in/out of scope): In - `maia-ui/src-tauri/src/lib.rs` `list_exercises`/`list_ingredients`/`list_stores`/`list_meals`/`list_body_metrics`/`list_experiments`/`list_tags`/`list_accounts`/`list_transactions`/`list_fx_rates` + `list_tasks`/`list_notes`/`list_goals`/`list_urls` rewritten to TEXT PK schema, `AppState` `Mutex<Connection>` stays. Out - DB nuke (T01), POST handlers (T02).
  - Done when: After phone `POST /tasks`, `invoke("list_tasks")` returns same `id` with `taskStatus`/`carryOver`; `list_notes` returns `body` not `path`; no `INTEGER` queries remain.
  - Verification notes (commands or checks): `cargo check -p maia-ui`; `pnpm build`; manual `POST /tasks` via `curl` then `sqlite3 maia.db "SELECT id FROM tasks"` and Tauri `list_tasks` shows it.

- [ ] T06: Utoipa OpenAPI for all endpoints (status:todo)
  - Task ID: T06
  - Goal: Generate OpenAPI via `utoipa` for all 12 POST + 10+ GET + `/health` + `/backup` + `/media`, serve Swagger at `/docs`.
  - Boundaries (in/out of scope): In - `sync-server/Cargo.toml` `utoipa 4` + `utoipa-axum 0.1` + `utoipa-swagger-ui 7`, `#[derive(ToSchema)]` on all DTOs, `#[utoipa::path(post, path="/tasks", ...)]` on handlers, `OpenApi` struct with `bearerAuth`, `server.rs` mount `/api-docs/openapi.json` + `/docs`, `fitfat` `SettingsState` endpoint overrides doc. Out - DB (T01), POST logic (T02).
  - Done when: `curl http://127.0.0.1:3030/api-docs/openapi.json | jq .paths | keys | length` >=20, `curl http://127.0.0.1:3030/docs` 200, `cargo check -p sync-server` green, `fitfat` can import `openapi.json`.
  - Verification notes (commands or checks): `cargo check -p sync-server`; `curl -s http://127.0.0.1:3030/api-docs/openapi.json | jq .info.title`; `curl -I http://127.0.0.1:3030/docs`.

- [x] T07: WebKit WebVTT gst-plugins-bad (status:done)
  - Task ID: T07
  - Goal: Fix `WebKit wasn't able to find a WebVTT encoder` warning.
  - Boundaries (in/out of scope): In - `flake.nix`/`nix/module.nix` add `gst_all_1.gst-plugins-bad` + `gst-plugins-base` to `buildInputs`/`TAURI_WEBKIT` env, `GST_PLUGIN_PATH` export. Out - DB/sync (T01-T06).
  - Done when: `nix develop -c gst-inspect-1.0 webvttenc` found, `pnpm tauri dev` no WebVTT warning, subtitles degraded gone.
  - Verification notes (commands or checks): `nix develop -c bash -c "gst-inspect-1.0 webvttenc | head"`; `rg "gst-plugins-bad" flake.nix`.

- [x] T08: Validation and cleanup (status:done)
  - Task ID: T08
  - Goal: Full test/lint pass, remove scaffolding, sync `context/`.
  - Boundaries (in/out of scope): In - `cargo test --workspace`, `cargo check --workspace`, `pnpm build`, `cargo fmt --check`, remove `context/tmp/`, update `context/overview.md` (single v30 `maia.db` + utoipa), `context/context-map.md`, `context/glossary.md` (UUID v7, `deleted[]`, `since` cursor). Out - new features.
  - Done when: `cargo test --workspace` green, `cargo check --workspace` green, `pnpm build` green, `curl /api-docs/openapi.json` valid, `sqlite3 maia.db "SELECT count(*) FROM tasks"` after phone sync >0.
  - Verification notes (commands or checks): `cargo test --workspace`; `cargo check --workspace`; `pnpm build`; `ls context/plans/fix-fitfat-posts.md`.

## Open questions
- Phone sample `POST /tasks` JSON to lock `deleted` field name (`deleted` vs `deletedIds`)? Assume `deleted: string[]` per `GET` `deleted`.
- Should `POST /receipt-pictures` multipart field stay `picture` or rename to `image` to match `localPath`? Keep `picture` per `openapi.yaml`.
- `note_audio` multipart field name `audio` vs `picture`? Keep `audio` per `data_push_service.dart:103`.
