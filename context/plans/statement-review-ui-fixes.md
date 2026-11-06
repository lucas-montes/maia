# Statement Review — Fix File Grouping, Checkbox Alignment, Select-All

## Change summary
The `statements-file-review` plan shipped file-grouped review (one card per dropped file, nested
per-account rows, shared selection) per its own evidence section, but in actual use the statements list
shows no file grouping at all — just a flat list of entries, as if every statement fell into its own
singleton pseudo-group (`groupStatements`'s fallback key `~${id}` is used when `sourcePath` is
missing/falsy). Separately, the entry checkboxes render visually misaligned, and there is no
"select all" control at either level (the top-level list, or within one file's entry list) despite the
file-review page having what looks like a "Select pending" affordance already (`data-action=
"select-all"`) — it isn't doing what's expected, or isn't present where expected. This plan is a
bug-fix pass against `maia-ui/src/views/statements.ts` and whatever's actually feeding it, grounded in
the mismatch between what the prior plan's code shows on inspection and what you're actually seeing.

## Success criteria
- [ ] Root cause of the missing file grouping identified and fixed — confirmed by checking whether
      `sourcePath` actually reaches the client for a real multi-account CA file (not just reading the
      code and assuming it works): if the server is returning it correctly, the bug is in the frontend
      grouping/render path; if not, it's in the HTTP→desktop plumbing.
- [ ] Dropping/scanning a CA export with multiple accounts renders as **one** file card containing all
      of its accounts as nested rows (matching the original plan's intent), not one flat row per
      statement.
- [ ] Entry checkboxes (in the file review entries table) are visually aligned with their row content —
      no more offset/misaligned checkbox column.
- [ ] A "select all pending" control exists and works at both levels: selecting every pending entry
      within one file's review, and selecting every pending file/statement on the top-level list for
      bulk actions.
- [ ] `npx tsc --noEmit`, `npx vite build`, and a manual walkthrough (drop the committed CA fixture,
      confirm grouping/checkboxes/select-all) all pass.

## Constraints and non-goals
**Constraints:**
- Fix within the existing architecture (file-grouped list, shared file/account/entry checkbox model,
  `POST /statements/import`/`/delete` batch endpoints) — this is a bug fix, not a redesign of the
  review flow's structure.
- No server-side schema changes expected; the `sourcePath` plumbing already exists per the prior plan's
  evidence — first confirm whether it's actually reaching the client before assuming otherwise.

**Non-goals:**
- No new statement-review features beyond what the prior plan already scoped (no grouping by period,
  no per-entry delete, etc.) — this plan only fixes what's broken, not expands scope.
- No mobile changes — statement review is desktop-only, unchanged.

## Assumptions
- "Select all" is wanted at both levels (top-level list selects files/statements for bulk import/
  delete; within a file, select all pending entries for that file's own import) — confirmed.
- The regression (if that's what this is — grouping shipped once per the prior plan's evidence section)
  happened sometime after `61cd503`; worth checking `git log` on `statements.ts` and
  `handlers/statements.rs` since then for anything that touches `source_path`/`sourcePath` before
  diving into fresh debugging from scratch.

## Task stack (T01..T04)

- [ ] T01: Diagnose the missing file grouping (status:open)
  - Goal: Reproduce against a real multi-account statement file; inspect the actual HTTP response
    `GET /statements` returns (via the running desktop app, not just reading the handler source) to see
    whether `sourcePath` is present and non-null for those rows. If present and correct, the bug is
    client-side (`groupStatements` or how the fetched list is passed to it); if absent/null, trace why
    the value that `handlers/statements.rs` selects isn't surviving to the response the client parses.
  - Done when: the exact point where `sourcePath` is lost (or where grouping logic fails despite having
    it) is identified with a concrete repro.

- [ ] T02: Fix file grouping (status:open)
  - Goal: Whatever T01 finds, fix it so one dropped file's statements render as one group again.
  - Done when: the CA fixture (multiple accounts, one file) renders as a single file card with nested
    account rows, matching the prior plan's documented intent.

- [ ] T03: Fix checkbox alignment (status:open)
  - Goal: Correct the CSS/markup so entry-row checkboxes line up with their row content consistently
    (likely a `lineRow`/table-cell alignment fix in `statements.ts`).
  - Done when: checkboxes visually align across every row in the entries table, at the list and file
    levels.

- [ ] T04: Add working select-all at both levels (status:open)
  - Goal: Top-level list gets a control to select every pending file/statement at once; within a file's
    review, a control selects every pending entry (fixing or replacing whatever `data-action=
    "select-all"` currently does, if it's present but not working as expected).
  - Done when: one click selects everything pending at each level, and the existing per-row/per-account/
    per-file checkboxes stay in sync with it (indeterminate states still correct).

## Risks / gotchas
- Don't assume the root cause before T01 actually confirms it — the code reading as correct on
  inspection is exactly why this needs a real repro rather than a guessed fix; a wrong guess here could
  "fix" something that was never broken while leaving the actual bug in place.
- If the regression traces back to a specific commit after `61cd503`, check whether that commit touched
  anything else nearby that might carry the same bug (e.g. a shared serialization helper).
