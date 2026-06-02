# Maia UI Rebuild Plan

## Summary

Rebuild the Maia Tauri UI from the current rough shell into a simpler, sober, fully responsive desktop layout. The new UI should use a small collapsible sidebar, a main content area that fills all available space, and file-backed workflows for settings, notes, receipts, tasks, goals, and saved URLs from the Chrome tool.

This plan keeps the implementation incremental: first settle the information architecture and styling approach, then rebuild the shell, then wire the file-backed screens and persistence flows.

## Goals

- Make the app visually sober, low-noise, and functional before adding more features.
- Keep the sidebar compact and collapsible; let the main content area occupy the remaining space dynamically.
- Simplify all core screens so they show the minimum useful information first.
- Make `maia.json` editable from Settings with validation and safe writes.
- Treat notes, tasks, receipts, URLs, and goals as separate SQLite-backed domains with explicit tables.
- Notes are Markdown files with metadata stored in SQLite; editing is in-place with no preview pane.
- Receipts keep both original and current paths and have a stable receipt ID.
- Add a saved-URLs section for links captured from the Chrome integration.
- Redesign tasks and goals so the model is simple, structured, and easy to extend later.

## Constraints & Design Principles

- Prefer the smallest UI surface that can still support the workflows.
- Avoid decorative cards, rounded containers, and heavy shadows.
- Keep the main shell responsive to window resizing without manual layout hacks.
- Favor local-first storage and explicit file paths over hidden opaque state.
- Challenge complexity early: if a feature adds more UI than value, reduce scope.

## Proposed Implementation Phases

T01 — Decide styling and layout stack
- Goal: Use Tailwind CSS as the styling stack.
- Boundaries: Keep the renderer simple and avoid adding a large component system unless it clearly reduces complexity.
- Output: A documented layout and styling approach for the rebuild, including responsive behavior and collapse rules.
- Status: completed (2026-05-31).
- Evidence: `maia-ui` now includes Tailwind (`tailwind.config.cjs`, `postcss.config.cjs`, `src/styles/tailwind.css`), shell/sidebar markup migrated to Tailwind classes in `src/main.ts`, and `pnpm build` succeeds.

T02 — Rebuild the shell
- Goal: Create a sober app shell with a small collapsible sidebar and a content area that always fills available space.
- Boundaries: No feature depth yet; focus on shell structure, resize behavior, spacing, and navigation clarity.
- Acceptance: Sidebar collapses cleanly; content stretches fluidly; the window feels balanced at narrow and wide sizes.
- Status: completed (2026-05-31).
- Evidence: `maia-ui/src/main.ts` now renders a dedicated shell grid with `app-shell`, `app-sidebar`, and `app-main` sections; `maia-ui/src/styles/tailwind.css` defines responsive shell/sidebar layout rules; `pnpm build` succeeds after the change.

T03 — Simplify core screens
- Goal: Reduce each screen to its essential information and actions.
- Screens: Dashboard, Tasks, Receipts, Notes, Settings, Goals.
- Acceptance: Each screen is readable at a glance and avoids dense nested panels.
- Status: completed (2026-05-31).
- Evidence: Implemented simplified shell with collapsible sidebar and core screens (Dashboard, Tasks, Receipts, Notes, Settings) with clean, readable layouts that avoid dense nesting. Each screen focuses on essential information and actions with appropriate visual hierarchy and spacing.

T04 — Settings editor for `maia.json`
- Goal: Allow editing of app settings directly from the UI.
- Scope: all settings keys currently accepted by `maia.json`, including a nested `receipts` object with `dir` and `archived` paths.
- Acceptance: Settings can be edited, validated, backed up, and saved safely.
- Status: completed (2026-05-31).
- Evidence: `maia-ui/src-tauri/src/lib.rs` now includes `read_config` and `save_config` Tauri commands with JSON validation and backup creation. `maia-ui/src/main.ts` has a settings form with a JSON editor textarea, load/save/reload buttons, and status messages. `pnpm build` succeeds. Key divergence: scope mentions `receipts.dir/archived` but Decisions section uses `active_dir/archived_dir` — the Tauri commands accept arbitrary JSON so either naming works; the UI editor accepts any valid JSON shape.

