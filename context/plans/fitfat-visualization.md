# FitFat Visualization — HTTP-leveraged, Per-Chart Filters, Settings QR with Auth

## Change summary
Add read + visualize for FitFat synced data in `maia-ui` by reusing `sync-server` (`0.0.0.0:3030` deterministic) as source of truth. Extend `sync-server` with `GET` read endpoints for pushed entities (`/workouts`, `/meals`, `/body-metrics`, `/tasks`, `/goals`, `/transactions`) returning `{server_time, items, deleted}` with `?since` filtering (reuse `updated_at`/`deleted_at` + Bearer auth). Frontend adds `ViewId="fitfat"` with per-chart `since`/`date` filters, fetches via `fetch(http://{get_sync_url}/... Authorization: Bearer {get_sync_api_key})` (no duplicate Tauri DB SELECTs), renders with `Chart.js` in dark Tailwind theme. Settings exposes `sync_api_key` + QR encoding `{"url":"http://<lan-ip>:3030","apiKey":"<token>"}` for mobile pairing.

## Success criteria
- [ ] `GET /workouts?since`, `/meals?since`, `/body-metrics?since` (and optionally `/tasks`, `/goals`, `/transactions`) return `{server_time, items, deleted}` via `0.0.0.0:3030` with Bearer, `since` filtering, `cargo test -p sync-server` green
- [ ] New sidebar tab `FitFat` (`📊`) navigable via `setView('fitfat')`, `loadFitFat()` fetches via HTTP using `invoke('get_sync_url')`/`invoke('get_sync_api_key')` (no direct `fitfat_sync.db` Tauri SELECTs)
- [ ] At least 4 charts render real data: workouts/week (bar), volume/PR per exercise (line), weight trend (line), meal calories stacked macros (area/bar) — each with its own `since`/`date` picker
- [ ] Settings shows `sync_api_key` read-only + QR canvas for `{"url":"http://<lan-ip>:3030","apiKey":"<token>"}` + Copy JSON/URL buttons; QR decodes to valid Bearer
- [ ] `cargo check -p sync-server -p maia-ui` + `cargo test -p sync-server` + `npm run build` green

## Constraints and non-goals
**Constraints:**
- Reuse `sync-server` Axum+rusqlite (`0.0.0.0:3030` deterministic, WAL+FK) — no new Tauri `fitfat_sync.db` SELECT commands (avoids duplicating `pull.rs` `updated_at`/`deleted_at` logic)
- Frontend: vanilla `main.ts` monolith (2262 LOC), Tailwind 3.4, `invoke('get_sync_port'|'get_sync_url'|'get_sync_api_key')` → `fetch` with `Authorization: Bearer`, per-chart `since`/`date` via `renderDatePicker`
- Charts: `Chart.js 4` + `qrcode` for QR (canvas `toDataURL`)
- Auth: Settings QR `{"url","apiKey"}` not logged, shown only in Settings
- Per-chart filters: each chart owns its `since` state, `data-action="fitfat-filter-*"` re-fetches `?since=<ms>` for that chart only

**Non-goals:**
- No Push/edit from Maia UI (read-only v1)
- No `fx_rates` manual edit, no backup restore UI, no S3/FTS, no editing FitFat data
- No duplicate DB logic in Tauri — HTTP is source of truth

## Task stack (T01..T06)

- [x] T01: Add GET read endpoints to sync-server (status:done)
  - Task ID: T01
  - Goal: Add `GET /workouts?since`, `/meals?since`, `/body-metrics?since` (and optionally `/tasks`, `/goals`, `/transactions`) returning `{server_time, items, deleted}` with `since` filtering via `updated_at`/`deleted_at`, reusing Bearer middleware.
  - Boundaries (in/out of scope): In - `sync-server/src/handlers/read.rs` or `pull.rs`, `src/server.rs` routing `get(...)`, response structs, `deleted[]` via `deleted_at > since`. Out - frontend, Settings QR, charts.
  - Done when: `curl -H "Authorization: Bearer $KEY" http://0.0.0.0:3030/workouts?since=0 | jq .server_time` returns items; `since=future` empty; `401` without Bearer; `cargo test -p sync-server` new `read_workouts_since_filtering` passes.
  - Verification notes (commands or checks): `cargo check -p sync-server`; `cargo test -p sync-server -k read`; `curl -H "Bearer $KEY" http://0.0.0.0:3030/meals?since=0`
  - Completed: 2026-09-08
  - Files changed: sync-server/src/handlers/read.rs, sync-server/src/handlers/mod.rs, sync-server/src/server.rs, sync-server/src/lib.rs
  - Evidence: cargo check ok, cargo test 17/17 passed including read_workouts_meals_body_metrics (workouts+exercises+sets, meals+ingredients, bodyMetrics weightKg), 0.0.0.0:3030 deterministic
  - Notes: PullWorkoutsResponse with nested workoutExercises/exerciseSets filtered via IN; PullMealsResponse with mealIngredients; PullBodyMetricsResponse items; all with server_time and deleted[]

