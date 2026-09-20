# Unified Maia UI — Grouped Tabs for All Models

## Change summary
Rebuild `maia-ui` from FitFat-is-separate to Maia-is-one: collapsible grouped sidebar with sections Training/Nutrition/Health/Planning/Finance, subtabs where needed, virtual-scroll + lazy media for 3799 exercises, per-tab search + per-tab sync indicator, read-only desktop for most models (editable: ingredients, exercises, workout_templates; add-only fx_rates), full ledger for finance, and read-only experiments/tags with linked-table displays. Single `maia.db` is source of truth; desktop reads via Tauri commands (direct DB) while mobile syncs via `sync-server` HTTP on `0.0.0.0:3030`.

## Success criteria
- [ ] Sidebar shows grouped sections: Training (Exercises | Workouts | Templates), Nutrition (Ingredients | Stores | Meals), Health (Body Metrics | Experiments), Planning (Notes | Tasks | Goals | URLs), Finance (Accounts | Transactions | Receipts), plus Tags, FxRates, Settings, Dashboard. Collapsible, counts per tab.
- [ ] Exercises: virtual scroll (100 visible), filters (body_part, equipment, primary_muscle, hasImage/hasVideo, text search), lazy `exercise_media/<id>.jpg|.mp4`, editable (name/body_part/equipment/primary/secondary/instructions/tips/faqs/keywords/tags).
- [ ] Workouts read-only list + detail (workout_exercises + exercise_sets with actuals), Templates editable (create/edit with exercises + planned sets, recurrence).
- [ ] Ingredients editable (name/macros/brand/barcode/isArchived) with pictures gallery + prices per store; Stores top-level tab (list + create); Meals show auto-calc calories (sum ingredient calories*grams/100) + meal_ingredients.
- [ ] Body Metrics read-only chart/table; Experiments read-only list + detail showing linked tasks/goals/notes/tags + checkins (rating 1..5 + note per day).
- [ ] Notes/Tasks/Goals/URLs each own tab, read-only desktop, showing junction links (task_experiments, task_goals, etc.) and tags.
- [ ] Tags global read-only tab: list tags (name/color/sortOrder) + usage counts per entity (tasks/experiments/goals/notes).
- [ ] Finance full ledger: Accounts list, Transactions list (type/amount/currency/amountBase/rateUsed/category/date/receipt), Receipts with image + parsedJson + transaction link.
- [ ] FxRates: list by base/date/since, add new rate, refresh (fetch), no edit.
- [ ] Per-tab sync indicator (server_time, since cursor, deleted count) and per-tab search (no global search).
- [ ] `cargo check --workspace` + `pnpm build` green; `cargo test -p sync-server` 17/17 pass; manual click-through of each tab shows data from `maia.db`.

## Constraints and non-goals
**Constraints:**
- Stack: Tauri 2 + `maia-ui/src-tauri` Rust (rusqlite bundled, WAL+FK) + `sync-server` Axum 0.7 on `0.0.0.0:3030` (Bearer). No new DB file; `maia.db` only. Reuse `sync-server/src/db.rs` schema (40 tables) + `exercise_media/` dir.
- Desktop read-only for most: editable only `ingredients`, `exercises`, `workout_templates`; `fx_rates` add-only; others read-only (mobile is writer). Enforce via UI (no edit buttons) not DB triggers.
- Virtual scroll for exercises (3799), lazy `img`/`video` via `loading="lazy"` + `onerror` hide, `GET /media/:id.jpg|.mp4` with Bearer for mobile parity but desktop uses `asset://` or `convertFileSrc` for local files.
- Per-tab sync: each tab fetches via Tauri command `list_<model>` with `since` cursor, shows `server_time` + `deleted` count; no global search.

**Non-goals:**
- No mobile UI changes, no new sync protocol, no FTS5/vector search, no S3, no field-level merge, no editing of body_metrics/experiments on desktop, no fx_rates edit/delete, no tag editing on desktop.

## Assumptions
- `maia.db` already seeded with 3799 exercises + `exercise_media/` (from `data/` ingest). `sync-server` auto-seeds if empty.
- FitFat mobile owns workouts/meals/body_metrics creation; desktop displays via same `maia.db` rows.
- `tags` vocabulary is shared; desktop shows usage via junction tables (`task_tags`, `experiment_tags`, `goal_tags`, `note_tags`).

## Task stack (T01..T13)

- [x] T01: Grouped collapsible sidebar + routing (status:todo)
  - Task ID: T01
  - Goal: Replace flat sidebar with grouped collapsible sections and subtab routing for Training.
  - Boundaries (in/out of scope): In - `maia-ui/src/main.ts` `renderSidebar` + `ViewId` union + `setView` + CSS for groups, Training subtabs (Exercises|Workouts|Templates). Out - data fetching, per-tab content (T02+).
  - Done when: Sidebar shows Training/Nutrition/Health/Planning/Finance groups, collapsible, Training expands to 3 subtabs, `setView('exercises')` etc. works, counts per tab.
  - Verification notes (commands or checks): `pnpm build`; manual click each group/tab; `rg "ViewId" maia-ui/src/main.ts`.

