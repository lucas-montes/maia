# Ingredient Price Tracking — Receipt Matching, Desktop History, Cost-per-Nutrient

## Change summary
Close the loop between receipts and ingredient prices, and bring the price-tracking feature mobile
already has to desktop. On receipt review (desktop), each parsed line item can be matched to an
existing ingredient (always manual — no auto-matching), creating an `ingredient_prices` row with a
store (upserted by merchant name) and a package weight entered at match time. The desktop ingredient
detail view gains the price history + evolution chart mobile already has (which the desktop Tauri
command currently doesn't even select `package_grams` for). On top of that, add the two new numbers
the roadmap is actually after: a period-aggregated price-per-100g per ingredient (total paid ÷ total
grams eaten in a week/month/year, matched through `meals → meal_foods → food_ingredients`), and a
derived price-per-gram-of-nutrient (protein/carbs/fat) computed from that against the ingredient's
existing per-100g nutrition facts. That period price-per-100g also drives a per-meal cost estimate
(nearest period at-or-before the meal's date) shown on logged meals, and an aggregate food-spend
rollup by day/week/month.

## Success criteria
- [ ] Desktop receipt detail view lists every parsed line item (not just a count) with a "match to
      ingredient" action; matching inserts an `ingredient_prices` row (ingredient, store, price,
      currency, package_grams, recorded_at = receipt date) and upserts the store by the receipt's
      `merchant` name.
- [ ] Desktop ingredient detail view shows the price history list (store, price, package size,
      cost-per-100g/kg, date) and a line chart of cost-per-100g over time — parity with what mobile's
      `_PricesSection` already does, via a Tauri command that actually selects `package_grams` (today's
      doesn't).
- [ ] `ingredient_prices` gains an optional `source_url` column; settable from both mobile's existing
      price-entry sheet and the new desktop match flow. Never auto-fetched.
- [ ] A period-aggregation query computes, per ingredient per chosen period (week/month/year):
      `total_paid / total_grams_eaten * 100`, where `total_grams_eaten` sums
      `food_ingredients.amount * (meal_foods.amount / batch_weight(food))` across meals in that period,
      and `total_paid` sums matched `ingredient_prices.price` (converted to base currency) for receipts
      dated in that period.
- [ ] For ingredients with a period price-per-100g available, a price-per-gram figure is derivable for
      protein/carbs/fat/sodium/fiber/sugar (`price_per_100g / nutrient_per_100g`), shown somewhere a
      user can compare ingredients (e.g. a sortable list: cheapest protein source this month).
- [ ] Each logged meal in the desktop diet log shows an estimated cost, computed by summing
      `grams_of_ingredient_in_meal * period_price_per_100g / 100` across its ingredients, using the
      most recent period at-or-before the meal's date that has a price for that ingredient (skip
      ingredients with no price data yet — no estimate, not a zero).
- [ ] A day/week/month aggregate "estimated food spend" rolls up those per-meal costs, next to the
      existing calorie/nutrient graphs (or wherever plan B's visualization work lands it).
- [ ] `cargo test -p maia` and `flutter analyze`/mobile tests stay green; new Rust unit tests cover the
      apportionment math (food → ingredient grams) and the period aggregation query on a few synthetic
      rows.

## Constraints and non-goals
**Constraints:**
- Matching parsed receipt items to ingredients is always manual — no fuzzy auto-matching, no
  auto-creation of unmatched items.
- No automated price fetching from `source_url` — manual entry/paste only.
- `ingredient_prices` already has `UNIQUE(ingredient_id, store_id, recorded_at)` — one price per
  ingredient/store/day; a receipt matched on a day that already has a price for that ingredient+store
  overwrites it (existing mobile behavior, carried over to desktop).
- Cost estimates use eaten-grams as a direct proxy for bought-grams within a period — buying more than
  is eaten (waste, stockpiling) is not modeled. This is accepted per the user's own framing of the
  feature, not something to solve here.

**Non-goals:**
- No change to the receipt OCR schema (`ParsedItem` stays name/price/currency) — quantity/weight is
  never parsed from the receipt image; it's entered by hand at match time.
- No mobile UI changes beyond exposing `source_url` in the existing price-entry sheet — the mobile
  price-tracking feature is otherwise already complete.
- No retroactive backfill tooling for receipts already parsed before this ships; matching is only
  offered going forward from the receipt detail view (a user can still open an old receipt and match it
  manually, this just isn't automated).

## Assumptions
- `source_url` lives on `ingredient_prices` (per price observation), not on `ingredients` — a price
  found online is tied to the specific observation, not a permanent attribute of the ingredient. Flag
  if you meant otherwise.
- "Total grams eaten" for the period aggregation uses the full meal-food apportionment chain
  (`meal_foods.amount` of a food, apportioned through `food_ingredients` by batch-weight fraction), not
  just foods that are a single raw ingredient — most logged meals are recipes (foods), not raw
  ingredients directly.
- Currency conversion for "total paid" reuses the existing `fx_rates`/base-currency machinery already
  used by transactions/statements, converting each `ingredient_prices.price` to `base_currency` before
  summing across a period.
- UI surface is desktop-only, per your answer — mobile keeps its existing (already complete)
  price-entry feature untouched except for the new `source_url` field.

## Task stack (T01..T07)

- [ ] T01: Schema — `source_url` column (status:open)
  - Goal: Guarded `ALTER TABLE ingredient_prices ADD COLUMN source_url TEXT` migration (append-only
    `MIGRATIONS`, bump `LATEST_VERSION`); include in push/pull payloads for `ingredient_prices`.
  - Done when: migration applies cleanly on an existing v-whatever db; `cargo test -p maia` green.

- [ ] T02: Desktop — receipt line-item review + match-to-ingredient (status:open)
  - Goal: Tauri command to list a receipt's parsed items; a "match" command that upserts the store by
    merchant name and inserts an `ingredient_prices` row (ingredient id, store id, price, currency,
    package_grams entered in the match dialog, recorded_at = receipt date, source_url optional).
    Frontend: per-item row in `receipts.ts`'s detail view with a match action/dialog (ingredient
    picker + package-grams input).
  - Done when: matching a receipt item creates the expected `ingredient_prices` row; re-matching the
    same item on the same day updates it (respects the existing UNIQUE constraint).

- [ ] T03: Desktop — ingredient price history + evolution chart (status:open)
  - Goal: Fix the existing Tauri command to also select `package_grams`/`source_url`; add a price
    history list (store, price, package size, derived cost-per-100g/kg) and a line chart of
    cost-per-100g over time to the ingredient detail view — the desktop equivalent of mobile's
    `_PricesSection`.
  - Done when: an ingredient with several price observations across different package sizes renders a
    correct, comparable cost-per-100g trend line.

- [ ] T04: Period price-per-100g aggregation (status:open)
  - Goal: A query/function computing, per ingredient and a given period (week/month/year), total grams
    eaten (via the food→ingredient apportionment chain) and total paid (via matched `ingredient_prices`
    in that period, converted to base currency), yielding price-per-100g. Expose via a Tauri command
    parameterized by period.
  - Done when: unit tests with synthetic meals/foods/food_ingredients/ingredient_prices rows produce the
    expected price-per-100g for a few hand-computed cases, including a food that blends multiple
    ingredients.

- [ ] T05: Price-per-nutrient-gram derivation + comparison view (status:open)
  - Goal: Derive price-per-gram for protein/carbs/fat/sodium/fiber/sugar from T04's price-per-100g and
    the ingredient's existing per-100g nutrition columns; a desktop view listing ingredients sorted by
    price-per-gram-of-protein (or whichever nutrient is selected) for the chosen period.
  - Done when: the comparison list correctly ranks a handful of ingredients with known prices/nutrition
    by cost-efficiency for a selected nutrient.

- [ ] T06: Per-meal cost estimate + aggregate food-spend rollup (status:open)
  - Goal: For each logged meal, estimate cost by summing `grams_of_ingredient * period_price_per_100g /
    100` across its ingredients (nearest period at-or-before the meal date with a price; skip
    ingredients with none). Show the estimate on the meal in the diet log. Add a day/week/month
    aggregate "estimated food spend" view.
  - Done when: a meal built from ingredients with known period prices shows a cost matching manual
    hand-calculation; a meal with a wholly-unpriced ingredient still shows a partial/best-effort
    estimate (or an explicit "incomplete" indicator) rather than silently showing zero.

- [ ] T07: Mobile — expose `source_url` in the existing price-entry sheet (status:open)
  - Goal: Add an optional URL field to `_PriceSheet` in `ingredient_detail_screen.dart`, wired through
    `IngredientPrice`/push/pull like the rest of the model.
  - Done when: a URL entered on mobile round-trips through sync and is visible on desktop's history
    list (T03).

## Risks / gotchas
- The apportionment chain (meal → food → food_ingredients → ingredient grams) touches a batch-weight
  derivation that's currently implicit (sum of `food_ingredients.amount`, minus `cooked_weight` when
  set) — get this wrong and every downstream price-per-100g/per-meal number is subtly off. Worth a
  dedicated, well-commented helper with direct unit tests before anything else in T04-T06 is built on it.
- Currency conversion for historical prices needs the `fx_rates` snapshot for the *price's* date, not
  today's rate — reuse the existing statement-import snapshot-lookup pattern (`fx_rates` fallback to
  previous snapshot), don't re-derive it.
- `UNIQUE(ingredient_id, store_id, recorded_at)` means two receipts from the same store on the same day
  for the same ingredient collide — the second match silently overwrites the first's price. Acceptable
  per existing mobile behavior, but worth a line in desktop's match UI if it's not already obvious there.