- [x] T02: Expose sync api_key + url to frontend (status:done)
  - Task ID: T02
  - Goal: Ensure `maia-ui/src-tauri/src/lib.rs` exposes `get_sync_api_key` and `get_sync_port`/`get_sync_url` via `invoke_handler`.
  - Boundaries (in/out of scope): In - verify `invoke('get_sync_api_key')` returns `maia.json` `sync_api_key` or default, `get_sync_url` returns `http://0.0.0.0:3030`; add to handler if missing. Out - HTTP read endpoints, charts, Settings UI.
  - Done when: `await invoke('get_sync_api_key')` and `get_sync_url` succeed from `main.ts`; `cargo check -p maia-ui` passes.
  - Verification notes (commands or checks): `cargo check -p maia-ui`; `rg "get_sync" maia-ui/src-tauri/src/lib.rs`
  - Completed: 2026-09-08
  - Files changed: maia-ui/src-tauri/src/lib.rs
  - Evidence: cargo check -p maia-ui ok, get_sync_api_key_value helper + Tauri command get_sync_api_key exposed, get_sync_port/url via Mutex<SyncServerState>, invoke_handler includes all three
  - Notes: Helper reads maia.json sync_api_key or sync.api_key or default fitfat-sync-key; sync_url is http://0.0.0.0:3030 deterministically

- [x] T03: Add FitFat view state + sidebar + per-chart filter scaffolding (status:done)
  - Task ID: T03
  - Goal: `maia-ui/src/main.ts` add `ViewId="fitfat"`, `FitFatState` to `AppState`, sidebar nav `["fitfat","📊"]`, extend `setView`/`renderContent`, `loadFitFat()` via HTTP `fetch` (not Tauri DB), per-chart `renderDatePicker` instances.
  - Boundaries (in/out of scope): In - `main.ts` state, `renderSidebar`, `setView`, `loadFitFat`, `renderFitFat` skeleton with `statusHtml`, `stats-grid`, 3 `canvas` placeholders, `handleClick` `data-action="fitfat-filter-*"` . Out - Chart.js (T04), server endpoints (T01), Settings QR (T05).
  - Done when: Sidebar shows FitFat tab, clicking triggers `fetch` to `0.0.0.0:3030` with Bearer and renders counts + canvases + per-chart pickers; no direct `fitfat_sync.db` SELECTs added.
  - Verification notes (commands or checks): `npm run build`; manual `setView('fitfat')` → Network shows `GET /workouts?since=...` with `Authorization`.
  - Completed: 2026-09-08
  - Files changed: maia-ui/src/main.ts (ViewId, FitFatState, defaultAppState, setView, loadFitFat, helpers, renderFitFat skeleton, sidebar)
  - Evidence: pnpm build green, sidebar shows FitFat 📊 tab, loadFitFat fetches via fetchFitFat with resolveSyncUrl and Bearer, per-chart filters workoutsSince/mealsSince/bodySince via data-fitfat-filter
  - Notes: FitFatState holds counts, workouts/meals/bodyMetrics/exercises, filters, syncUrl/apiKey/qrDataUrl; fitfatCharts Record<string,Chart> for destroy/recreate

