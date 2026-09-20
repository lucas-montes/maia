# Read-Only UI — Remove Legacy Edit Forms (Keep 8 Editable Categories)

## Change summary
Make the desktop UI globally read-only except for 8 explicitly allowed editable categories: **exercises** (`update_exercise`), **workout templates** (`workout_templates` + children), **ingredients** (`update_ingredient`), **stores** (`create_store`), **urls** (`create_url`/`delete_url`/`mark_url_read`), **receipts** (`archive_receipt` + `push_receipt_pictures`), **fxRates** (`create_fx_rate`), **settings** (`save_config`/`read_config` → `maia.json` local-only, not synced). Remove all creation/update/delete forms, handlers, and invoke branches for `tasks`/`goals`/`notes`/`workouts`/`meals`/`transactions`/`accounts`/`experiments`/`bodyMetrics` across `maia-ui/src/main.ts` (render* + handleClick/handleInput), `maia-ui/src-tauri/src/lib.rs` (8 Tauri commands), `sync-server` HTTP POST handlers (6+ routes), and `shared` Crud for non-allowed tables. Keep all `list_*`/`get_*`/`read_*` and `GET` pulls for offline-first sync. Settings remains editable as local desktop config (`maia.json` ignored via `.gitignore`, never sent to `sync-server`).

## Success criteria
- [ ] `maia-ui/src/main.ts` has no `create-todo`/`toggle-todo`/`delete-todo`/`create-goal`/`goal-add-task`/`goal-remove-task`/`goal-progress`/`create-note`/`save-note` forms or `handleClick`/`handleInput` branches; `renderTodos`/`renderGoals`/`renderNotes` are read-only viewers with search/filter only; `renderSettings` **keeps** `save-settings`/`reload-settings` form (`CONFIG_FIELD_DEFS` 7 keys) and `saveConfigFromForm`/`loadConfig`.
- [ ] `maia-ui/src-tauri/src/lib.rs:generate_handler!` no longer exposes `save_note`, `create_note`, `create_task`, `update_task`, `delete_task`, `create_goal`, `update_goal`, `delete_goal` (keeps `save_config`/`read_config`); `cargo check -p maia-ui` has no dead-code warnings for removed commands; `rg "create_task|update_task" maia-ui/src-tauri/src/lib.rs` returns only allowed `create_store`/`create_fx_rate`/`update_exercise`/`update_ingredient`/`archive_receipt`/`create_url` + `save_config`.
- [ ] `sync-server/src/server.rs` `create_router` no longer routes `POST /workouts`, `POST /notes`, `POST /tasks`, `POST /goals`, `POST /meals`, `POST /transactions`, `POST /budget-accounts`, `POST /backup` (keep `POST /ingredients`, `POST /templates`, `POST /receipt-pictures`); `cargo check -p sync-server` and `cargo test -p sync-server` 17/17 pass; `curl -X POST /tasks` returns 404/405, `curl /tasks?since=0` still 200.
- [ ] `shared/src/finances/models.rs` `Investment`/`Transaction` Crud and `shared/src/purchases/models.rs` `ReceiptMetadata`/`Purchase` not exposed for non-allowed tables (removed or `#[cfg]`-gated); `cargo check --workspace` clean.
- [ ] Allowed edits still work: `exercises` edit (`update_exercise` via `data-action='ex-edit'`), `templates` create (`tpl-create`), `ingredients` edit (`ing-edit`), `stores` create (`store-create`), `urls` create/delete/mark-read, `receipts` archive, `fxRates` add (`fx-add`), `settings` save/reload (`save_config` → `maia.json` + `maia.json.bak`) with `list_*` refresh; `cargo test --workspace` + `pnpm build` green.

## Constraints and non-goals
**Constraints:**
- Stack: Tauri 2 + `maia-ui/src-tauri` rusqlite (single `maia.db` v30) + `sync-server` Axum 0.7 + `shared` Crud macros; no new DB file.
- Read-only means no `INSERT`/`UPDATE`/`DELETE`/`fs::write`/`fs::rename` reachable from UI or HTTP for disallowed tables; `GET` pulls and `list_*` remain.
- Keep `GET` for all resources for offline-first sync (FitFat mobile still pushes via allowed POSTs, desktop reads via `since` cursors).
- **Settings (`maia.json`) is local-only** (`maia-ui/src-tauri/src/lib.rs:read_config`/`save_config` use `fs::read_to_string`/`fs::write` + backup, `sync-server` has zero `maia.json` routes, `.gitignore` ignores `maia.json`); keep `save_config`/`read_config` and `CONFIG_FIELD_DEFS` editable.

**Non-goals:**
- No new editable categories beyond the 8; no mobile UI changes; no FTS5/vector; no `PUT/DELETE` via HTTP; no new DB schema.

