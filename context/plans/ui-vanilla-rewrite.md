# Plan: ui-vanilla-rewrite

## Change summary

Replace the maia-ui frontend monolith (`maia-ui/src/main.ts`, 2,697 lines) with a
modular vanilla-TypeScript UI. The current UI re-renders the entire `#app` via
`innerHTML` at 72 call sites — including on every keystroke in search inputs,
with manual `focus()`/`setSelectionRange` hacks to survive the teardown — which
is why it feels slow and janky. The rewrite keeps TypeScript + Vite + Tailwind
v4 and adds **DaisyUI 5** (Tailwind plugin, CSS classes only — no JS framework,
per user decision), **Lucide** icons, and **nanostores** (1 KB state micro-lib)
for fine-grained updates. Big-bang cutover on a branch; master keeps the old UI
until the final task.

The Rust/Tauri command layer (`src-tauri`), the sync-server HTTP contract, and
the Nix packaging layout are untouched; only the rendering layer changes. The
pending one-line `grid-cols-[auto,1fr]` → `grid-cols-[auto_1fr]` fix in
`src/styles/tailwind.css` is subsumed: that file is replaced wholesale in T01.

### Design reference (inspiration)

- **Layout inspiration: https://html-dashboard.daisyui.com/** — the official
  DaisyUI dashboard example. Borrowed patterns: grouped sidebar nav with
  badges/counts and a collapsible drawer; topbar with search and profile menu;
  a row of `stat` cards for overview metrics (dashboard); a responsive grid of
  `panel`-style cards mixing charts, tables, and activity lists; `badge` status
  chips; `table` with row hover for record lists; `modal`/`toast` overlays.
- **Component docs: https://daisyui.com/docs/install/vite/** — install is
  `@plugin "daisyui";` in the Tailwind v4 CSS entry (already using
  `@tailwindcss/vite`).
- Theme: custom DaisyUI theme mapped from the existing `tokens.css`
  surface/ink/accent palette (dark), registered via `@plugin "daisyui/theme"`.

## Acceptance criteria

- [x] AC1: Interactions never re-render the app shell — the sidebar DOM node
      keeps its identity across view switches and input events; only the bound
      list/section re-renders on keystrokes.
  - Validate: code inspection (no `appRoot.innerHTML` full-app render outside
    bootstrap) + manual `pnpm tauri dev` walkthrough (type in a search input,
    switch views, confirm sidebar state and focus survive).
- [x] AC2: All 21 views are feature-par with the old UI: dashboard, exercises,
      workouts, templates (+sets CRUD), ingredients, foods (editor + archive),
      stores, meals, body metrics (chart), experiments (+heatmap), todos, goals
      (+progress slider), notes (list + editor), receipts (list + detail +
      archive/restore), urls (open/mark-read), sync (QR + info), tags, accounts,
      transactions (+search), fxRates (add/refresh), settings (config editor).
  - Validate: manual walkthrough of each view against
    `git show master:maia-ui/src/main.ts` as the behavioral reference.
- [x] AC3: The frontend builds clean with the new stack.
  - Validate: `cd maia-ui && pnpm build` (tsc + vite) exits 0 and emits `dist/`.
- [x] AC4: The Rust side is untouched and green.
  - Validate: `cargo test -p maia-ui` in `maia-ui/src-tauri` passes (incl. the
    custom-protocol and read-only-connection tests).
- [x] AC5: The packaged desktop app builds with the new frontend.
  - Validate: `nix build .#maia-desktop --no-link` in `server/` succeeds.
- [x] AC6: The monolith is gone — `main.ts` is replaced by the module tree and
      the dead `src/styles.css` is deleted.
  - Validate: `ls maia-ui/src` shows the module tree; no 2,000+ line file;
    `grep -c "data-action" maia-ui/src` shows delegation replaced by per-view
    handlers.

### Full validation

- `cd maia-ui && pnpm build`
- `cargo test -p maia-ui` (from `maia-ui/src-tauri`)
- `nix build .#maia-desktop --no-link` and `nix flake check --no-build` (in `server/`)

### Context sync

- `context/ui-rebuild/shell-and-screens.md` — rewrite to describe the new shell,
  module structure, and DaisyUI/nanostores stack (the old description documents
  the monolith).
- `context/glossary.md` — add DaisyUI, Lucide, nanostores entries.
- `context/context-map.md` — refresh the ui-rebuild row.

## Task context synchronization lifecycle

- **Task context synchronization:** every task carries `pending | synced | blocked`.
  A completed task must be `synced` before another task can start or the plan can
  finish.
- For `blocked`, record **Blocker**, **Required action**, and **Retry condition**
  beside the status. Never infer `synced` from conversation history; write every
  lifecycle transition to the plan file.

## Constraints and non-goals

- **In scope:** `maia-ui/src/**` (new module tree), `maia-ui/index.html`,
  `maia-ui/package.json` + `pnpm-lock.yaml` (new deps), `maia-ui/vite.config.ts`
  (unchanged unless the build demands it), `server/nix/packages.nix`
  (`frontend.passthru.pnpmDepsHash` update when the lockfile changes).
- **Out of scope:** any `src-tauri/**` Rust change, any `sync-server` change,
  `tauri.conf.json` keys other than none (frontendDist already `../dist`),
  NixOS module, new features beyond parity.
- **Constraints:** no JS framework (user decision); Tauri v2 `invoke` contract
  frozen; `pnpm build` must stay the build command (Nix + tauri.conf depend on
  it); keep Chart.js 4 for charts; keep `qrcode` for the sync QR.
- **Non-goal:** rewriting the backend, redesigning the data model, or adding
  views that don't exist today.

## Assumptions