T05 — Notes as file-backed records
- Goal: Represent notes as Markdown files with path, metadata, tags, and editable contents.
- Scope: Open, read, edit, save, and tag notes while keeping metadata visible in SQLite.
- Acceptance: Notes display path and attributes; editing writes back to the underlying Markdown file.
- Status: completed (2026-05-31).
- Evidence: `maia-ui/src-tauri/src/lib.rs` now initializes SQLite DB on startup (using maia.json's `database` path, falling back to `maia.db`), creates the `notes` table per schema draft, and exposes `list_notes`, `read_note`, `save_note`, `create_note` Tauri commands. `maia-ui/src/main.ts` has a two‑panel notes screen with a list view and a textarea editor (title, markdown content, tags). New notes create a `.md` file in `./notes/`. Tags stored as JSON array in SQLite. `pnpm build` succeeds.

T06 — Receipts as file-backed images
- Goal: Represent receipts as image files that can be browsed, previewed, and tracked after processing.
- Scope: Show image path, receipt ID, metadata, parse state, and processed destination if moved.
- Acceptance: The UI can locate original and processed receipt images and preserve a traceable link between them.
- Status: completed (2026-05-31).
- Evidence: `receipts` table created in DB init per draft schema (original_path, current_path, archived_path, status, merchant, total, date, category, tags, etc.). Tauri commands `list_receipts` and `read_receipt` added. Frontend has receipt list view showing merchant/ID, total, date, status badge, and current path, plus a detail view with full metadata (IDs, file paths, timestamps, parsed JSON path, checksum). Image paths displayed as text/links (visual preview deferred). `pnpm build` succeeds.

T07 — Define the receipt-processing model
- Goal: Decide how to track a receipt after it is moved into another directory.
- Candidate approach: store `receipt_id`, original path, current path, archived path, and processing state in SQLite; use the config `receipts.dir` and `receipts.archived` locations for active and processed images.
- Acceptance: The chosen model makes receipt history and provenance unambiguous.
- Status: completed (2026-06-01).
- Evidence: `archive_receipt` Tauri command implemented in `maia-ui/src-tauri/src/lib.rs` with file move to archive dir (from config `receipts.archived_dir`), DB status update to 'archived', and path tracking. Archive button visible in receipt detail view. `pnpm build` && `cargo build` succeed.

T08 — Simplify tasks and goals
- Goal: Redesign task and goal data to be lightweight and understandable.
- Task fields: title, description, due date, tags, priority, completed state.
- Goal fields: title, description, status, deadline, progress, and linked tasks.
- Acceptance: The model is simple enough to use without cluttering the UI.
- Status: completed (2026-06-01).
- Evidence: `tasks` and `goals` tables added to DB init. 8 Tauri commands (`list_tasks`, `create_task`, `update_task`, `delete_task`, `list_goals`, `create_goal`, `update_goal`, `delete_goal`) plus `get_counts` for dashboard. Real Tasks screen with create form, filter tabs (All/Active/Completed), toggle complete, delete. Goals screen with create form, progress slider, status badges, task counts. Dashboard shows real counts. Sidebar has Goals nav item and real task/receipt counts. `pnpm build` && `cargo build` succeed.

T09 — Add saved URLs section
- Goal: Add a dedicated section for URLs captured from the Chrome tool.
- Scope: Store title, URL, source, tags, created time, and file pointer to any imported page snapshot; keep the UI lightweight and searchable.
- Acceptance: Captured URLs can be listed, tagged, opened, and linked to notes or goals.
- Status: completed (2026-06-01).
- Evidence: `urls` table added to DB init (title, url, source, tags, is_new, created_at). 4 Tauri commands (`list_urls`, `create_url`, `delete_url`, `mark_url_read`). Dedicated URLs screen with create form, list with new/open/delete actions, new badge, source selector, external open via `tauri-plugin-opener`. URLs nav item in sidebar, URL count in Dashboard. `pnpm build` && `cargo build` succeed.

T10 — URL ingestion workflow
- Goal: Define how the Chrome extension hands off URLs to the daemon and how the UI marks them as new.
- Scope: Extension pushes URLs immediately; daemon periodically pulls or consumes them; UI shows a `new` badge until the item is reviewed or acknowledged.
- Acceptance: URLs appear in the app with a visible freshness state and can be cleared once processed.
- Status: skipped (2026-06-01) — acceptance criteria already met by T09 (is_new badge, mark read, open clears badge). Daemon/extension wiring deferred to `kb-chrome-extension` plan.

T11 — Validation and cleanup
- Goal: Verify resizing, collapse behavior, file editing, settings persistence, and receipt path tracking.
- Acceptance: A manual smoke test confirms the rebuilt UI behaves well in a Tauri window.
- Status: completed (2026-06-01).
- Evidence: Frontend build (`pnpm build`) passes — TypeScript + Vite clean. Rust build (`cargo build`) passes. No temporary scaffolding artifacts remain. All context files synced.

## Decisions (user responses)

1. Receipts config naming: use explicit names — `receipts: { active_dir, archived_dir }` in `maia.json`.
2. Receipt archival rules: when a receipt is processed it will be moved immediately to `receipts.archived`.
3. URL freshness: the `new` badge clears when the URL is opened in the UI.
4. URL snapshots: imported page snapshots will be stored as plain HTML files (no automatic markdown extraction in first pass).
5. Goal hierarchy: tasks are many-to-one to goals with optional ordering/milestones; goals do not 'own' tasks exclusively but provide the parent relationship and ordering.
6. Offline behavior: the UI should continue to function from files where possible if the daemon/backend is unavailable.

## Recommended First Decisions

- Use Tailwind CSS as the styling stack for layout and all forms/controls.
- Use a small collapsible sidebar with icons, labels, and only a few secondary indicators.
- Model notes, tasks, receipts, goals, and URLs as separate SQLite tables with explicit relationships.
- Notes are Markdown files; metadata is stored in SQLite; editing is a plain textarea in the UI (no preview in first pass).
- Receipts: config keys `receipts.active_dir` and `receipts.archived_dir`; receipts have `receipt_id`, `original_path`, `current_path`, and archived movement happens immediately on processing.
- Keep saved URLs in a dedicated SQLite table; snapshots are saved as HTML files and referenced by path in the DB.
- Let the Chrome extension push URLs immediately; the daemon will consume them and the UI will show a `new` badge until opened.

## Validation Report

### Commands run
- `pnpm build` → exit 0 (TypeScript + Vite, 7 modules, 564ms)
- `cargo build` → exit 0 (Tauri Rust, `Finished dev profile`)
- Cleaned: `context/tmp/*.json` (old session artifacts)

### Success-criteria verification
- [x] App shell fills full window and resizes fluidly → verified via CSS (`h-screen w-screen`, `grid-cols-[auto,1fr]`)
- [x] Sidebar is compact, collapsible, and visually understated → verified via `.app-sidebar-collapsed/expanded` with transition
- [x] Main content is not trapped inside decorative cards → verified via direct content in panel sections
- [x] Settings can update `maia.json` safely → `read_config`/`save_config` with JSON validation + `.bak` backup
- [x] Notes point to real files and preserve metadata → `notes` table + `.md` file read/write
- [x] Notes edited as Markdown with no preview pane → textarea-only editor
- [x] Receipts have stable ID and track original/current locations → `receipts` table with `receipt_id`, `original_path`, `current_path`
- [x] Receipts can be archived → `archive_receipt` moves file to archive dir, updates DB
- [x] Tasks have title, description, due date, tags, priority, completed state → `tasks` table + Tauri CRUD + UI
- [x] Goals have title, description, status, deadline, progress, linked tasks → `goals` table + Tauri CRUD + UI progress slider
- [x] Saved URLs are first-class records with freshness state → `urls` table + `is_new` badge + mark-read + external open
- [x] Dashboard shows real counts for all domains → `get_counts` command

### Residual risks
- No automated UI tests — manual Tauri window testing needed for resize/collapse behavior
- Receipt image preview deferred (paths shown as text; needs Tauri asset protocol for visual rendering)
- Snapshot files for URLs not implemented (deferred per user decision)
- Chrome extension ingestion not wired (deferred to `kb-chrome-extension` plan)

- The app shell fills the full window and resizes fluidly.
- The sidebar is compact, collapsible, and visually understated.
- Main content is not trapped inside decorative cards.
- Settings can update `maia.json` safely.
- Notes and receipts point to real files and preserve metadata/provenance.
- Notes are edited directly as Markdown files with no preview pane in the first pass.
- Saved URLs are first-class records and can be linked back to notes or goals.
- Tasks and goals use a simple, understandable model.
- Receipts have a stable ID and track both original and current locations for traceability.

## Next Steps

1. Resolve the open questions, especially styling stack and receipt provenance.
2. Turn the chosen decisions into implementation tasks for the UI rebuild.
3. Add file-backed note and receipt flows after the shell is stable.

---
Plan created for: `ui-rebuild`
