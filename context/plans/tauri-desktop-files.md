
# File-based notes & receipts (Tauri)

## Summary

Add file-based notes and receipt image workflows to the Tauri desktop shell. Notes will be shown as file references (path + attributes) rather than storing full file contents in the UI. Receipts will be read from a user-configured directory, previewed as thumbnails and large images, and optionally sent to an AI parsing service which saves parsed JSON results next to each image.

This plan is scoped for an incremental implementation that keeps the UI local-first while preparing safe backend wiring.

## Goals

- Show notes as references to files (path, size, mtime, tags) in the Notes view.
- Display receipt images from a configured folder with thumbnails and a viewer panel.
- Provide a Tauri-backed file API (list/read/write) the UI calls to operate on note & receipt files.
- Implement an AI parsing flow: pick an image, send to model endpoint, receive structured JSON, and save the JSON to disk.
- Allow editing of `maia.json` (paths and `model_api`) from the Settings view and persist changes.

## Constraints & Decisions

- Keep the first pass local-only: files are read from disk, parsed results saved locally; syncing to the Maia backend is a later step.
- Use Tauri filesystem and dialog APIs for cross-platform file access and user confirmation.
- Store parsed JSON results adjacent to the image using a predictable filename: `<image>.<ext>.parsed.json` or in a `.maia/parsed` folder next to the configured receipts path.
- `model_api` from `maia.json` will be used as the service endpoint; authentication and secrets remain the user's responsibility and are not stored in plain text unless the user configures them.

## Tasks (F01..F08)

F01 — Notes: file references UI
- Goal: Replace note content preview with a compact file-row showing path, filesize, mtime, and optional tags/metadata.
- Boundaries: UI only; do not modify note files on disk in this task.
- Acceptance: Notes list shows file path, last-modified date, size, and an optional 'Open' action which calls the Tauri opener (or native handler).
- Verify: Place plain text files in the configured notes folder; the Notes view lists them correctly.

F02 — Receipts: image browser & viewer
- Goal: Show thumbnails for images in the configured receipts directory and a larger preview/detail pane with metadata (merchant/date/total if present from parsed JSON).
- Boundaries: Read-only viewer in this task; parsing/saving handled by F04.
- Acceptance: Clicking a thumbnail opens the image preview; large preview shows image and any existing parsed JSON metadata if present.
- Verify: Put JPG/PNG images into `receipts_path` and open the app; thumbnails and previews appear.

F03 — Tauri FS & commands
- Goal: Define and implement `tauri::command`s to: list files in a directory, read a file's bytes, read file metadata, write JSON files, and open a file in the OS.
- Boundaries: Commands should validate paths and restrict operations to configured directories.
- Acceptance: Frontend can call commands to list and read files and to write parsed JSON results.
- Verify: A small end-to-end smoke test: list files → read first file bytes → show image preview in UI.

F04 — AI parsing pipeline for receipts
- Goal: Implement a pipeline to send an image to the configured `model_api`, parse the response into a structured Receipt JSON schema, and save it next to the image.
- Boundaries: The plan should support both HTTP POST and local CLI backends; retry and error handling are required.
- Acceptance: After parsing completes, a JSON file appears in the receipts folder (or `.maia/parsed`), and the UI refreshes to show parsed fields (merchant, date, total, category, note).
- Verify: Provide a mocked endpoint or a dry-run with a stub response; parsing result file is written and shown in the UI.

F05 — Settings: edit `maia.json`
- Goal: Add a Settings control that reads and writes `maia.json` for keys: `receipts_path`, `database`, and `model_api` (and others as agreed).
- Boundaries: Validate JSON shape before writing and create a backup copy `maia.json.bak` before overwriting.
- Acceptance: Changing `receipts_path` updates the UI immediately (after user confirmation) and persists to `maia.json`.
- Verify: Edit `receipts_path` in settings, restart the UI (or refresh), and the new directory is used by file listings.

F06 — UX: actions, progress and undo
- Goal: Add user feedback (toasts/progress) for long-running parse operations, and an undo or retry flow for failed writes.
- Acceptance: Parsing shows a progress indicator and final success/error toast with retry link.

F07 — Security & permissions
- Goal: Define required Tauri allowlist entries and tauri.conf.json notes (CSP, file access). Plan for user prompts when granting folder access.
- Acceptance: A short checklist exists and the plan includes recommended tauri.conf.json changes (document-only; actual changes occur in implementation sessions).

F08 — Tests & verification
- Goal: Manual verification steps and a small automated smoke test (if feasible) for file reads, image preview, parse write, and settings changes.
- Acceptance: A checklist with steps that validate each earlier task.

## Implementation notes / API

- Tauri commands to expose (suggested names):
  - `list_files(dir: string) -> FileEntry[]` (name, path, size, mtime, isDir, extension)
  - `read_file_bytes(path: string) -> Uint8Array` (for images)
  - `write_json(path: string, payload: string)`
  - `open_path(path: string)` (native opener)
  - `update_maia_config(payload: object)` (writes `maia.json` with backup)

- UI behavior:
  - Notes view: call `list_files(notes_path)` and render rows containing `path`, `size`, `mtime`, and an `Open` button.
  - Receipts view: call `list_files(receipts_path)` filter images, generate thumbnails (client-side from bytes), show gallery, allow selecting one image and call `read_file_bytes` to display full-size.
  - Parse action: user clicks `Parse` on a receipt → frontend sends the image bytes to the `model_api` endpoint (or to a Tauri command that performs the network call) → on success save JSON via `write_json` and refresh UI.

## Data layout on disk

- Parsed JSON naming convention: `<original-filename>.parsed.json` or place under `${receipts_path}/.maia/parsed/<original-filename>.json`.
- Keep parsed JSON alongside images for discovery by the UI; allow settings to choose 'inline' vs 'sidecar folder' behavior.

## Risks & Mitigations

- Large images may use large memory when read into the renderer: create thumbnails server-side (Tauri command) or use low-res client-side previews.
- Exposing FS write operations is sensitive: always confirm destructive actions and limit paths to configured directories.
- Network parsing errors must be communicated clearly; support retries and local stubbing for dev.

## Next steps

1. Start with F01 (Notes UI) and F02 (Receipt viewer) so the UI surface reflects real file-based content.
2. Implement F03 (Tauri FS commands), then wire UI to them.
3. Implement F04 (AI parsing pipeline) behind a toggle or 'Parse' button.
4. Add F05 (Settings editor) and F06 (UX polish).

---
Plan created for: `tauri-desktop-files`
