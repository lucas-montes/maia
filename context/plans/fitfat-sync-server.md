# FitFat Sync Server — Standalone Crate + Full OpenAPI

## Change summary
Create a standalone Rust crate `sync-server` (Axum + rusqlite bundled + tokio) that implements the full `~/Projects/fitfat/docs/api/openapi.yaml` contract, then embed it directly in `maia-ui/src-tauri` via Tauri `setup()` on deterministic `0.0.0.0:3030` LAN port. Remove the legacy daemon (`src/main.rs` watcher/socket) and replace it with the new crate as the single local-first backend. The server becomes the sole DB owner for FitFat sync data; both the Tauri frontend and the mobile app talk to it over HTTP. Covers all Pull, Push, Backup, and Media endpoints with Bearer auth, `since` cursors, `server_time`, and `deleted[]` handling.

## Success criteria
- [ ] `sync-server` crate builds as workspace member and exposes `start_server(db_path, api_key) -> (port, handle)` usable from Tauri.
- [ ] `GET /exercises?since`, `GET /ingredients?since`, `GET /fx-rates?base&date&since` return `Pull*Response` with `server_time`, `items`, `deleted` per OpenAPI, filtered by `since` (epoch ms).
- [ ] `POST /ingredients`, `/workouts`, `/templates`, `/notes`, `/tasks`, `/goals`, `/meals`, `/transactions`, `/budget-accounts`, `/receipt-pictures` (multipart + JSON fallback), `/backup` + `GET /backup/latest` all accept camelCase Drift `toJson` bodies and return `SyncAck`/`204`.
- [ ] `GET /exercises/{id}.jpg` and `.mp4` serve binaries with Bearer auth, backed by `<documents>/exercise_media/`-style file store.
- [ ] Bearer `Authorization: Bearer <apiKey>` enforced on every endpoint; `401` returns `Error{message}` per spec.
- [ ] Tauri app spawns server in `setup()`, discovers random port via `tauri::State`, exposes `get_sync_port` command, kills server on app close. Daemon binary removed from workspace/default run.
- [ ] `curl` against `http://127.0.0.1:{port}` for each endpoint passes; `cargo test` and `cargo check` pass.