- nanostores (plus a small hand-rolled `bind`/`mount` helper over template
  literals) counts as "not a framework" — it is a 1 KB state micro-lib, and it
  is the mechanism that removes the full-app re-render without adopting one.
- Lucide via the `lucide` package's `createIcons` on `data-lucide` attributes
  (vanilla-friendly), tree-shaken per icon.
- The old `main.ts` on master is the behavioral reference for parity; no
  spec exists beyond it and `context/ui-rebuild/shell-and-screens.md`.
- Work happens on branch `ui/rewrite-vanilla`; master stays on the old UI until
  T07 cutover.
- The uncommitted `grid-cols` fix in the working tree is folded into T01's
  stylesheet replacement rather than committed separately.

## Task stack

- [x] T01: `Scaffold DaisyUI shell, router, and typed api layer` (status:done)
  - Task ID: T01
  - Scope: In — package.json deps (daisyui, lucide, nanostores), Tailwind v4
    CSS entry with `@plugin "daisyui"` + custom theme from tokens.css, Lucide
    setup, `lib/api.ts` typed wrappers for all ~50 invoke commands, app shell
    (permanently-mounted sidebar with grouped nav + counts, topbar, main
    outlet) per the html-dashboard.daisyui.com layout, hash router with
    mount/unmount per route, bootstrap in `main.ts`. Update
    `nix/packages.nix` `pnpmDepsHash` for the new lockfile. Out — individual
    views (T02–T05), polish (T06), monolith deletion (T07).
  - Dependencies: none
  - Done when: `pnpm build` green; `pnpm tauri dev` shows the new shell with
    sidebar + empty main area; navigating routes swaps only the main outlet;
    `nix build .#frontend --no-link` succeeds with the updated hash.
  - Verify: `pnpm build`; `nix build .#frontend --no-link`; manual route
    switching in `pnpm tauri dev`.
  - Context synchronization: pending

- [x] T02: `Port core write views (todos, goals, notes, urls, settings)` (status:done)
  - Task ID: T02
  - Scope: In — route modules for todos (CRUD, filters, priority badges,
    datepicker), goals (CRUD, status badges, progress slider), notes (list +
    markdown editor, file-backed), urls (create/open/mark-read/delete, new
    badge), settings (config form editor with backup semantics). Out —
    receipts/dashboard (T03), read-only data views (T04).
  - Dependencies: T01
  - Done when: each view's CRUD flows work against the real Tauri commands and
    match old-UI behavior; no full-shell re-render during any interaction.
  - Verify: manual CRUD walkthrough per view in `pnpm tauri dev`.
  - Context synchronization: pending

- [x] T03: `Port receipts and dashboard views` (status:done)
  - Task ID: T03
  - Scope: In — receipts (list with status badges, detail pane with paths and
    timestamps, archive/restore), dashboard (get_counts stat cards, quick
    actions, recent activity) using DaisyUI `stat`/`badge`/`table` per the
    dashboard example. Out — training/nutrition views (T04).
  - Dependencies: T02
  - Done when: receipts archive/restore round-trips and dashboard counts match
    `get_counts`; layout follows the stat-cards + panels-grid pattern.
  - Verify: manual walkthrough; archive → restore → re-archive a receipt.
  - Context synchronization: pending

- [x] T04: `Port training and nutrition views` (status:done)
  - Task ID: T04
  - Scope: In — exercises (list + media), workouts (read-only list),
    templates (CRUD + exercise sets), ingredients (CRUD), foods (editor with
    ingredient draft rows + archive), stores (CRUD), meals (read-only with
    macro summary). Out — charts/health/finance (T05).
  - Dependencies: T02
  - Done when: all eight views are feature-par, including the foods editor's
    draft-row editing without focus loss.
  - Verify: manual walkthrough per view; foods editor keystroke test (no
    re-render focus jump).
  - Context synchronization: pending

- [x] T05: `Port health, finance, and system views` (status:done)
  - Task ID: T05
  - Scope: In — body metrics (Chart.js canvas + list), experiments (list +
    heatmap), tags, accounts, transactions (+search filter), fxRates
    (add/refresh), sync (sync_info + QR code). Out — polish (T06).
  - Dependencies: T04
  - Done when: all seven views are feature-par; chart renders from
    list_body_metrics; QR renders from sync_info payload.
  - Verify: manual walkthrough per view.
  - Context synchronization: pending

- [x] T06: `Add overlays, shortcuts, transitions, and lazy media` (status:done)
  - Task ID: T06
  - Scope: In — DaisyUI toast/modal components replacing the hand-rolled
    overlay roots, keyboard shortcut parity (Escape semantics, nav shortcuts),
    View Transitions API for route changes, IntersectionObserver lazy media
    for exercise images/videos and receipt images, virtualized list for long
    tables. Out — any view logic changes beyond wiring overlays.
  - Dependencies: T05
  - Done when: toasts/modals render through DaisyUI components; shortcuts from
    the old UI work; media loads lazily; route changes animate.
  - Verify: manual interaction pass; `pnpm build` green.
  - Context synchronization: pending

- [x] T07: `Cut over: delete monolith and verify packaged build` (status:done)
  - Task ID: T07
  - Scope: In — delete old `main.ts` implementation remnants and dead
    `src/styles.css`, ensure `index.html` entry is final, run the full
    validation suite, merge branch to master. Out — new features.
  - Dependencies: T06
  - Done when: AC1–AC6 all hold on master; old monolith absent from the tree.
  - Verify: full validation commands (see Acceptance criteria).
  - Context synchronization: pending

## Open questions

- None. The stack, strategy, and visual direction were decided with the user
  (no framework, big-bang, DaisyUI 5 + Lucide); the dashboard example at
  https://html-dashboard.daisyui.com/ is the layout reference.
