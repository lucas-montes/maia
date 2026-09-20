# Maia UI — component catalog

Dark-only desktop system on Tailwind v4. Tokens live in `tokens.css`
(`@theme`); components below are the only approved building blocks for views.
Rules: no new colors outside the scales, every interactive element keeps a
visible focus ring, success stays silent (toast/banner, never `alert()` for
expected outcomes — `alert()`/`confirm()` remain only for destructive confirms
until views migrate to modal/toast).

## Tokens (`tokens.css`)

| Group | Names | Notes |
|---|---|---|
| Surfaces | `surface-950/900/800/700` | app bg → borders |
| Ink | `ink-50/100/300/400/500` | primary → faint |
| Accent | `accent-400/500/600` | links, selection, rings, fills |
| Status | `ok-400`, `warn-400`, `caution-400`, `err-400/500` | badges, banners |
| Radius | `maia-control` (0.25rem), `maia-card` (0.5rem), `maia-pill` | — |
| Motion | `maia-snappy`, `maia-fluid` (Open Props) | transitions only |
| Elevation | `maia-raised`, `maia-popover` (Open Props) | dialogs, toasts |

## Components (`tailwind.css` @layer components)

| Component | Classes | Variants / states |
|---|---|---|
| Card | `.panel`, `.panel-header`, `.panel-title`, `.panel-subtitle`, `.panel-content` | spacing via utilities (`space-y-*`) |
| Section header | `.section-header` | title + actions row |
| Buttons | `.button` (+`.secondary`, `.accent`, `.danger`), `.button-outline` (+`.active`) | hover, focus-visible, disabled |
| Inputs | native elements (base layer) | uniform 2.75rem height, dark select arrow + option list |
| Badges | `.badge` + `-muted/-ok/-warn/-err/-info` | replaces `receipt-status-*` in Phase 4 |
| List rows | `.list-row` (+`.active`) | replaces `todo-/receipt-/note-item` in Phase 4 |
| Tabs | `.tabs`, `.tab` (+`.active`) | — |
| Modal | `.modal-overlay` (+`.modal-open`), `.modal-dialog`, `.modal-title`, `.modal-actions` | per-view JS toggles `.modal-open` |
| Drawer | `.drawer-overlay` + `.drawer-panel` (+`.drawer-open`) | right sheet |
| Toasts | `.toast-stack`, `.toast` (+`-ok/-err`) | views append nodes; auto-dismiss in JS |
| Tooltip | `[data-tip]` | CSS-only, hover |
| Skeleton | `.skeleton` | with `animate-pulse`, sized by utilities |
| Forms | `.field-label`, `.field-hint` | — |
| Chips | `.chip` | macro pills, counts, tags |
| Banners | `.banner` (+`.banner-err/-ok`) | status notices incl. settings saved |
| Stats | `.stat` | dashboard metric cards |
| Divided lists | `.list-divided` | bordered divide containers, 7 sites (scroll stays inline) |
| Buttons | `.button.ok` | green primary CTA (ingredient flow) |
| Shell/sidebar | `.app-shell*` | unchanged |
| Datepicker | `.datepicker-*` | unchanged |
| Empty state | `.empty-state` | unchanged |

## Phase 4 migration notes

- Done: `main.ts` palette migrated to tokens (592 sites, 1:1 values);
  `receipt-status-*` removed in favor of `.badge-*`; all `alert()` call
  sites use `toast(err)`; both `confirm()` sites use `confirmDialog` modal
  (one intentional `confirm` fallback remains inside the helper).
- Done: dead-CSS sweep — `todo-*`, `receipt-*` rows, `note-item*` generation,
  `settings-*`/`status-*`/`action-group`/`toggle-group`, dashboard/activity
  helpers removed (zero references); stylesheet fully tokenized.
- Done: notes/receipts rows use `.list-row`; `note-list-*` deleted;
  template deletes use `.button.danger`.
- Done: ingredient modal uses `.modal-overlay/.modal-dialog/.modal-title/
  .modal-actions`; archived pill → `.badge-warn`; macro pills → `.chip`;
  dashboard cards → `.panel p-4`; exercise lists share one selected state.
- Done: dashboard cards unified to `.panel p-4` (11 sites); both exercise
  lists get a distinct selected state (`bg-surface-700/50` vs hover) and the
  standalone list an `.empty-state` fallback.
- Done: Plan view (task rows → `.list-row`, empty → `.empty-state`,
  redundant `panel-subtitle` utilities removed across 10 sites).
- Remaining: Diet (done except ingredient detail polish), Budget, Settings,
  FitFat dashboard/diagnostics structural passes.
- `panel-*` was phantom (undefined) before this catalog — views using it gain
  the card look with no markup changes.