## Assumptions
- `fitfat` mobile is source of truth for `tasks`/`goals`/`notes`/`workouts`/`meals`; desktop is now read-only viewer for those.
- `exercises`, `workout_templates`, `ingredients`, `stores`, `urls`, `receipts`, `fxRates`, `settings` remain editable as listed; all other tables are read-only.

## Task stack (T01..T06)

- [x] T01: Frontend — strip edit forms for tasks/goals/notes (keep settings) (status:done)
  - Task ID: T01
  - Goal: Remove creation/update/delete UI for `todos`, `goals`, `notes` in `maia-ui/src/main.ts`; keep `settings` editable.
  - Boundaries (in/out of scope): In - `renderTodos` add-form (L1800-1837) + `createTodo`/`toggleTodo`/`deleteTodo` + `handleClick` branches `create-todo`/`toggle-todo`/`delete-todo`; `renderGoals` add-form + `createGoal`/`updateGoalProgress`/`deleteGoal` + `goal-add-task`/`goal-remove-task`/`create-goal`/`delete-goal` + `handleInput` slider; `renderNotes` `create-note`/`save-note` + `createNote`/`saveCurrentNote` + `handleInput` note binds. Keep `renderSettings` `formFields` + Save/Reload + `saveConfigFromForm`/`loadConfig` + `handleClick` `save-settings`/`reload-settings`. Out - `exercises`/`templates`/`ingredients`/`stores`/`urls`/`receipts`/`fxRates`/`settings` edit forms (T05), Tauri commands (T02), HTTP routes (T03).
  - Done when: `rg "data-action='(create-todo|toggle-todo|delete-todo|create-goal|goal-add-task|create-note|save-note)'" maia-ui/src/main.ts` returns 0; `rg "data-action='save-settings'"` returns 1 (kept); `pnpm build` succeeds; `renderTodos`/`renderGoals`/`renderNotes` show only filter/list, no inputs/buttons for create/update/delete.
  - Verification notes (commands or checks): `pnpm --dir maia-ui build`; `rg "createTodo|toggleTodo|deleteTodo|createGoal|saveCurrentNote" maia-ui/src/main.ts` 0; `rg "save-settings" maia-ui/src/main.ts` 1; manual click-through `todos`/`goals`/`notes` shows no edit UI, `settings` still editable.

- [x] T02: Tauri — remove 8 disallowed commands (keep settings) (status:done)
  - Task ID: T02
  - Goal: Remove `save_note`, `create_note`, `create_task`, `update_task`, `delete_task`, `create_goal`, `update_goal`, `delete_goal` from `maia-ui/src-tauri/src/lib.rs`; keep `save_config`/`read_config`.
  - Boundaries (in/out of scope): In - delete function definitions (L364 `save_note`, L437 `create_note`, L770 `create_task`, L844 `update_task`, L934 `delete_task`, L972 `create_goal`, L1042 `update_goal`, L1113 `delete_goal`) and remove from `generate_handler!` (L1569). Keep `save_config`, `read_config`, `archive_receipt`, `create_url`, `delete_url`, `mark_url_read`, `create_store`, `create_fx_rate`, `update_exercise`, `update_ingredient`, all `list_*`/`get_*`. Out - frontend (T01), HTTP (T03), `shared` (T04).
  - Done when: `cargo check -p maia-ui` has no `save_note`/`create_task` symbols; `rg "fn (save_note|create_note|create_task|update_task|delete_task|create_goal|update_goal|delete_goal)" maia-ui/src-tauri/src/lib.rs` 0; `rg "save_config" maia-ui/src-tauri/src/lib.rs` 1; `tauri::generate_handler!` contains only allowed commands + `save_config`/`read_config`.
  - Verification notes (commands or checks): `cargo check -p maia-ui`; `cargo test --workspace` (if any); `rg "create_task" maia-ui/src-tauri/src/lib.rs` 0; `rg "save_config" maia-ui/src-tauri/src/lib.rs` 1.

