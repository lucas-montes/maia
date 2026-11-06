# Financial Scenario Simulations

## Change summary
A general scenario tool, desktop-only: derive a baseline (average monthly income and spend-by-category)
from actual transaction history, let the user override pieces of it per named, saved scenario —
replace a category's monthly cost (the relocation use case: "rent becomes €1200 instead of €2000"),
shift income, set an assumed annual inflation rate and an assumed annual investment return — and
project net worth forward over a chosen horizon. Multiple scenarios can be compared side by side as
overlaid lines on one chart, with a summary table of each scenario's ending numbers.

## Success criteria
- [ ] A new `finance_scenarios` table (name, assumptions as JSON, timestamps) persists named scenarios;
      CRUD via new Tauri commands.
- [ ] Baseline derivation: average monthly income and average monthly spend-by-category computed from
      actual `transactions` over a trailing window (default last 12 months, configurable), using
      `amount_base` for cross-currency consistency.
- [ ] A scenario's assumptions can override: any category's baseline monthly amount (add/replace), a
      monthly income delta, an annual inflation rate (compounds against expense categories not
      explicitly overridden), an annual investment return rate (compounds against the running net worth
      balance), and the projection horizon (months).
- [ ] A projection function runs the scenario month-by-month from current net worth (sum of account
      balances today) and produces a `(month, net_worth)` series.
- [ ] A new desktop "Simulations" screen: create/edit/delete named scenarios, a chart overlaying every
      selected scenario's projection line (plus a "no changes" baseline line for reference), and a
      summary table (ending net worth, total net saved, final monthly expense total) per scenario.
- [ ] `cargo test -p maia` covers the projection math directly (a scenario with known inputs produces a
      hand-computable net worth at month N) and the baseline derivation (seeded transactions produce the
      expected average income/category spend).

## Constraints and non-goals
**Constraints:**
- Desktop-only — this is a planning tool over the finance data the desktop app already owns; no mobile
  UI.
- Cost-of-living reference numbers (what rent/groceries/etc. actually cost in a target location) are
  always typed in by hand as category overrides — no external cost-of-living data source.
- Investment return is a single assumed annual rate applied to the whole running net worth balance —
  not asset-level, does not read the Trading 212 portfolio holdings.

**Non-goals:**
- No tax modeling, no currency-of-residence conversion beyond what `amount_base` already gives, no
  Monte Carlo / probabilistic projection — this is a simple deterministic compounding model, matching
  "simple compound growth rate."
- No automatic category override suggestions (e.g. "here's what rent typically costs in Portugal") —
  that would require the external data source explicitly ruled out.
- Does not touch the statement-import review flow (separate roadmap item / plan).

## Assumptions
- "Current net worth" for a projection's starting point is the sum of every account's current balance
  (opening balance + all its transactions to date), not a user-entered figure.
- Inflation (annual rate) compounds monthly (`rate/12`, applied each simulated month) against whichever
  expense categories were *not* explicitly overridden by the scenario — an overridden category is
  assumed to already reflect its target-context cost and is held flat unless the scenario also gives it
  its own growth, which is out of scope for a first pass.
- A category override replaces that category's baseline monthly amount outright (not an adjustment/delta
  on top of the baseline) — matches the relocation framing ("rent becomes X").
- Scenarios are a flat list (no folders/tags) — fine to revisit if the user ends up with many.

## Task stack (T01..T05)

- [ ] T01: Schema — `finance_scenarios` (status:open)
  - Goal: Migration adding `finance_scenarios(id, name, assumptions_json, created_at, updated_at,
    deleted_at)`. `assumptions_json` shape: `{horizonMonths, incomeDeltaMonthly, categoryOverrides:
    [{category, monthlyAmount}], inflationAnnualPct, investmentReturnAnnualPct}`.
  - Done when: migration applies cleanly; `cargo test -p maia` green.

- [ ] T02: Baseline derivation (status:open)
  - Goal: A function computing average monthly income and average monthly spend-by-category from
    `transactions` over a trailing window (default 12 months), using `amount_base`.
  - Done when: unit tests with seeded transactions across a few months and categories produce the
    expected per-category monthly averages.

- [ ] T03: Projection engine (status:open)
  - Goal: Given a baseline + a scenario's assumptions, run the month-by-month simulation (apply category
    overrides and income delta from month 1; compound non-overridden expense categories by inflation/12
    each month; compound the running net worth balance by investment return/12 each month) producing a
    `(month, net_worth)` series.
  - Done when: a hand-computed scenario (fixed income, one overridden category, no inflation, a round
    investment return) matches the function's output exactly for a few months.

- [ ] T04: Scenario CRUD + projection Tauri commands (status:open)
  - Goal: Create/list/update/delete scenario commands; a "project" command taking a scenario id (or ad
    hoc assumptions for an unsaved draft) and returning its projection series plus summary numbers.
  - Done when: creating, editing and deleting a scenario round-trips correctly; projecting a saved
    scenario matches T03's direct test.

- [ ] T05: Desktop Simulations screen (status:open)
  - Goal: New sidebar view: scenario list with create/edit/delete, a multi-select to choose which
    scenarios (plus the unmodified baseline) to overlay on one Chart.js line chart, and a summary table
    below it.
  - Done when: creating two scenarios (e.g. "stay" vs "move") and selecting both renders two distinct
    projection lines and a two-row summary table with sensible, distinct numbers.

## Risks / gotchas
- The "12 months trailing" baseline window is a guess at a sensible default — someone with less than a
  year of transaction history (or seasonal spending) could get a skewed baseline. Worth surfacing the
  window length in the UI rather than hiding it, so a thin baseline is visible rather than silently
  misleading.
- Compounding monthly at `annual_rate/12` is an approximation (not the exact monthly-equivalent rate of
  an annual rate) — fine for a rough planning tool, but worth a one-line comment in the code so nobody
  mistakes it for precise financial math.
- Category names are free text today (no fixed enum) — a baseline computed by exact string match on
  `category` will silently miss typos/variants ("Groceries" vs "groceries"). Not solving category
  normalization here, but worth a glance at how dirty the real data is before assuming exact-match
  grouping is good enough.
