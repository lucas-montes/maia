```markdown
# Tauri Desktop UI Plan (merged)

## Summary

Add a native desktop UI for Maia using the existing Tauri app in `maia-ui` with plain HTML. The plan covers the UI surface, navigation model (tabs), core screens, reusable components, and an explicit file-based workflow for notes and receipt images that integrates with Tauri file APIs and an AI parsing pipeline. The approach is incremental: UI shape → file access → parsing pipeline → packaging/CI.

## Motivation

- Provide a clear desktop home for Maia features (todos, receipts, notes, settings) using a lightweight Tauri shell.
- Keep the first version simple (vanilla HTML/TS/CSS) and iteratively add filesystem + parsing features.
- Reuse existing Rust crates and expose narrow, audited Tauri commands for file and parsing operations.

## Constraints & Decisions

- The UI remains plain HTML/TypeScript for now (no SPA framework rewrite).
- Use top-level tabs as the primary navigation surface; keyboard shortcuts and optional secondary navigation can be implemented later.
- File access and parsing are local-first: files live on disk, parsed JSON is saved locally, and backend sync is a subsequent phase.

## Tasks (T01..T12)

T01 — Map UI surface
- Goal: Define page structure: header, tab strip, main content area, and utility panels.
- Done: Layout map exists in `context/` and `maia-ui` code shows a single-shell layout (sidebar currently used; to be converted to a top tab strip in the UI implementation).
- Verify: Window shows tab strip and content areas during manual walkthrough.

T02 — Define navigation model (tabs)
- Goal: Make tabs the primary way users switch views (Dashboard, Todos, Receipts, Notes, Settings).
- Done: Documentation updated to reflect tabs as primary navigation.
- Verify: Implement tab strip in `maia-ui` and confirm keyboard shortcut support for switching.

T03 — List core screens
- Goal: Dashboard, Todos, Receipts, Notes/KB, Settings, and System/Status.
- Done: Each screen has an initial spec (content, actions, acceptance criteria).
- Verify: Each screen is reachable from the tab strip and renders basic content.

T04 — Reusable components
- Goal: Define cards, lists/tables, filters, forms, modals, toasts, empty/loading states, and badges.
- Verify: Components are reusable across at least two screens (e.g., cards on Dashboard and Settings).

T05 — Interactions & data flow
- Goal: Define rules for create/edit/delete/search/filter/refresh and clearly separate UI-local state from backend-backed sources of truth.
- Verify: Manual walkthrough validates optimistic vs. confirmed update rules.

T06 — File-based notes & receipts (F01..F08)
- Goal: Add file-based workflows where Notes are file references and Receipts are images in a configured directory with parsing.

- F01 — Notes: file references UI
	- Replace note content preview with file-rows showing path, size, mtime, and tags/metadata.
	- 'Open' action uses Tauri opener to launch the default OS handler.

- F02 — Receipts: image browser & viewer
	- Show thumbnails for images in `receipts_path` and a preview/detail pane that displays parsed metadata when present.

- F03 — Tauri FS & commands
	- Implement safe `tauri::command`s: `list_files`, `read_file_bytes`, `read_file_meta`, `write_json`, `open_path`, `update_maia_config`.
	- Commands validate and restrict operations to configured directories.

- F04 — AI parsing pipeline for receipts
	- Send images to `model_api` from `maia.json` (support HTTP POST and CLI backends), parse response to structured JSON, and save sidecar parsed files.

- F05 — Settings: edit `maia.json`
	- Read and write `maia.json` keys (`receipts_path`, `database`, `model_api`) with validation and automatic backups (`maia.json.bak`).

- F06 — UX: progress, toasts, undo/retry
	- Add progress indicators and toasts for parsing; support retry on failure.

- F07 — Security & permissions
	- Define allowlist/tauri.conf.json guidance, require confirmation for write operations, and restrict path access.

- F08 — Tests & verification
	- Manual verification steps and simple smoke tests for file read/write and parse flows.

T07 — Packaging & distribution
- Goal: Configure bundling for Linux (AppImage/.deb) and note macOS/Windows steps.
- Verify: Local bundle launches on a test machine.

T08 — CI: Build & release pipeline
- Goal: Add GitHub Actions to build and produce artifacts.

T09 — Security review
- Goal: Harden `tauri.conf.json`, set CSP, and document file access restrictions.

T10 — Docs & developer guide
- Goal: Add README and context docs describing how to extend file parsing and Tauri commands.

T11 — Validation & cleanup
- Goal: Smoke tests, tidy code, finalize `context/` artifacts, and create follow-ups.

T12 — Follow-up integration
- Goal: Wire parsed output to Maia backend and add sync behavior.

## Implementation notes & recommended Tauri commands

- Suggested Tauri commands (Rust-side):
	- `list_files(dir: String) -> Vec<FileEntry>`
	- `read_file_bytes(path: String) -> Vec<u8>`
	- `read_file_meta(path: String) -> FileMeta` (size, mtime, is_dir, ext)
	- `write_json(path: String, payload: String)`
	- `open_path(path: String)`
	- `update_maia_config(payload: serde_json::Value)` (writes `maia.json` with a backup)

- UI wiring:
	- Notes view calls `list_files(notes_path)` and renders rows with `Open` and metadata.
	- Receipts view calls `list_files(receipts_path)`, filters images, renders thumbnails, and reads full bytes for preview via `read_file_bytes`.
	- Parse flow: UI either sends bytes to `model_api` directly (frontend HTTP) or asks a Rust command to call the model endpoint and save the parsed JSON with `write_json`.

## Data layout on disk

- Sidecar parsed files: either `${image}.${ext}.parsed.json` or `${receipts_path}/.maia/parsed/<image>.json` (configurable).

## Security & mitigations

- Limit Tauri commands to configured directories and validate paths to avoid directory traversal.
- Confirm destructive actions and create backups before overwriting `maia.json`.
- For large images, generate thumbnails in Rust (byte-resize) rather than loading large images into the renderer.

## Acceptance criteria

- The tab-based UI surfaces work and navigate to each core screen.
- Notes view lists files with path/metadata and opens files with the OS default handler.
- Receipts view displays image thumbnails and a preview; parsing writes a JSON sidecar and UI shows parsed fields.
- Settings can update `maia.json` (with backup) and change `receipts_path` immediately after confirmation.

## Next steps

1. Implement T03/T02 tab change in the UI (convert sidebar to top tab strip) so tabs are the canonical nav.
2. Implement F01 & F02 (notes as file rows and receipts gallery) and stub the Rust commands (F03).
3. Implement F04 (parsing pipeline) behind a button/toggle and F05 (settings editor).
4. Add packaging (T06) and CI (T08) after the runtime flow is stable.

---
Plan created for: `tauri-desktop` (merged with `tauri-desktop-files`)
```