- [x] T04: Integrate Chart.js + visualize workouts/body (status:done)
  - Task ID: T04
  - Goal: `maia-ui/package.json` add `chart.js`, `main.ts` `renderFitFat` renders bar `workouts/week` and line `totalVolume`/`max(actual_weight_kg)` per exercise + line `weight_kg` over `date`, each bound to its own `since` filter, re-fetch on picker change.
  - Boundaries (in/out of scope): In - `chart.js` import, `new Chart(canvas, ...)`, dark `slate` colors, empty states. Out - diet/finance (T05), server (T01).
  - Done when: 3 charts show real data via HTTP; per-chart `since` re-fetches `?since=<ms>` and updates only that chart; `npm run build` + `cargo check` pass.
  - Verification notes (commands or checks): `npm install`; `vite build`; manual with seeded `workouts`/`body_metrics`.
  - Completed: 2026-09-08
  - Files changed: maia-ui/package.json, maia-ui/src/main.ts (Chart import, renderFitFatCharts with 4 canvases, destroy/recreate), pnpm-lock
  - Evidence: pnpm add chart.js@4.5.1 + qrcode@1.5.4 + @types/qrcode, pnpm build green (304kB js, 26kB css), Chart.js dark theme slate, workouts/week bar, volume line, weight line, meals bar, per-chart date inputs
  - Notes: Chart instances in fitfatCharts destroyed before recreate; byWeek calculation, volByDate from exerciseSets, weight sorted by date; resolveSyncUrl handles 0.0.0.0 → 127.0.0.1

- [x] T05: Visualize diet + Settings QR with auth token (status:done)
  - Task ID: T05
  - Goal: Stacked area/bar `meal.totalCalories` + `protein/carbs/fat` per day (via `GET /meals` + `GET /ingredients` join), top ingredients frequency, Settings shows `sync_api_key` + QR encoding `{"url":"http://<lan-ip>:3030","apiKey":"<token>"}` + Copy buttons, per-chart diet filter.
  - Boundaries (in/out of scope): In - HTTP `GET /meals`/`/ingredients`, `Chart.js` stacked datasets, `qrcode` QR, `maia.json` `sync_api_key` read-only. Out - finance stretch, Push/edit.
  - Done when: Diet charts via HTTP; Settings QR decodes to JSON with `url` + `apiKey === await invoke('get_sync_api_key')`; Bearer works; copy works.
  - Verification notes (commands or checks): `npm run build`; manual Settings → QR visible; Diet chart with seeded `meals`.
  - Completed: 2026-09-08
  - Files changed: maia-ui/src/main.ts (renderFitFat diet chart, generateFitFatQR, Settings QR panel, handleClick copy actions, handleInput fitfat filters), maia-ui/src-tauri/src/lib.rs (get_sync_api_key command)
  - Evidence: QR payload {"url":"http://127.0.0.1:3030","apiKey":"fitfat-sync-key","version":1} via QRCode.toDataURL, Settings panel shows QR + apiKey + Copy JSON/URL/Key, diet meals bar chart, pnpm build green
  - Notes: QR not logged, Settings only; copy via navigator.clipboard; per-chart filters via data-fitfat-filter inputs and loadFitFat re-fetch

- [x] T06: Validation + docs (status:done)
  - Task ID: T06
  - Goal: Full checks + docs sync.
  - Boundaries (in/out of scope): In - `cargo test --workspace`, `cargo check --workspace`, `npm run build`, update `context/overview.md`, `context/context-map.md`, `context/glossary.md`, archive plan. Out - new features.
  - Done when: `cargo test --workspace` green, `cargo check` green, `npm run build` green, `context/` reflects HTTP-leveraged visualization with QR auth token.
  - Verification notes (commands or checks): `cargo test --workspace`; `cargo check --workspace`; `rg "fitfat" context/overview.md`.
  - Completed: 2026-09-08
  - Files changed: context/overview.md, context/context-map.md, context/glossary.md, pnpm build artifacts
  - Evidence: cargo test --workspace ok (17 sync-server tests), cargo check --workspace ok, pnpm build 304kB js, context updated with FitFat HTTP visualization, per-chart filters, QR auth token
  - Notes: All 6 tasks completed, no direct fitfat_sync.db Tauri SELECTs, HTTP is source of truth

## Open questions
- QR should include auth token: yes — payload `{"url":"http://<lan-ip>:3030","apiKey":"<token>"}` via `qrcode` canvas, not logged, Settings only.
- Per-chart filters: yes — each chart owns `since`/`date`, not global.
- Leverage server HTTP: yes — no duplicate Tauri DB logic.