- [x] T02: Exercises catalog — virtual scroll, filters, lazy media, editable (status:todo)
  - Task ID: T02
  - Goal: Exercises tab with virtual scroll (100 window), filters (body_part, equipment, primary_muscle, hasImage/hasVideo, text search), lazy thumbnails, detail with instructions/tips/faqs/keywords + edit.
  - Boundaries (in/out of scope): In - Tauri commands `list_exercises`/`get_exercise`/`update_exercise` (rusqlite), `maia-ui/src/main.ts` exercises state + render + `GET /media/:id` fallback, `sync-server/src/db.rs` exercises table. Out - workouts/templates (T03), ingredients (T04).
  - Done when: 3799 exercises searchable, filter combos work, scroll virtualized, image lazy loads from `exercise_media/<id>.jpg`, edit saves to `maia.db` and reflects after refresh.
  - Verification notes (commands or checks): `cargo check -p maia-ui`; `sqlite3 maia.db "SELECT count(*) FROM exercises"`; `curl -H "Authorization: Bearer $KEY" http://127.0.0.1:3030/exercises?since=0 | jq .items[0]`; manual edit.

- [x] T03: Workouts + Templates (Training subtabs) (status:todo)
  - Task ID: T03
  - Goal: Workouts read-only list/detail (workout_exercises + exercise_sets with actuals) and Templates editable (CRUD with exercises + planned sets + recurrence).
  - Boundaries (in/out of scope): In - Tauri commands `list_workouts`/`list_workout_templates`/`create_template`/`update_template`, `maia-ui` Training subtabs, `workout_templates` + `workout_template_exercises` + `workout_template_sets` tables. Out - exercises catalog (T02), meals (T05).
  - Done when: Workouts show date/notes/routineId/templateId + nested exercises/sets; Templates can be created/edited (name/notes/startDate/recurrence/exercises/sets) and persist; desktop cannot create workouts.
  - Verification notes (commands or checks): `cargo test -p sync-server push_templates_atomic`; manual create template + verify `SELECT * FROM workout_templates`.

- [x] T04: Ingredients + Stores + Prices (Nutrition) (status:todo)
  - Task ID: T04
  - Goal: Ingredients editable with pictures gallery + price history sparkline per ingredient per store; Stores top-level tab.
  - Boundaries (in/out of scope): In - Tauri commands `list_ingredients`/`update_ingredient`/`list_stores`/`create_store`/`list_ingredient_prices` (history), `maia-ui` Ingredients + Stores tabs with sparkline, `ingredients`/`stores`/`ingredient_pictures`/`ingredient_prices` tables. Out - meals (T05), fx_rates (T11).
  - Done when: Ingredients list shows macros/brand/barcode/isArchived + pictures + price history sparkline per ingredient-store (time vs price); edit saves; Stores tab lists/creates stores.
  - Verification notes (commands or checks): `sqlite3 maia.db "SELECT count(*) FROM ingredients"`; manual edit ingredient + sparkline renders.

- [x] T05: Meals with auto-calc all macros (status:todo)
  - Task ID: T05
  - Goal: Meals tab read-only showing meal_ingredients + auto-calc all available macros (calories/protein/carbs/fat/sodium/fiber/sugar * grams/100).
  - Boundaries (in/out of scope): In - Tauri commands `list_meals` with join to `meal_ingredients` + `ingredients`, `maia-ui` Meals tab calc including optional sodium/fiber/sugar, `meals`/`meal_ingredients` tables. Out - ingredient editing (T04), body_metrics (T06).
  - Done when: Meals list shows eatenAt + ingredients + computed all macros; no edit UI on desktop.
  - Verification notes (commands or checks): `cargo check -p maia-ui`; manual verify calc vs `SELECT` sum.

- [x] T06: Body Metrics read-only (status:todo)
  - Task ID: T06
  - Goal: Body Metrics tab read-only chart + table (date, weightKg, heightCm).
  - Boundaries (in/out of scope): In - Tauri command `list_body_metrics`, `maia-ui` chart (Chart.js) + table, `body_metrics` table. Out - experiments (T07), editing.
  - Done when: Chart renders weight over time, table lists rows, no create/edit buttons.
  - Verification notes (commands or checks): `sqlite3 maia.db "SELECT * FROM body_metrics LIMIT 1"`; manual view.

- [x] T07: Experiments read-only with linked tables + calendar heatmap (status:todo)
  - Task ID: T07
  - Goal: Experiments tab read-only showing linked tasks/goals/notes/tags + calendar heatmap of checkins (rating 1..5 color).
  - Boundaries (in/out of scope): In - Tauri commands `list_experiments` + `list_experiment_checkins` + junction queries (`task_experiments`, `experiment_goals`, `experiment_notes`, `experiment_tags`), `maia-ui` Experiments tab heatmap, `experiments`/`experiment_checkins` tables. Out - editing, body_metrics (T06).
  - Done when: Experiments list shows name/purpose/status/categories/dates + detail shows linked entities + heatmap calendar (day cell color by rating, tooltip note); no edit.
  - Verification notes (commands or checks): `sqlite3 maia.db "SELECT count(*) FROM experiments"`; manual heatmap.