- [x] T03: HTTP — remove 6+ disallowed POST routes (status:done)
  - Task ID: T03
  - Goal: Make `sync-server` read-only for disallowed tables by removing `POST /workouts`, `POST /notes`, `POST /tasks`, `POST /goals`, `POST /meals`, `POST /transactions`, `POST /budget-accounts`, `POST /backup` (keep `POST /ingredients`, `POST /templates`, `POST /receipt-pictures`).
  - Boundaries (in/out of scope): In - `sync-server/src/server.rs` `create_router` route deletions, delete/gate handlers `push_workouts` (L273), `push_notes` (L505), `push_tasks` (L625), `push_goals` (L658), `push_meals` (L784), `push_transactions` (L813), `push_budget_accounts` (L833), `post_backup` (backup.rs) — or `#[cfg]`-gate; keep `push_ingredient`, `push_templates`, `push_receipt_pictures` and all `GET` pulls. Out - Tauri (T02), `shared` (T04), frontend (T01).
  - Done when: `curl -X POST -H "Authorization: Bearer test-key" http://127.0.0.1:3030/tasks -d '{}' -w "%{http_code}"` returns 404/405; `curl -H "Authorization: Bearer test-key" http://127.0.0.1:3030/tasks?since=0` still 200; `cargo check -p sync-server` and `cargo test -p sync-server` 17/17 pass (or updated).
  - Verification notes (commands or checks): `cargo check -p sync-server`; `cargo test -p sync-server`; `rg "push_tasks|push_goals|push_notes" sync-server/src/server.rs` 0 for removed routes; manual `curl` probes.

- [x] T04: Shared — gate Crud for non-allowed tables (status:done)
  - Task ID: T04
  - Goal: Prevent `shared` Crud from exposing `transactions`/`investments`/`purchases` for non-allowed writes.
  - Boundaries (in/out of scope): In - `shared/src/finances/models.rs` `Investment`/`Transaction` `#[derive(Crud)]`, `shared/src/purchases/models.rs` `ReceiptMetadata`/`Purchase` — remove `Crud` or `#[allow(dead_code)]`/feature-gate; keep `Receipt` for `receipts` allowed; `maia-macros/src/database.rs` unchanged. Out - Tauri (T02), HTTP (T03), frontend (T01).
  - Done when: `rg "table_name.*transactions|table_name.*investments|table_name.*purchases" shared/src` 0 or gated; `cargo check --workspace` clean; no `Investment::create`/`Transaction::create` reachable from disallowed code.
  - Verification notes (commands or checks): `cargo check --workspace`; `rg "Crud" shared/src/finances/models.rs` 0 or gated.

- [x] T05: Keep & fix 8 allowed edit paths (status:done)
  - Task ID: T05
  - Goal: Ensure `exercises` (`update_exercise` via `ex-edit`), `templates` (`tpl-create` → `create_template`), `ingredients` (`ing-edit`), `stores` (`store-create`), `urls` (`create_url`/`delete_url`/`mark_url_read`), `receipts` (`archive_receipt` + `push_receipt_pictures`), `fxRates` (`fx-add` → `create_fx_rate`), `settings` (`save_config`/`read_config`) remain functional after removals.
  - Boundaries (in/out of scope): In - add missing `handleClick` for `ex-edit` → `invoke("update_exercise")`, `ing-edit` → `invoke("update_ingredient")`, `tpl-create` → `invoke("create_template")` + `list_templates` in `lib.rs` if absent; keep `store-create`, `fx-add`, `create-url`, `archive_receipt`, `save-settings`/`reload-settings` wiring; verify `list_*` refresh. Out - disallowed tables (T01-T04), HTTP POST removals.
  - Done when: `exercises` edit saves and `list_exercises` reflects, `templates` create persists, `ingredients` edit saves, `stores`/`urls`/`fxRates`/`receipts` archive work, `settings` save creates `maia.json.bak` and persists; `rg "data-action='(ex-edit|ing-edit|tpl-create|store-create|fx-add|save-settings)'" maia-ui/src/main.ts` all have handlers; `cargo check -p maia-ui` and `pnpm build` green.
  - Verification notes (commands or checks): `pnpm --dir maia-ui build`; `cargo check -p maia-ui`; manual: edit exercise, create template, edit ingredient, create store/url/fxRate, archive receipt, save settings.

- [x] T06: Validation and cleanup (status:done)
  - Task ID: T06
  - Goal: Full validation and context sync.
  - Boundaries (in/out of scope): In - `cargo check --workspace`, `cargo test --workspace`, `pnpm --dir maia-ui build`, `cargo fmt --check` if available, remove `context/tmp/`, update `context/overview.md` (read-only except 8), `context/context-map.md`, `context/glossary.md`. Out - new features.
  - Done when: All checks green, `rg` for disallowed edit forms/commands returns 0 (except allowed 8), `context/` reflects read-only except 8.
  - Verification notes (commands or checks): `cargo check --workspace`; `cargo test --workspace`; `pnpm --dir maia-ui build`; `rg "create_task|delete_task|save_note" maia-ui/src-tauri/src/lib.rs` 0; `rg "save_config" maia-ui/src-tauri/src/lib.rs` 1; `ls .opencode/plans/readonly-ui.md`.

## Open questions
- Should `receipts` edit be limited to `archive_receipt` only, or also allow `update_receipt`? Currently only `archive` is kept.
