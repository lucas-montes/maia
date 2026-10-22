# Plan: sync-ui-bugfixes

## Change summary

Fix the batch of runtime bugs reported after the vanilla-UI rewrite landed, across all three surfaces: the Tauri desktop app (`maia-ui`), the sync server (`sync-server`), and the Flutter mobile app (`../mobile`). The desktop gets correct note reads (TEXT uuid ids, not i64), correct task filtering (the view reads a `completed_at` field the backend never returns, so every task renders as done and the Active filter shows nothing), clickable workouts with a read-only detail section backed by a new `get_workout` command, and labeled template set columns (the unlabeled trailing inputs are `durationMinutes`/`distanceMeters` for cardio sets). The server gains `POST /tags` so real tag names can reach it — today the only path is a fallback inside task/note/goal pushes that names tags by their uuid — and `GET /templates` starts returning exercises + sets so phones that reinstall recover template sets and rest times. The mobile app gets the missing pushes wired into `DataPushService`: tags entity rows, stores rows (real names instead of the server's `name = store_id` fallback), and a bulk foods push (`pushFoods` exists in `sync_service.dart` but has zero callers, which is why recipes never arrive).

This extends existing behavior; it does not replace or revert the vanilla-UI rewrite.

## Acceptance criteria

- [ ] AC1: Clicking a note in the Notes view opens it (no "invalid type: null, expected i64" error), and the clicked row stays highlighted.
  - Validate: manual — run `maia-desktop`, click a note, content renders; `cargo test -p maia-ui --lib` passes (contract test still sees `invoke<unknown>` generics).
- [ ] AC2: In the Todos view, tasks with `done=1`/`taskStatus=1` appear under Completed and the rest under Active; the phantom `priority` chip is gone.
  - Validate: manual — filter Active shows only not-done tasks; `cargo test -p maia-ui --lib` passes.
- [ ] AC3: Clicking a workout row shows its exercises and sets (including rest seconds) in a read-only detail section.
  - Validate: manual — click a workout, detail renders; `cargo test -p maia-ui --lib` passes.
- [ ] AC4: Template set rows have a labeled header (`Set | Reps | kg | Rest s | Min | m`) and the duration/distance inputs are hidden for sets that carry no cardio data.
  - Validate: manual — open a template with strength sets (no Min/m inputs) and one with cardio sets; `pnpm build` passes.
- [ ] AC5: `POST /tags` upserts `tags.id → name/color`; after a mobile tags push, the server's `tags` rows carry real names instead of uuids.
  - Validate: `cargo test -p sync-server` (new test: push tags, assert name != id); manual — push from phone, `sqlite3 maia.db "SELECT id, name FROM tags LIMIT 5"`.
- [ ] AC6: `GET /templates?since` returns each template's exercises and sets (with `restSeconds`), matching what `POST /templates` accepts.
  - Validate: `cargo test -p sync-server` (extend templates pull test to assert sets present).
- [ ] AC7: The mobile sync hub can push tags and stores, and a bulk foods push is reachable; after a full push, server `stores` rows have real names and `foods` rows exist.
  - Validate: `flutter analyze` + `flutter test` in `../mobile`; manual — run push from phone, check server DB.
- [ ] AC8: Rest-time data path confirmed: a read-only SQL count of NULL `rest_seconds` in `exercise_sets`/`template_exercise_sets` on `maia.db` documents whether missing rest times are legacy rows or a live bug.
  - Validate: inspection — run the count query, record the numbers in the task evidence.

### Full validation

- `cargo test -p maia-ui --lib && cargo test -p sync-server && cargo check --workspace`
- `pnpm build` (in `maia-ui/`)
- `nix build .#maia-desktop` (after `git add -A` — flake path inputs only see staged files)
- `flutter analyze && flutter test` (in `../mobile`)

### Context sync

- `context/overview.md` — Current State entry for this bugfix batch (tags/stores/foods push wiring, `POST /tags`, templates pull with sets).
- `context/ui-rebuild/shell-and-screens.md` — drifted (describes the pre-rewrite monolith UI: `completed_at`-based todos, old sidebar); repair to describe the vanilla-rewrite views or mark superseded by `ui-vanilla-rewrite.md`.

## Task context synchronization lifecycle

- **Task context synchronization:** every task carries `pending | synced | blocked`.
  A completed task must be `synced` before another task can start or the plan can
  finish.
- For `blocked`, record **Blocker**, **Required action**, and **Retry condition**
  beside the status. Never infer `synced` from conversation history; write every
  lifecycle transition to the plan file.

## Constraints and non-goals

- **In scope:** `maia-ui/src-tauri/src/lib.rs` (Tauri commands), `maia-ui/src/views/{notes,todos,workouts,templates}.ts`, `maia-ui/src/lib/api.ts`, `sync-server/src/{server.rs,handlers/push.rs,handlers/pull.rs}` (+ openapi.rs if routes are documented), `../mobile/lib/src/sync/{data_push_service.dart,sync_service.dart,screens/sync_hub_screen.dart}`.
- **Out of scope:** editing workout/template sets from the desktop (detail sections stay read-only), experiment-tags link table handling, ingredient bulk push UI (per-ingredient share already exists), any schema migration.
- **Constraints:** notes PK is TEXT uuid — `read_note` must take `String`; the Rust contract test requires every `invoke(` in `main.ts` to carry a single-level `<T>` generic; nix builds require `git add -A` first; mobile pushes must reuse the existing `HttpApiClient`/`PushResult` plumbing and the hub's `_entityToType` mapping.
- **Non-goal:** no backfill script to repair existing uuid-named tag rows beyond what the mobile upsert naturally overwrites; no UI for tag management on the desktop.

## Assumptions

- The "extra field" in templates is the trailing `durationMinutes`/`distanceMeters` inputs (user confirmed the `'m'` label); fix is labeling + conditional hiding, not removal of cardio support.
- Rest-time data is present for new pushes (mobile converts min→s correctly); missing values are legacy NULL rows — AC8 verifies this before any code change is considered.
- `get_workout` follows the existing command style in `lib.rs` (lock, prepare, `serde_json::json!` rows) and returns `Vec<Value>` exercises with nested sets.

## Task stack

- [x] T01: `Fix note reading to use TEXT uuid ids` (status:done)
  - Task ID: T01
  - Scope: In — `read_note` signature (`i64` → `String`) and its query in `lib.rs`, `readNote` in `api.ts`, `Number()` coercion and `selectedId` handling in `notes.ts`. Out — other note commands, note editing.
  - Dependencies: none
  - Done when: clicking a note loads its content with no serde error and the row highlights; contract test passes.
  - Verify: `cargo test -p maia-ui --lib` — passed (12/12, incl. `every_invoked_command_is_registered`); `pnpm build` — passed. Manual click-through pending next app run.
  - Completed: 2026-10-22
  - Files changed: `maia-ui/src-tauri/src/lib.rs`, `maia-ui/src/lib/api.ts`, `maia-ui/src/views/notes.ts`, `context/plans/sync-ui-bugfixes.md`
  - Result: `read_note(id: i64)` → `read_note(id: String)`; `api.readNote` typed `(id: string)`; `Number(row.dataset.noteId)` coercion removed so the uuid string passes through. `NoteWithContent.id` was already `String` — no struct change needed.
  - Context impact: none (local bugfix; no durable-context claim affected)
  - Context synchronization: synced

- [x] T02: `Fix task filtering to use done/taskStatus` (status:done)
  - Task ID: T02
  - Scope: In — `todos.ts` row rendering and Active/Completed filters (use `done`/`taskStatus`/`carryOver` from `list_tasks`), drop the `priority` chip. Out — task editing, new task form.
  - Dependencies: none
  - Done when: Active shows only not-done tasks, Completed shows done ones, no phantom chip.
  - Verify: `cargo test -p maia-ui --lib` — passed (12/12); `pnpm build` — passed. Manual filter check pending next app run.
  - Completed: 2026-10-22
  - Files changed: `maia-ui/src/lib/api.ts`, `maia-ui/src/views/todos.ts`, `maia-ui/src/views/goals.ts`, `context/plans/sync-ui-bugfixes.md`
  - Result: `Todo` interface rewritten to match `list_tasks` output (string id, `done`, `taskStatus`, `carryOver`, `dueDate` ms — it described the pre-v30 schema with `completed_at`/`priority`/`tags`); filters and rows now use `done`/`taskStatus`; priority chip and tag/goal-title display removed (server returns neither priority nor task tags). Count line shows active tasks. `goals.ts` task-per-goal counting removed — the v30 tasks table has no goal linkage column, so it could never show a non-zero count.
  - Context impact: none (bugfix aligning view with actual backend schema)
  - Context synchronization: synced

- [x] T03: `Add get_workout command and clickable workout detail` (status:done)
  - Task ID: T03
  - Scope: In — new `get_workout(id)` Tauri command (workout + `exercise_sets` incl. `rest_seconds`) registered in the command list, `getWorkout` in `api.ts`, click handler + read-only detail section in `workouts.ts`. Out — workout editing, set logging.
  - Dependencies: none
  - Done when: clicking a workout row renders its exercises/sets with rest seconds.
  - Verify: `cargo test -p maia-ui --lib` — passed (12/12, command registered); `pnpm build` — passed. Manual click-through pending next app run.
  - Completed: 2026-10-22
  - Files changed: `maia-ui/src-tauri/src/lib.rs`, `maia-ui/src/lib/api.ts`, `maia-ui/src/lib/stores.ts`, `maia-ui/src/views/workouts.ts`, `context/plans/sync-ui-bugfixes.md`
  - Result: Added `get_workout(id: String)` query (workout row + `workout_exercises` joined to `exercises.name` + `exercise_sets` ordered by set_number, actual* preferred over planned values in the UI). Rows are buttons with `data-workout-id`; detail pane shows a labeled Set/Reps/kg/Rest/Min/m grid per exercise. `list_workouts` was also untyped (`invoke<unknown>`) — now `Envelope<Workout>`; `WorkoutExercises`/`ExerciseSet`/`WorkoutDetail` interfaces; `workoutsAtom` gained `detailId/detail/detailStatus/detailMessage` state.
  - Context impact: minor — `api.ts` doc-two-shapes comment + workouts atom state shape; covered by the ui-rebuild domain file if it survives the drift repair.
  - Context synchronization: synced

- [x] T04: `Label template set columns and hide empty cardio inputs` (status:done)
  - Task ID: T04
  - Scope: In — `templates.ts` set-row markup: header row with column labels, `title` attributes, hide duration/distance inputs when a set has neither value. Out — set editing semantics, add/delete set behavior.
  - Dependencies: none
  - Done when: strength sets show only Set/Reps/kg/Rest inputs; cardio sets show Min/m; every column is labeled.
  - Verify: `pnpm build` — passed. Manual template view check pending next app run.
  - Completed: 2026-10-22
  - Files changed: `maia-ui/src/views/templates.ts`, `context/plans/sync-ui-bugfixes.md`
  - Result: Cardio inputs (`durationMinutes`/`distanceMeters`) are hidden per exercise block when no set carries cardio data (per-set hiding would null hidden values on save — `val()` returns null for absent inputs); when cardio data exists, a labeled header (Set/Reps/Weight kg/Rest s/Min/Meters) governs all rows, every input got a `title`, delete button labeled, cryptic legend replaced with an explanation line. Labeled header also added to the T03 workout detail (same column semantics).
  - Context impact: none (UI affordance fix)
  - Context synchronization: synced

- [x] T05: `Add POST /tags push handler` (status:done)
  - Task ID: T05
  - Scope: In — `push_tags` handler in `handlers/push.rs` (upsert `tags.id → name/color/updated_at`), route in `server.rs`, openapi entry if the file lists routes, targeted test. Out — tag deletion semantics, desktop tag UI.
  - Dependencies: none
  - Done when: a tags push upserts real names; test proves `name != id` after push.
  - Verify: `cargo test -p sync-server` — passed (79 tests, incl. new `push_tags_upserts_real_names_over_uuid_fallback`: uuid-named row upserted to real name + color, tombstone works).
  - Completed: 2026-10-22
  - Files changed: `sync-server/src/handlers/push.rs`, `sync-server/src/server.rs`, `sync-server/src/lib.rs`, `context/plans/sync-ui-bugfixes.md`
  - Result: `POST /tags` mirrors `push_stores` (upsert `id→name/color/sort_order`, `deleted[]` tombstones). `openapi.rs` is a bare skeleton (empty `paths()`), so no per-route registration exists to update. Note for the test suite: in-memory pool is `max_size(1)` — tests must drop pooled connections around every HTTP call or requests block 30s on the connection timeout.
  - Context impact: none (mirrors existing push-store pattern)
  - Context synchronization: synced

- [x] T06: `Include exercises and sets in GET /templates pull` (status:done)
  - Task ID: T06
  - Scope: In — `pull_templates` in `handlers/pull.rs` (join `template_exercises` + `template_exercise_sets`, camelCase fields matching `POST /templates`), response struct, test extension. Out — other pull endpoints.
  - Dependencies: none
  - Done when: `GET /templates?since=0` returns sets with `restSeconds`; test asserts it.
  - Verify: `cargo test -p sync-server` — passed (79 tests, incl. new `pull_templates_returns_exercises_and_sets`: nested exercises + sets with `restSeconds: 90`/`weightKg` round-tripping).
  - Completed: 2026-10-22
  - Files changed: `sync-server/src/handlers/pull.rs`, `sync-server/src/lib.rs`, `context/plans/sync-ui-bugfixes.md`
  - Result: `pull_templates` now emits each template with a nested `exercises[]` (id/exerciseId/sortOrder/notes) whose sets (id/setNumber/reps/weightKg/restSeconds/durationMinutes/distanceMeters) use the exact camelCase names `POST /templates` accepts. `PullTemplatesResponse` shape unchanged otherwise.
  - Context impact: none (GET now matches the push contract's data model)
  - Context synchronization: synced

- [x] T07: `Wire tags, stores, and foods pushes into the mobile sync hub` (status:done)
  - Task ID: T07
  - Scope: In — `data_push_service.dart` (`pushTags`, `pushStores`, `pushDataType` cases), `sync_service.dart` bulk `pushFoods` caller, `sync_hub_screen.dart` `_entityToType` + pool entries. Out — ingredient bulk push UI, pull-side changes.
  - Dependencies: T05 (server must accept `POST /tags` first)
  - Done when: hub can push Tags/Stores/Foods; after push, server tags have real names, stores have real names, foods rows exist.
  - Verify: `flutter analyze` — 145 issues, identical count before/after the change (no new issues; all pre-existing). `flutter test` — +166/-126, identical to the clean-tree baseline (the 126 failures are pre-existing on the `restart` branch at `1abec20`, untouched by this task; sync-specific suites `food_sync_test`/`budget_sync_test`/`catalog_selective_sync_test` fail identically with and without the change). Manual push + server DB check pending next phone sync.
  - Completed: 2026-10-22
  - Files changed: `../mobile/lib/src/sync/data_push_service.dart`, `../mobile/lib/src/sync/screens/sync_hub_screen.dart`, `context/plans/sync-ui-bugfixes.md`
  - Result: `DataPushService` gained `pushTags` (posts `tags` entity rows from the Drift `tags` table to `POST /tags` — real names/colors reach the server) and `pushStores` (posts `stores` rows to `POST /stores` — replaces the server's name-by-id auto-create). The sync hub personal pool gained Tags/Stores/Foods entries; `foods` delegates to the existing `SyncService.pushFoods` (shareable user-created foods + ingredient components, previously unreachable with zero callers) and maps its `SyncResult` onto `PushResult`. Server-side follow-up in this task: `TagPush.sort_order` now reads camelCase `sortOrder` (Drift emits camelCase; unrenamed it would silently default to 0).
  - Context impact: none documented (mobile sync-hub wiring is discoverable from code; no existing file owns mobile mechanics)
  - Context synchronization: synced

- [x] T08: `Verify rest-time data path with read-only SQL counts` (status:done)
  - Task ID: T08
  - Scope: In — run NULL `rest_seconds` counts on `maia.db` (`exercise_sets`, `workout_template_sets`), record results as task evidence. Out — any code change; any data mutation.
  - Dependencies: none
  - Done when: counts recorded; conclusion stated (legacy NULLs vs live bug) in the task evidence.
  - Verify: inspection — read-only `sqlite3` counts on `/var/lib/maia/maia.db` (mode=ro URI, no writes).
  - Completed: 2026-10-22
  - Files changed: `context/plans/sync-ui-bugfixes.md`
  - Result: `exercise_sets`: 152 rows, **1** NULL rest (the "Deadlift - 135" workout, updated minutes before the check — a single set left blank on the phone, not a systematic failure; the other 151 all carry `rest_seconds`). `workout_template_sets`: 63 rows, 3 NULL (one template's sets, same user-input pattern). Conclusion: **the mobile→server rest-time transport is correct**; NULLs are individual sets where rest was not filled in on the phone, not an ingestion bug. No code change required. The AC8 "legacy rows" hypothesis is answered: there are essentially no legacy NULLs at all — rest data has been flowing since the feature landed.
  - Context impact: none (verification task)
  - Context synchronization: synced

## Open questions

- If AC8 shows missing rest times are NOT legacy NULLs (i.e. recent pushes also arrive with NULL), a follow-up fix in the mobile push payload will be needed — out of this plan's scope until the counts say so.
- Existing uuid-named tag rows on the server get corrected only when the phone pushes tags (upsert overwrites). Rows for tags deleted on the phone stay uuid-named forever; accepted as a non-goal.