- [x] T08: Notes / Tasks / Goals / URLs separate tabs (status:todo)
  - Task ID: T08
  - Goal: Each Planning entity own tab, read-only desktop, showing junction links and tags.
  - Boundaries (in/out of scope): In - Tauri commands `list_notes`/`list_tasks`/`list_goals`/`list_urls` + junction queries (`task_goals`, `task_notes`, `goal_notes`, `goal_workouts`, `note_workouts`, `*_tags`), `maia-ui` 4 tabs. Out - tags global (T09), finance (T10).
  - Done when: Each tab lists items with tags + linked entities; detail shows notes body, task dates/status, goal progress, url is_new; no edit on desktop.
  - Verification notes (commands or checks): `cargo check -p maia-ui`; manual click each tab.

- [x] T09: Tags global read-only with usage (status:todo)
  - Task ID: T09
  - Goal: Tags tab read-only showing name/color/sortOrder + usage counts per entity (tasks/experiments/goals/notes).
  - Boundaries (in/out of scope): In - Tauri command `list_tags` with counts via junction tables, `maia-ui` Tags tab. Out - editing, other tabs.
  - Done when: Tags list shows color + counts (e.g., "3 tasks, 1 goal"), sorted by sortOrder; no edit.
  - Verification notes (commands or checks): `sqlite3 maia.db "SELECT * FROM tags"`; manual view.

- [x] T10: Finance full ledger — Accounts, Transactions, Receipts (status:todo)
  - Task ID: T10
  - Goal: Finance section with Accounts, Transactions, Receipts (image + parsedJson + transaction link) full ledger UI.
  - Boundaries (in/out of scope): In - Tauri commands `list_accounts`/`list_transactions`/`list_receipts` + `get_receipt_image`, `maia-ui` Finance tabs, `accounts`/`transactions`/`receipts` tables, `exercise_media` pattern for receipt images. Out - fx_rates (T11), editing (read-only except maybe accounts).
  - Done when: Accounts show type/openingBalance; Transactions show type/amount/currency/amountBase/rateUsed/category/date/receipt; Receipts show localPath image + parsedJson + transactionId link; no desktop edit for transactions/receipts.
  - Verification notes (commands or checks): `sqlite3 maia.db "SELECT count(*) FROM receipts"`; manual receipt image load.

- [x] T11: FxRates — add new + refresh, no edit (status:todo)
  - Task ID: T11
  - Goal: FxRates tab list by base/date/since, add new rate, refresh (fetch), no edit/delete.
  - Boundaries (in/out of scope): In - Tauri commands `list_fx_rates`/`create_fx_rate`/`refresh_fx_rates`, `maia-ui` FxRates tab, `fx_rates` PK (code, base_code, rate_date). Out - editing existing rates.
  - Done when: List filters by base/date/since, add form creates new (code/base/rateDate/rateToBase), refresh re-fetches; edit button absent.
  - Verification notes (commands or checks): `curl -H "Authorization: Bearer $KEY" "http://127.0.0.1:3030/fx-rates?base=USD&date=2026-09-08&since=0"`; manual add.

- [x] T12: Per-tab sync + per-tab search (status:todo)
  - Task ID: T12
  - Goal: Each tab shows sync indicator (server_time, since cursor, deleted count) and has its own search input (no global search).
  - Boundaries (in/out of scope): In - `maia-ui/src/main.ts` per-tab `since` state + `fetch` via Tauri commands, sync badge UI, per-tab search filtering. Out - global search, new models.
  - Done when: Each tab header shows "Synced at <server_time> • <n> deleted" and search filters that tab's list only.
  - Verification notes (commands or checks): `pnpm build`; manual search per tab.

- [x] T13: Validation and cleanup (status:todo)
  - Task ID: T13
  - Goal: Full test/lint pass, remove scaffolding, sync `context/`.
  - Boundaries (in/out of scope): In - `cargo test --workspace`, `cargo check --workspace`, `pnpm build`, remove `context/tmp/`, update `context/overview.md` (unified UI), `context/context-map.md`, `context/glossary.md`. Out - new features.
  - Done when: `cargo test --workspace` green, `cargo check --workspace` green, `pnpm build` green, `context/` reflects grouped tabs + single `maia.db` + `exercise_media`.
  - Verification notes (commands or checks): `cargo test --workspace`; `cargo check --workspace`; `pnpm build`; `ls context/plans/ui-unified-tabs.md`.

## Open questions
- None — price history sparkline, all-macros auto-calc, heatmap confirmed 2026-09-08.