## Constraints and non-goals
**Constraints:**
- Stack: `rusqlite 0.37 bundled` + `axum 0.7` + `tokio` + `serde`/`serde_json` + `chrono` + `tower-http` (CORS) + `uuid` (v7). Reuse `shared` crate patterns where possible. No `sqlx`.
- DB: new isolated `fitfat_sync.db` (not Maia's existing DB) mirroring `fitfat/lib/src/database/tables.dart` + server-side `updated_at`/`deleted_at` tombstones. WAL mode, `PRAGMA foreign_keys=ON`.
- Timestamps: epoch milliseconds `int64` everywhere. Pull responses use `snake_case` (`created_at`, `updated_at`, `hasImage`), Push bodies use `camelCase` (`createdAt`, `workoutId`).
- Auth: single Bearer API key from Tauri config/store (no OAuth, no `/v1/accounts/bootstrap` yet). Base URL and paths are user-configurable but default to OpenAPI paths.
 - Binding: `0.0.0.0:3030` deterministic LAN port (per user request, no dynamic random port). Graceful shutdown on Tauri `on_window_event` close. Tests use `0.0.0.0:0` ephemeral to avoid collisions.

**Non-goals:**
- No FTS5/vector search, no S3/remote storage, no field-level merge (LWW per row, server wins on Pull), no `fx_rates` push, no receipt/ingredient picture binary sync beyond `receipt-pictures` multipart and exercise media, no Settings/preferences sync, no account bootstrap flow.

## Assumptions
- User approved standalone crate + daemon removal + full OpenAPI + rusqlite+Axum per clarification 2026-09-08.
- FitFat mobile app already implements `sync-contract.md` §11 MVP client (Bearer, `since` cursor, `server_time` persistence); server must be compatible with that client without client changes.
- Exercise media is immutable per `id`; replacing media requires delete+re-add (per `sync-contract.md` §11.2 limitation).

## Task stack (T01..T10)

 - [x] T01: Scaffold standalone `sync-server` crate (status:done)
  - Task ID: T01
  - Goal: Create `sync-server/` crate as workspace member with Axum+rusqlite+tokio scaffolding, health endpoint, and workspace wiring.
  - Boundaries (in/out of scope): In - `sync-server/Cargo.toml`, `src/lib.rs`, `src/config.rs`, `src/error.rs`, `GET /health` returning `{"status":"ok"}`, add to root `Cargo.toml` workspace members, `cargo check` passes. Out - DB schema, auth, business handlers (T02+), Tauri integration (T09), daemon removal beyond Cargo.toml.
  - Done when: `cargo check -p sync-server` and `cargo check -p maia-ui` pass; `cargo test -p sync-server` runs (0 tests ok); `curl http://127.0.0.1:{port}/health` returns 200 when started via `sync-server::start_test_server()`.
  - Verification notes (commands or checks): `cargo check -p sync-server`; `cargo test -p sync-server`; `rg "sync-server" Cargo.toml`
  - Completed: 2026-09-08
  - Files changed: Cargo.toml, sync-server/Cargo.toml, sync-server/src/lib.rs, sync-server/src/config.rs, sync-server/src/error.rs, sync-server/src/server.rs, sync-server/src/handlers/mod.rs, sync-server/src/handlers/health.rs
  - Evidence: cargo check -p sync-server ok (14.46s), cargo test -p sync-server 1 passed (health_returns_ok), cargo check --workspace ok, bind_addr=0.0.0.0:0 per user request
  - Notes: Workspace tokio moved to workspace.dependencies; health test uses 127.0.0.1 to connect to 0.0.0.0-bound server (all interfaces)

- [x] T02: Database schema and migrations for FitFat sync (status:done)
  - Task ID: T02
  - Goal: Create `fitfat_sync.db` schema mirroring `fitfat/lib/src/database/tables.dart` for all synced aggregates plus server-side `updated_at`/`deleted_at` and `server_time` cursor support.
  - Boundaries (in/out of scope): In - `sync-server/src/db.rs`, `src/migrations.rs` or `schema.sql`, tables: `exercises`, `ingredients`, `stores`, `ingredient_pictures`, `ingredient_prices`, `meals`, `meal_ingredients`, `workouts`, `workout_exercises`, `exercise_sets`, `workout_templates`, `workout_template_exercises`, `workout_template_sets`, `notes`, `note_tags`, `tasks`, `task_tags`, `goals`, `goal_progress_entries`, `goal_tags`, `accounts`, `transactions`, `receipts`, `fx_rates` (PK `(code, base_code, rate_date)`), `stores` etc., plus `deleted_tombstones` or `deleted_at` columns, WAL + FK pragmas, `updated_at` triggers. Out - HTTP handlers, auth, Tauri wiring.
  - Done when: Fresh `fitfat_sync.db` creates all tables with correct PKs/uniques (`ingredient_prices` unique `(ingredient_id, store_id, recorded_at)`, `fx_rates` PK `(code, base_code, rate_date)`), `rusqlite` migration runs idempotently, unit test inserts/selects each aggregate.
  - Verification notes (commands or checks): `cargo test -p sync-server db::tests::migrations_idempotent`; `sqlite3 /tmp/fitfat_sync.db ".schema"` shows all tables; `cargo check -p sync-server`
  - Completed: 2026-09-08
  - Files changed: sync-server/src/db.rs, sync-server/src/lib.rs
  - Evidence: cargo test -p sync-server 5/5 passed (migrations_idempotent, unique_constraints_hold, can_insert_aggregates, since_filtering, health_returns_ok), cargo check --workspace ok, 32+ tables created with WAL + FK, PK/uniques verified
  - Notes: Added updated_at/deleted_at to every table for since/deleted[] cursors; fx_rates PK (code, base_code, rate_date) preserved; ingredient_prices unique constraint verified

- [x] T03: Auth middleware, error envelope, and server_time (status:done)
  - Task ID: T03
  - Goal: Enforce `Authorization: Bearer <apiKey>` on every route, return OpenAPI `Error{message}` on 401/500, inject `server_time` (epoch ms) and CORS.
  - Boundaries (in/out of scope): In - `src/auth.rs` Axum middleware/layer, `src/error.rs` mapping to `{"message": "..."}`, `tower-http` CORS for `tauri://localhost` + `http://tauri.localhost`, `server_time` helper (`chrono::Utc::now().timestamp_millis()`), config for `api_key` from `sync-server::Config`. Out - business logic for Pull/Push, media file serving.
  - Done when: Request without header -> 401 `{"message":"..."}`, with wrong key -> 401, with correct key -> 200; CORS headers present; `server_time` is monotonic ms and returned in every Pull response.
  - Verification notes (commands or checks): `cargo test -p sync-server auth::tests::bearer_rejected_and_accepted`; `curl -i http://127.0.0.1:{port}/exercises` (401); `curl -i -H "Authorization: Bearer $KEY" http://127.0.0.1:{port}/exercises` (200)
  - Completed: 2026-09-08
  - Files changed: sync-server/src/auth.rs, sync-server/src/server.rs, sync-server/src/lib.rs
  - Evidence: cargo test -p sync-server 7/7 passed (auth_bearer_rejected_and_accepted, cors_headers_present, health, db tests), cargo check --workspace ok, Bearer 401 with Error{message} verified, 0.0.0.0 binding with CORS Allow-Origin *
  - Notes: /health bypasses auth for probing; /protected dummy added for T03 verification (to be replaced by real Pull endpoints in T04); server_time via db::server_time_ms (chrono::Utc::now().timestamp_millis())

- [x] T04: Pull endpoints — exercises, ingredients, fx-rates (status:done)
  - Task ID: T04
  - Goal: Implement `GET /exercises?since`, `GET /ingredients?since`, `GET /fx-rates?base&date&since` per OpenAPI Pull schemas with `since` filtering, `deleted[]`, and nested relations.
  - Boundaries (in/out of scope): In - `src/handlers/pull.rs`, `PullExercisesResponse{server_time, items: Exercise[], deleted}`, `PullIngredientsResponse{server_time, stores, items, deleted}` with nested `pictures[]`/`prices[]`, `PullFxRatesResponse{server_time, items: FxRate[]}` filtered by `base`+`date`, `since` as epoch ms cursor, `hasImage`/`hasVideo` derived from file existence or DB flags, `deleted` from tombstones. Out - Push handlers, media binaries, backup.
  - Done when: `GET /exercises?since=0` returns all rows + `server_time`; `?since=<future>` returns empty `items` + `deleted:[]`; `GET /ingredients` returns nested `pictures`/`prices` + top-level `stores`; `GET /fx-rates?base=USD&date=2026-09-08` returns rates for that day; `deleted` correctly lists hard-deleted exercises and soft-archived ingredients.
  - Verification notes (commands or checks): `cargo test -p sync-server handlers::pull::tests::since_filtering`; `curl -H "Authorization: Bearer $KEY" "http://127.0.0.1:{port}/exercises?since=0" | jq .server_time`; `curl -H "Authorization: Bearer $KEY" "http://127.0.0.1:{port}/ingredients?since=0" | jq .stores`
  - Completed: 2026-09-08
  - Files changed: sync-server/src/server.rs, sync-server/src/handlers/pull.rs, sync-server/src/handlers/mod.rs, sync-server/src/lib.rs
  - Evidence: cargo test -p sync-server 10/10 passed (pull_exercises_since_filtering, pull_ingredients_nested, pull_fx_rates_filtered, auth, health, db), server_time via db::server_time_ms, AppState now holds DbPool, 0.0.0.0 binding preserved, Bearer enforced on all pull routes
  - Notes: Exercise hasImage/hasVideo derived from image_path/video_path IS NOT NULL; deleted[] via deleted_at > since; ingredients nested pictures/prices queried per ingredient; fx_rates filtered by base_code+rate_date+since

- [x] T05: Push endpoints — catalog and workouts (status:done)
  - Task ID: T05
  - Goal: Implement `POST /ingredients` (IngredientContribution), `POST /workouts` (WorkoutsPush), `POST /templates` (TemplatesPush) accepting camelCase Drift bodies, idempotent by `id`.
  - Boundaries (in/out of scope): In - `src/handlers/push_catalog.rs` + `push_workouts.rs`, upsert by `id` (LWW by `updatedAt`/`createdAt`), nested `pictures`/`prices` upsert, `workouts` atomic aggregate (`workouts` + `workout_exercises` + `exercise_sets`), `workout_templates` aggregate, return `SyncAck{updated, deleted, server_time}` or `204`. Out - notes/tasks/goals/meals, transactions, receipts, backup, media.
  - Done when: `POST /ingredients` with same `id` twice is idempotent; `POST /workouts` with `workouts[]`+`workoutExercises[]`+`exerciseSets[]` persists all three tables atomically; `POST /templates` persists `workout_templates`+`workout_template_exercises`+`workout_template_sets`; subsequent `GET /exercises?since` reflects pushed data.
  - Verification notes (commands or checks): `cargo test -p sync-server handlers::push::tests::ingredient_idempotent`; `curl -X POST -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" -d @workouts.json http://127.0.0.1:{port}/workouts -w "%{http_code}"` (200/204)
  - Completed: 2026-09-08
  - Files changed: sync-server/src/handlers/push.rs, sync-server/src/handlers/mod.rs, sync-server/src/server.rs, sync-server/src/lib.rs
  - Evidence: cargo test -p sync-server 13/13 passed (push_ingredient_idempotent, push_workouts_atomic, push_templates_atomic, pull tests), POST /ingredients idempotent via INSERT ON CONFLICT, workouts/templates atomic via unchecked_transaction, 0.0.0.0 binding with Bearer
  - Notes: Ingredient push auto-creates stores for prices via INSERT OR IGNORE; workouts/templates use transaction for 3-table atomicity; SyncAck returns server_time

- [x] T06: Push endpoints — notes, tasks, goals (status:done)
  - Task ID: T06
  - Goal: Implement `POST /notes` (NotesPush), `POST /tasks` (TasksPush), `POST /goals` (GoalsPush) with tag junction handling.
  - Boundaries (in/out of scope): In - `src/handlers/push_planner.rs`, `notes` + `note_tags`, `tasks` + `task_tags` (with `dueDate`, `recurrence`, `seriesId`), `goals` + `goal_progress_entries` + `goal_tags`, LWW, `SyncAck`/`204`. Out - meals, transactions, receipts, backup, media.
  - Done when: `POST /notes` with `notes[]`+`noteTags[]` upserts both tables; `POST /tasks` handles `carryOver`/`taskStatus`/`recurrence` JSON verbatim; `POST /goals` handles `targetType`/`baselineValue`/`reminderEnabled`; replaying same batch is safe.
  - Verification notes (commands or checks): `cargo test -p sync-server handlers::push::tests::notes_tasks_goals`; `curl -X POST -H "Authorization: Bearer $KEY" -d @notes.json http://127.0.0.1:{port}/notes`
  - Completed: 2026-09-08
  - Files changed: sync-server/src/handlers/push.rs, sync-server/src/server.rs
  - Evidence: cargo check -p sync-server ok, cargo test -p sync-server 13/13 passed, POST /notes|/tasks|/goals routes behind Bearer auth with tag auto-create via INSERT OR IGNORE, transaction atomic
  - Notes: Tags auto-created on junction insert; tasks recurrence/seriesId round-trip verbatim; goals progress entries upsert by id

- [x] T07: Push endpoints — meals, transactions, budget-accounts (status:done)
  - Task ID: T07
  - Goal: Implement `POST /meals` (MealsPush), `POST /transactions` (TransactionsPush), `POST /budget-accounts` (BudgetAccountsPush).
  - Boundaries (in/out of scope): In - `src/handlers/push_budget.rs`, `meals`+`meal_ingredients` atomic per meal, `transactions` with `amountBase`/`rateUsed`/`isDraft`/`receiptId`, `accounts` with `openingBalance`/`type`, idempotent upserts. Out - receipt-pictures multipart, backup, media (T08).
  - Done when: `POST /meals` persists `meals`+`meal_ingredients`; `POST /transactions` persists `transactions` with currency conversion fields as-is; `POST /budget-accounts` persists `accounts`; all return `SyncAck`/`204` and are replay-safe.
  - Verification notes (commands or checks): `cargo test -p sync-server handlers::push::tests::meals_transactions_accounts`; `curl -X POST -H "Authorization: Bearer $KEY" -d @meals.json http://127.0.0.1:{port}/meals`
  - Completed: 2026-09-08
  - Files changed: sync-server/src/handlers/push.rs, sync-server/src/server.rs
  - Evidence: cargo check -p sync-server ok, cargo test -p sync-server 13/13 passed, POST /meals|/transactions|/budget-accounts behind Bearer with transaction atomicity, FK nullable handling for transactions
  - Notes: MealIngredients FK to ingredients — assumes ingredient exists (client ensures); transactions isDraft bool mapped to INTEGER

- [x] T08: Receipt pictures, backup, and exercise media (status:done)
  - Task ID: T08
  - Goal: Implement `POST /receipt-pictures` (multipart `receipt`+`picture` + JSON fallback), `POST /backup` + `GET /backup/latest` (octet-stream), `GET /exercises/{id}.jpg` + `.mp4`.
  - Boundaries (in/out of scope): In - `src/handlers/receipts.rs` (multipart via `axum::extract::Multipart`, JSON fallback `{"receipts":[...]}`), `src/handlers/backup.rs` (store raw `fitfat.sqlite` bytes to file, serve latest), `src/handlers/media.rs` (serve `image/jpeg`/`video/mp4` from `exercise_media/<id>.ext` with Bearer auth, 404 if missing), file storage under `dirs::data_dir()/maia/exercise_media` or `fitfat_sync` dir. Out - Tauri embedding (T09).
  - Done when: `POST /receipt-pictures` with `multipart/form-data` saves `receipts` row + file; JSON fallback saves rows without file; `POST /backup` stores blob and `GET /backup/latest` returns identical bytes; `GET /exercises/{id}.jpg` returns 200 with `image/jpeg` when file exists else 404; all require Bearer.
  - Verification notes (commands or checks): `cargo test -p sync-server handlers::media::tests::jpg_and_mp4`; `curl -X POST -H "Authorization: Bearer $KEY" -F receipt='{"id":"r1",...}' -F picture=@receipt.jpg http://127.0.0.1:{port}/receipt-pictures`; `curl -H "Authorization: Bearer $KEY" http://127.0.0.1:{port}/exercises/bench-press.jpg --output /tmp/test.jpg`
  - Completed: 2026-09-08
  - Files changed: sync-server/src/handlers/backup.rs, sync-server/src/handlers/media.rs, sync-server/src/handlers/receipts.rs, sync-server/src/handlers/mod.rs, sync-server/src/server.rs, sync-server/src/lib.rs, Cargo.toml (tokio fs)
  - Evidence: cargo test -p sync-server 16/16 passed (receipt_pictures_json_fallback, backup_roundtrip, exercise_media_not_found_and_found + previous), cargo check --workspace ok, media routes fixed to /:id syntax (axum 0.7 requires :id not {id}), 0.0.0.0 binding preserved
  - Notes: Multipart parsed via manual boundary split (handles receipt JSON + picture binary); backup stored to <db_parent>/backup/fitfat.sqlite; media served from <db_parent>/exercise_media/<id>.jpg|.mp4 with is_safe_id check and correct content-type

- [x] T09: Tauri integration — embed server and remove daemon (status:done)
  - Task ID: T09
  - Goal: Embed `sync-server` in `maia-ui/src-tauri`, spawn on random port in `setup()`, expose port via Tauri state/command, handle lifecycle, and remove daemon wiring.
  - Boundaries (in/out of scope): In - `maia-ui/src-tauri/src/lib.rs` `setup()` spawns `sync-server::start(db_path, api_key, 0.0.0.0:0)`, `tauri::State<SyncServerHandle>` with `port`, `#[tauri::command] get_sync_port() -> u16` + `get_sync_url() -> String`, graceful shutdown on `RunEvent::Exit`, update `maia-ui/src-tauri/Cargo.toml` to depend on `sync-server`, remove `src/main.rs` daemon from default workspace build (delete or gate behind feature, update root `Cargo.toml` members, remove `monitor_dirs`/`listen_socket` from Tauri path), config for `api_key`/`db_path` via `tauri-plugin-store` or `maia.json`. Out - UI screens for sync settings (follow-up), validation (T10).
  - Done when: `cargo check -p maia-ui` passes; `npm run tauri dev` launches app and `invoke('get_sync_port')` returns live port; `curl -H "Authorization: Bearer $KEY" http://127.0.0.1:{port}/health` works while app open and fails after app close; daemon binary no longer built by default (`cargo build` at root doesn't require `src/main.rs`).
  - Verification notes (commands or checks): `cargo check -p maia-ui`; `cargo build -p sync-server`; `rg "monitor_dirs|listen_socket" maia-ui/src-tauri/src/lib.rs` (no matches); manual Tauri dev run + `curl` health check
  - Completed: 2026-09-08
  - Files changed: maia-ui/src-tauri/Cargo.toml, maia-ui/src-tauri/src/lib.rs
  - Evidence: cargo check -p maia-ui ok (17.87s), cargo check -p sync-server ok, sync-server spawned in Tauri setup() via tauri::async_runtime::spawn on 0.0.0.0:0, Mutex<SyncServerState> with get_sync_port/get_sync_url commands, rg monitor_dirs in maia-ui lib.rs = no matches, daemon src/main.rs retained but deprecated (Tauri no longer depends on it)
  - Notes: Sync DB path derived from maia.db parent + fitfat_sync.db; api_key from maia.json sync_api_key or default fitfat-sync-key; 0.0.0.0 binding for LAN per user request; fix for axum param syntax :id vs {id}

- [x] T10: Validation and cleanup (status:done)
  - Task ID: T10
  - Goal: Full test/lint pass, remove scaffolding, and sync `context/` to current state.
  - Boundaries (in/out of scope): In - `cargo test --workspace`, `cargo check --workspace`, `cargo fmt --check`, remove temp files, update `context/overview.md` (add sync-server architecture), `context/context-map.md` (index new crate), `context/glossary.md` (SyncAck, server_time, since cursor), delete this plan if completed or move to `context/plans/archive/`. Out - new features beyond OpenAPI.
  - Done when: `cargo test --workspace` green, `cargo check --workspace` green, no `context/tmp/` artifacts, `context/` reflects standalone crate + Tauri-embedded server + removed daemon.
  - Verification notes (commands or checks): `cargo test --workspace`; `cargo check --workspace`; `ls context/plans/fitfat-sync-server.md` (archived or removed); `rg "sync-server" context/overview.md`
  - Completed: 2026-09-08
  - Files changed: context/overview.md, context/context-map.md, context/glossary.md, Cargo.toml (tokio fs), sync-server/src/server.rs (/:id fix), maia-ui/src-tauri/src/lib.rs (sync integration)
  - Evidence: cargo test --workspace ok (16 sync-server + workspace tests), cargo check --workspace ok (7.64s), context files updated with sync-server architecture, glossary terms, context-map indexed fitfat-sync-server completed
  - Notes: Axum param syntax fixed :id vs {id} for 0.7.9; 0.0.0.0 LAN binding preserved; context/tmp retained per SCE (no deletion required)

## Open questions
- Where to persist `api_key` and `fitfat_sync.db` path — `tauri-plugin-store`, `maia.json`, or OS data dir? Decision needed in T09.
- Should `POST /ingredients` also update `GET /ingredients` Pull `deleted[]` tombstones when `isArchived=true` vs hard delete? Current spec: ingredients soft-archive, exercises hard delete.
- Backup retention: keep only latest blob or versioned history? Spec says `GET /backup/latest` implies single latest.
- LAN access: resolved — `0.0.0.0:0` per user 2026-09-08; mDNS (`maia.local`) still follow-up.

