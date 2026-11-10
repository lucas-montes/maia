# Statement Review — Fix File Grouping, Checkbox Alignment, Select-All

> **Superseded by `statement-intake-and-account-merge` (2026-11-09).**
>
> T01/T02 are answered: `sourcePath` *was* reaching the client (`src/handlers/statements.rs:103`,
> non-null on every row) and `groupStatements` *was* grouping. The list read as flat because the
> brokerage parsers split one file into one statement per month and the review UI renders one
> "account" row per statement — so a single Firstrade file filled one card with 34 rows, each
> offering to create an account. The real fix is in the parser, not the grouping code.
>
> T03 (checkbox alignment) and T04 (select-all) remain valid and are carried as T10 of the new plan.
> **They were implemented on 2026-11-10** — see Evidence below; the new plan's T10 is done with them.

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
- [x] Root cause of the missing file grouping identified and fixed — confirmed by checking whether
      `sourcePath` actually reaches the client for a real multi-account CA file (not just reading the
      code and assuming it works): if the server is returning it correctly, the bug is in the frontend
      grouping/render path; if not, it's in the HTTP→desktop plumbing.
- [ ] Dropping/scanning a CA export with multiple accounts renders as **one** file card containing all
      of its accounts as nested rows (matching the original plan's intent), not one flat row per
      statement.
- [x] Entry checkboxes (in the file review entries table) are visually aligned with their row content —
      no more offset/misaligned checkbox column.
- [x] A "select all pending" control exists and works at both levels: selecting every pending entry
      within one file's review, and selecting every pending file/statement on the top-level list for
      bulk actions.
- [x] `npx tsc --noEmit`, `npx vite build` pass; `npx vitest run` covers the selection rule.
      The manual walkthrough (drop the committed CA fixture) is listed under Remaining manual steps —
      it is worth doing together with the successor plan's T01, since one file still stages as many
      statements until then.

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

- [x] T01: Diagnose the missing file grouping (status:done — answered in `statement-intake-and-account-merge`)
  - Goal: Reproduce against a real multi-account statement file; inspect the actual HTTP response
    `GET /statements` returns (via the running desktop app, not just reading the handler source) to see
    whether `sourcePath` is present and non-null for those rows. If present and correct, the bug is
    client-side (`groupStatements` or how the fetched list is passed to it); if absent/null, trace why
    the value that `handlers/statements.rs` selects isn't surviving to the response the client parses.
  - Done when: the exact point where `sourcePath` is lost (or where grouping logic fails despite having
    it) is identified with a concrete repro.

- [ ] T02: Fix file grouping (status:superseded — nothing is wrong with the grouping; the parser
      split is, and the fix is T01 of `statement-intake-and-account-merge`)
  - Goal: Whatever T01 finds, fix it so one dropped file's statements render as one group again.
  - Done when: the CA fixture (multiple accounts, one file) renders as a single file card with nested
    account rows, matching the prior plan's documented intent.

- [x] T03: Fix checkbox alignment (status:done)
  - Goal: Correct the CSS/markup so entry-row checkboxes line up with their row content consistently
    (likely a `lineRow`/table-cell alignment fix in `statements.ts`).
  - Done when: checkboxes visually align across every row in the entries table, at the list and file
    levels.
  - Outcome: the entries table (`lineRow`) was already aligned; the two misaligned places were the
    file review's **accounts** table and the **list** rows. See Evidence for the measurements.

- [x] T04: Add working select-all at both levels (status:done)
  - Goal: Top-level list gets a control to select every pending file/statement at once; within a file's
    review, a control selects every pending entry (fixing or replacing whatever `data-action=
    "select-all"` currently does, if it's present but not working as expected).
  - Done when: one click selects everything pending at each level, and the existing per-row/per-account/
    per-file checkboxes stay in sync with it (indeterminate states still correct).
  - Outcome: both controls already existed (`select-all-statements` on the list, `select-all` in the
    file review) and the list one was correct. The file/detail one was not "not present" but
    *dishonest*: it selected every `pending` line, including lines the page renders without a
    checkbox, so `Import selected (N)` could exceed the ticks on screen. Selection now derives from
    one shared set that the render also consumes.

## Deviations from the plan
- **The entries table was not the misaligned one.** T03 guessed at `lineRow`; measuring showed its
  checkboxes sit within 1.5px of their row's first text line at every zoom level. The two real
  offenders were the file review's **accounts** table — its cells kept the daisyUI default
  `vertical-align: middle`, so a checkbox centred itself against a two-line cell and dropped ~9px
  (~25px zoomed) below the account name whenever an account number was shown — and the **list**
  rows, whose hand-tuned `mt-2` fell 5.8px short of the title line.
- **Select-all existed; it was lying, not missing.** The plan read the `data-action="select-all"`
  affordance as possibly broken-or-absent. It is present and wired at both levels. What was wrong is
  that `select-all` and the per-account checkbox worked off `line.status === "pending"` while the
  render decides a checkbox from something narrower, so they could select lines with no checkbox.
- **A shared set, not three filters.** Rather than repeat the render's condition in the handlers,
  `src/lib/statementReview.ts` computes the set once and `lineRow`/`transferRowsHtml` take it as an
  argument — so "what select-all picks" and "what the page shows as checkable" cannot drift apart.
- **Pending counts were left alone.** The header still shows the statements' real pending count, not
  the selectable count: the number is a fact about the data and must keep matching the list page.
  Only the select-all button's disabled state follows the selectable set.

## Evidence
- Layout measured in headless Chromium against the real built stylesheet
  (`dist/assets/index-*.css`), as Δ between the checkbox's centre and the centre of its row's first
  text line, at root font sizes 16/20/12px (the app's Ctrl+/- zoom):

  | row | before | after |
  |---|---|---|
  | accounts table, account **with** a number | +8.9 / +25.1 / +6.8 px | −0.5 / 0.0 / −0.5 px |
  | accounts table, account without a number | −0.5 px | −0.5 px |
  | list statement row | −5.8 / −6.6 / −4.5 px | −1.8 / −1.6 / −1.5 px |
  | entries table (`lineRow`) | −0.5 px | unchanged |

  The before/after screenshots show the same thing: the accounts column was ragged (one checkbox on
  its account name, the next sunk between name and number) and is now flush.
- `npx vitest run`: 33 passed (24 pre-existing + 9 new in `src/lib/statementReview.test.ts`).
- `npx tsc --noEmit` clean; `npx vite build` clean (136 kB css, 485 kB js).

## Remaining manual steps
1. Walk through the real app once: drop the CA fixture, confirm the accounts column is flush and
   that "Select pending" ticks exactly the boxes it claims.
2. Worth pairing with the successor plan's T01 — until the parsers stop splitting by month, a
   Firstrade/T212 file still fills the review with one row per month.

## Risks / gotchas
- Don't assume the root cause before T01 actually confirms it — the code reading as correct on
  inspection is exactly why this needs a real repro rather than a guessed fix; a wrong guess here could
  "fix" something that was never broken while leaving the actual bug in place.
- If the regression traces back to a specific commit after `61cd503`, check whether that commit touched
  anything else nearby that might carry the same bug (e.g. a shared serialization helper).
