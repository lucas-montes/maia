```markdown
# Tauri Desktop App Plan

## Summary

Add a native desktop UI for Maia using Tauri. The desktop app will provide a lightweight, secure UI surface that integrates with existing Maia Rust crates and/or the running daemon. The plan is implementation-focused and staged to reduce risk: scaffold → integrate → UI → package → CI.

## Motivation

- Provide a familiar desktop client for Maia features (todos, receipts, notes).
- Reuse existing Rust code (`shared`, `ai`, etc.) where practical.
- Offer a cross-platform distribution path with small maintenance overhead.

## Constraints & Decisions

- Rust-first workspace (existing project is Rust-heavy). Dioxus is a strong candidate for front-end because the repo references Dioxus elsewhere. Alternatives: React/Vite or Svelte for web tech integration.
- Do not modify production daemon behaviour as part of scaffolding; integration should be additive.

## Tasks (T01..T10)

T01 — Scaffold Tauri app
- Goal: Create a `tauri` crate or workspace member with minimal app that compiles and opens a window.
- Boundaries: Keep app isolated; do not change core daemon yet.
- Done: `cargo tauri dev` builds and opens a placeholder window.
- Verify: Run dev command locally; window shows placeholder content.

T02 — Choose front-end framework
- Goal: Decide between Dioxus (Rust), React/Vite, or Svelte. Document trade-offs.
- Done: Decision documented in `context/` and scaffolding aligned.
- Verify: README updated and dev hot-reload confirmed for chosen stack.

T03 — Integrate with Maia backend
- Goal: Define integration approach: linking shared crates vs. IPC to running daemon (Unix socket). Prefer linking shared crates for tight integration; use socket fallback.
- Done: Example command that reads from DB or calls a shared function is implemented.
- Verify: UI command returns real data from the workspace DB/daemon.

T04 — Define IPC commands and API
- Goal: List `tauri::command`s and event names the UI will use; define JSON payload shapes.
- Done: Commands implemented as stubs in Rust and exposed to the front-end.
- Verify: Front-end triggers commands and receives expected responses.

T05 — Implement core UI screens
- Goal: Build minimal screens (Dashboard, Todos, Receipts, Settings) that call commands.
- Done: Screens render and display data from backend commands.
- Verify: Manual walkthrough shows navigation and data responses.

T06 — Packaging and distribution
- Goal: Configure bundling for Linux (AppImage/.deb) and note macOS/Windows steps.
- Done: Linux bundle produced locally.
- Verify: AppImage launches the app on test machine.

T07 — CI: Build & release pipeline
- Goal: Add GitHub Actions workflows to build and produce artifacts on push or tag.
- Done: Workflow runs and uploads build artifacts.
- Verify: Action completes on `main`/`master` and artifacts are downloadable.

T08 — Security & permissions review
- Goal: Harden `tauri.conf.json`, set CSP, limit file access, document recommended settings.
- Done: Settings applied and documented.
- Verify: Static audit checklist completed.

T09 — Docs and developer guide
- Goal: Add a `README` for the Tauri crate and update `context/` with developer steps.
- Done: Clear dev steps to run, package, and add IPC commands.
- Verify: A new contributor can follow the docs and run `cargo tauri dev` successfully.

T10 — Validation and cleanup
- Goal: Run smoke tests, tidy code, update `context/` artifacts, and create follow-up tickets.
- Done: Tests and manual validation pass; context files updated.
- Verify: All earlier done checks pass and CI succeeds.

## Dependencies

- `tauri` + bundler (platform toolchains for packaging).
- Optional: `dioxus` crates or front-end toolchain (Node/Vite) depending on choice.

## Risks & Mitigations

- Binary size and packaging complexity — mitigate by starting Linux-only and optimizing later.
- Integration complexity with the running daemon — start with a read-only command and iterate.

## Alternatives Considered

- Desktop via Electron: heavier and less aligned with Rust-first stack.
- Web dashboard served by embedded HTTP server: valid, but not native and would complicate local auth.

## Next Steps

- Start implementation (T01). After scaffolding, iterate T02–T05 in short cycles.
- When ready, proceed to packaging (T06) and CI (T07).

---
Plan created for: `tauri-desktop`.
```