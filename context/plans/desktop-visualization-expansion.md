# Desktop Visualization Expansion — Insights View

## Change summary
A new "Insights" screen in the desktop app, separate from the existing list screens, bringing
together the charts the roadmap asks for across three domains: exercise (per-exercise weight/rep
progression picker, plus an aggregate training-volume-over-time chart), nutrition (a calories chart
and a separate macros chart, each with their own day/week/month toggle), and finance (spending by
category per period, and account balance over time). Built on Chart.js, which is already a dependency
and already has one working chart (`bodyMetrics.ts`) to follow as the pattern — canvas + a render
function that updates `chart.data` in place rather than recreating the `Chart` instance.

## Success criteria
- [ ] A new `Insights` view/route in the desktop sidebar, independent of `meals.ts`/`workouts.ts`/
      `exercises.ts`/`transactions.ts`/`accounts.ts`, which keep their existing list-only UIs unchanged.
- [ ] Exercise section: an exercise picker rendering a weight/reps-over-time line chart for that
      exercise (pulling from `exercise_sets` joined through `workout_exercises`/`workouts`, using
      `actual_weight_kg`/`actual_reps` when present, falling back to planned `weight_kg`/`reps`), plus
      a separate aggregate volume chart (Σ `weight_kg * reps` per workout, bucketed by week/month) with
      its own period toggle.
  - [ ] Nutrition section: a calories-over-time chart and a macros-over-time chart (protein/carbs/fat as
      separate series), each independently toggleable between day/week/month, built from `meal_foods`
      joined to `meals.eaten_at`.
- [ ] Finance section: a spending-by-category chart (grouped/stacked bars, one group per period) and an
      account-balance-over-time line chart (opening balance + running sum of that account's
      transactions), each with its own period toggle.
- [ ] Every chart's period toggle is independent — switching one chart's granularity does not affect
      any other chart on the page.
- [ ] All data comes from direct SQL in new Tauri commands (matching the existing pattern — e.g. how
      `ingredient_prices`/`body_metrics` are queried directly from `maia-ui/src-tauri/src/lib.rs`
      rather than through the HTTP sync API), since the desktop app reads its local `maia.db` directly.
- [ ] `cargo check -p maia-ui` clean; manual walkthrough of each chart against seeded data confirms
      correct bucketing (a day/week/month toggle produces the expected number of buckets and values).

## Constraints and non-goals
**Constraints:**
- Reuse the `bodyMetrics.ts` chart pattern (canvas element, a `Chart` instance held in a closure
  variable, `renderChart()` updates `chart.data` rather than destroying/recreating the chart) for every
  new chart, for visual and code consistency.
- Exercise set values: prefer `actual_*` columns (what was actually done) over the planned `weight_kg`/
  `reps` when both are present — matches how the rest of the app already treats planned-vs-actual.
- Finance amounts use `amount_base` (already currency-converted), not raw `amount`, for cross-account/
  cross-currency aggregation — consistent with how statements/transactions already resolve currency.

**Non-goals:**
- No new chart types beyond line/bar — no heatmaps, no radar charts, nothing Chart.js doesn't give for
  free.
- No cross-filtering between charts (e.g. clicking a category bar does not filter the exercise chart).
- No mobile equivalent — this is desktop-only, matching how the existing `bodyMetrics.ts` chart is
  desktop-only too.
- Does not include the ingredient price-evolution chart or the food-cost estimates — those belong to
  the `ingredient-price-tracking` plan; if that plan's work lands first, its chart can simply be added
  as another Insights section later rather than duplicated here.

## Assumptions
- "Insights" is a new top-level sidebar entry (own route), not a tab bolted onto the dashboard — the
  dashboard stays the stat-tiles-plus-quick-actions view it is today.
- Account balance chart starts from `accounts.opening_balance` and walks transactions in date order;
  transfers (`to_account_id` set) adjust both accounts' running balances.
- "Spending by category" means expense-type transactions only (not income/transfers), grouped by the
  existing free-text `category` column, with an "Uncategorized" bucket for null/empty.

## Task stack (T01..T05)

- [ ] T01: Exercise — per-exercise progression picker (status:open)
  - Goal: Tauri command returning `(date, weight, reps)` series for a chosen exercise id, preferring
    `actual_*` columns; frontend exercise picker (likely in `exercises.ts` or the new Insights view)
    rendering a line chart.
  - Done when: picking an exercise with several logged sets across different dates renders a correct
    chronological progression line.

- [ ] T02: Exercise — aggregate training volume (status:open)
  - Goal: Tauri command bucketing Σ(`weight_kg * reps`) per workout by week/month; chart with its own
    period toggle.
  - Done when: switching the toggle re-buckets correctly (e.g. a month view collapses multiple weekly
    bars into one).

- [ ] T03: Nutrition — calories and macros charts (status:open)
  - Goal: Two Tauri commands (or one parameterized by metric) bucketing `meal_foods` sums by day/week/
    month via `meals.eaten_at`: one for calories, one returning protein/carbs/fat as separate series.
    Two charts, each with its own period toggle.
  - Done when: a day with multiple meals sums correctly; switching to week/month re-buckets as expected.

- [ ] T04: Finance — spending by category + balance over time (status:open)
  - Goal: Tauri commands for (a) expense totals grouped by category per period, using `amount_base`,
    and (b) per-account running balance over time from `opening_balance` + ordered transactions
    (transfers touching both accounts). Two charts, each with its own period toggle, account picker for
    the balance chart.
  - Done when: a manually-computed running balance for a seeded account matches the chart; category
    totals for a period match a hand-summed check.

- [ ] T05: Insights view assembly (status:open)
  - Goal: New sidebar route wiring T01-T04's sections together on one screen, each with its own
    independent period/exercise/account control.
  - Done when: navigating to Insights renders all sections without one chart's loading state blocking
    another's.

## Risks / gotchas
- Bucketing "by week" needs a consistent week-start convention (Monday vs Sunday) — pick one and apply
  it everywhere in this plan, since mixing conventions across charts would make periods look
  inconsistent even though each chart is independently correct.
- The balance-over-time chart's transfer handling (one transaction touching two accounts) is the one
  piece of real accounting logic here — get the sign/direction wrong and every balance after the first
  transfer is wrong. Worth a direct unit test in Rust against a couple of seeded accounts+transfers
  before wiring up the chart.
- Exercise sets with neither planned nor actual weight (e.g. bodyweight/cardio exercises) should be
  skipped from the weight-progression chart rather than plotted as zero.
