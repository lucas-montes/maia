//! Ingredient price tracking: period price-per-100g, price-per-nutrient-gram,
//! and the per-meal/aggregate cost estimates built on top of them.
//!
//! Everything here reads through the `meals -> meal_foods -> foods ->
//! food_ingredients -> ingredients` chain to turn "a food was logged" into
//! "this many grams of this raw ingredient were eaten". Getting that
//! conversion wrong would make every number downstream subtly off, so it is
//! isolated in [`food_batch_weight`] and [`grams_of_ingredient_in_meal_food`]
//! and tested directly, independent of the aggregation built on top.
//!
//! Cost estimates use eaten-grams in a period as a direct proxy for
//! bought-grams in that period. Buying more than is eaten (waste,
//! stockpiling) is not modeled -- that is an accepted simplification, not a
//! bug.

use rusqlite::{Connection, OptionalExtension, params};

/// A bucket of calendar time price-per-100g (and spend) is aggregated over.
///
/// `Day` is only used for spend-rollup bucketing (T06); period
/// price-per-100g (T04) is always Week/Month/Year.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Day,
    Week,
    Month,
    Year,
}

impl Period {
    /// The `[start, end)` epoch-millisecond window, in UTC, containing `at_ms`.
    pub fn bounds(self, at_ms: i64) -> (i64, i64) {
        use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};

        let dt = DateTime::<Utc>::from_timestamp_millis(at_ms).unwrap_or_else(|| Utc.timestamp_millis_opt(0).unwrap());
        let date = dt.date_naive();
        let (start_date, end_date) = match self {
            Period::Day => (date, date + Duration::days(1)),
            Period::Week => {
                // ISO week: Monday 00:00 through the following Monday.
                let back = date.weekday().num_days_from_monday() as i64;
                let start = date - Duration::days(back);
                (start, start + Duration::days(7))
            }
            Period::Month => {
                let start = date.with_day(1).unwrap_or(date);
                let end = if start.month() == 12 {
                    chrono::NaiveDate::from_ymd_opt(start.year() + 1, 1, 1).unwrap()
                } else {
                    chrono::NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1).unwrap()
                };
                (start, end)
            }
            Period::Year => {
                let start = chrono::NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap();
                let end = chrono::NaiveDate::from_ymd_opt(date.year() + 1, 1, 1).unwrap();
                (start, end)
            }
        };
        let to_ms = |d: chrono::NaiveDate| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
        (to_ms(start_date), to_ms(end_date))
    }

    /// The window of the same kind immediately before `bounds`.
    pub fn previous(self, bounds: (i64, i64)) -> (i64, i64) {
        self.bounds(bounds.0 - 1)
    }
}

/// The batch weight a food's per-100g figures are derived from: the user's
/// entered `cooked_weight` when set and positive, else the raw sum of its
/// components' `amount`. `None` when neither gives a usable (positive)
/// weight -- a food with no composition, or a zero/negative sum.
///
/// This mirrors the comment above `food_ingredients` and `foods.cooked_weight`
/// in `db.rs`: cooking moves water in or out of the batch without changing its
/// calories, so a client's per-100g math uses the cooked weight as the
/// denominator when known, and the raw sum otherwise.
pub fn food_batch_weight(conn: &Connection, food_id: &str) -> rusqlite::Result<Option<f64>> {
    let cooked: Option<f64> = conn
        .query_row(
            "SELECT cooked_weight FROM foods WHERE id = ?1 AND deleted_at IS NULL",
            params![food_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if let Some(cooked) = cooked {
        if cooked > 0.0 {
            return Ok(Some(cooked));
        }
    }
    let sum: f64 = conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM food_ingredients WHERE food_id = ?1 AND deleted_at IS NULL",
        params![food_id],
        |r| r.get(0),
    )?;
    Ok(if sum > 0.0 { Some(sum) } else { None })
}

/// Grams of one ingredient contributed by a single logged `meal_foods` row.
///
/// `meal_amount` is how many grams of the *food* were eaten
/// (`meal_foods.amount`); `ingredient_amount` is that ingredient's share of
/// the food's raw composition (`food_ingredients.amount`); `batch_weight` is
/// [`food_batch_weight`]'s result for that food. Pure and separately tested
/// from the DB-touching apportionment below, since this is the arithmetic the
/// whole feature stands on.
pub fn grams_of_ingredient_in_meal_food(meal_amount: f64, ingredient_amount: f64, batch_weight: f64) -> f64 {
    if batch_weight <= 0.0 {
        return 0.0;
    }
    meal_amount * (ingredient_amount / batch_weight)
}

/// Every ingredient a food is built from, with its grams-per-unit-batch
/// fraction already resolved against [`food_batch_weight`]. `None` when the
/// food has no usable batch weight (see `food_batch_weight`).
fn food_components(conn: &Connection, food_id: &str) -> rusqlite::Result<Option<Vec<(String, f64)>>> {
    let Some(batch) = food_batch_weight(conn, food_id)? else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(
        "SELECT ingredient_id, amount FROM food_ingredients WHERE food_id = ?1 AND deleted_at IS NULL",
    )?;
    let rows: Vec<(String, f64)> = stmt
        .query_map(params![food_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(Result::ok)
        .collect();
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        rows.into_iter().map(|(ingredient_id, amount)| (ingredient_id, amount / batch)).collect(),
    ))
}

/// Total grams of `ingredient_id` eaten across every logged meal in
/// `[start_ms, end_ms)`, via `meals -> meal_foods -> foods -> food_ingredients`.
///
/// A food with no resolvable batch weight contributes nothing for that one
/// `meal_foods` row rather than erroring the whole sum -- the same
/// "best-effort, not all-or-nothing" choice `resolve_food_nutrition` makes on
/// the desktop side.
pub fn total_grams_eaten(conn: &Connection, ingredient_id: &str, start_ms: i64, end_ms: i64) -> rusqlite::Result<f64> {
    let mut stmt = conn.prepare(
        "SELECT mf.amount, fi.amount, mf.food_id
         FROM meal_foods mf
         JOIN meals m ON m.id = mf.meal_id
         JOIN food_ingredients fi ON fi.food_id = mf.food_id AND fi.ingredient_id = ?1
         WHERE mf.deleted_at IS NULL AND fi.deleted_at IS NULL AND m.deleted_at IS NULL
           AND m.eaten_at >= ?2 AND m.eaten_at < ?3",
    )?;
    let rows: Vec<(f64, f64, String)> = stmt
        .query_map(params![ingredient_id, start_ms, end_ms], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(Result::ok)
        .collect();

    let mut total = 0.0;
    for (meal_amount, ingredient_amount, food_id) in rows {
        if let Some(batch) = food_batch_weight(conn, &food_id)? {
            total += grams_of_ingredient_in_meal_food(meal_amount, ingredient_amount, batch);
        }
    }
    Ok(total)
}

/// The `code`/`base` conversion rate for a price observed at `at_ms`: the
/// latest `fx_rates` snapshot on or before that date, falling back further
/// back in time as needed (never forward -- a 2026-01 price is never
/// converted at a 2026-06 rate). `None`, not an error, when nothing is found:
/// callers treat an unconvertible price as "skip it", not "fail the whole
/// aggregation".
///
/// Deliberately a small duplicate of `statements::store::rate_for` rather than
/// a shared call: that one takes a `Transaction` and bails on a missing rate,
/// which is the right behaviour for an import but not for a best-effort
/// aggregate read.
pub fn rate_to_base(conn: &Connection, code: &str, base: &str, at_ms: i64) -> Option<f64> {
    if code.eq_ignore_ascii_case(base) {
        return Some(1.0);
    }
    let date = chrono::DateTime::from_timestamp_millis(at_ms)?.format("%Y-%m-%d").to_string();
    conn.query_row(
        "SELECT rate_to_base FROM fx_rates
         WHERE code = ?1 AND base_code = ?2 AND rate_date <= ?3 AND deleted_at IS NULL
         ORDER BY rate_date DESC LIMIT 1",
        params![code, base, date],
        |r| r.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

/// Total paid for `ingredient_id`, converted to `base_currency`, across every
/// `ingredient_prices` observation recorded in `[start_ms, end_ms)`.
///
/// An observation whose currency has no `fx_rates` snapshot at-or-before its
/// date is skipped rather than failing the sum -- see [`rate_to_base`].
fn total_paid_base(
    conn: &Connection,
    ingredient_id: &str,
    base_currency: &str,
    start_ms: i64,
    end_ms: i64,
) -> rusqlite::Result<f64> {
    let mut stmt = conn.prepare(
        "SELECT price, currency_code, recorded_at FROM ingredient_prices
         WHERE ingredient_id = ?1 AND deleted_at IS NULL
           AND recorded_at >= ?2 AND recorded_at < ?3",
    )?;
    let rows: Vec<(f64, String, i64)> = stmt
        .query_map(params![ingredient_id, start_ms, end_ms], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(Result::ok)
        .collect();

    let mut total = 0.0;
    for (price, currency, recorded_at) in rows {
        if let Some(rate) = rate_to_base(conn, &currency, base_currency, recorded_at) {
            total += price * rate;
        }
    }
    Ok(total)
}

/// Price-per-100g for `ingredient_id` over the period containing `at_ms`:
/// `total_paid / total_grams_eaten * 100`. `None` when either side of that
/// ratio is unavailable -- no grams eaten (nothing to divide by), or no paid
/// total (no price observations in the period, or none convertible) -- so a
/// caller never mistakes "no data" for a confident zero.
pub fn price_per_100g_for_period(
    conn: &Connection,
    ingredient_id: &str,
    base_currency: &str,
    period: Period,
    at_ms: i64,
) -> rusqlite::Result<Option<f64>> {
    let (start, end) = period.bounds(at_ms);
    price_per_100g_in_bounds(conn, ingredient_id, base_currency, (start, end))
}

fn price_per_100g_in_bounds(
    conn: &Connection,
    ingredient_id: &str,
    base_currency: &str,
    (start, end): (i64, i64),
) -> rusqlite::Result<Option<f64>> {
    let grams = total_grams_eaten(conn, ingredient_id, start, end)?;
    if grams <= 0.0 {
        return Ok(None);
    }
    let paid = total_paid_base(conn, ingredient_id, base_currency, start, end)?;
    if paid <= 0.0 {
        return Ok(None);
    }
    Ok(Some(paid / grams * 100.0))
}

/// [`price_per_100g_for_period`], searching backwards through up to
/// `max_lookback` periods at-or-before `at_ms` for the most recent one that
/// has a price. This is what powers the per-meal cost estimate: "the most
/// recent period at-or-before the meal's date that has a price for that
/// ingredient" (never a later period, which would be using a price the user
/// could not have known yet).
pub fn nearest_period_price(
    conn: &Connection,
    ingredient_id: &str,
    base_currency: &str,
    period: Period,
    at_ms: i64,
    max_lookback: u32,
) -> rusqlite::Result<Option<f64>> {
    let mut bounds = period.bounds(at_ms);
    for _ in 0..=max_lookback {
        if let Some(price) = price_per_100g_in_bounds(conn, ingredient_id, base_currency, bounds)? {
            return Ok(Some(price));
        }
        bounds = period.previous(bounds);
    }
    Ok(None)
}

/// Per-100g price of one gram of a nutrient, derived from a price-per-100g and
/// the ingredient's own per-100g nutrition figure: `price_per_100g /
/// nutrient_per_100g`. `None` when the nutrient figure is missing or zero
/// (division is meaningless -- "costs nothing per gram of a nutrient this
/// food doesn't have" is not a real answer).
pub fn price_per_nutrient_gram(price_per_100g: f64, nutrient_per_100g: Option<f64>) -> Option<f64> {
    let nutrient = nutrient_per_100g?;
    if nutrient <= 0.0 {
        return None;
    }
    Some(price_per_100g / nutrient)
}

/// How many months back [`nearest_period_price`] searches for the per-meal
/// cost estimate. Three years covers "I haven't bought this in a while"
/// without an unbounded scan over every month since the ingredient existed.
const MEAL_COST_LOOKBACK_MONTHS: u32 = 36;

/// A meal's estimated cost, in `base_currency`.
#[derive(Debug, Clone, PartialEq)]
pub struct MealCostEstimate {
    pub total_base: f64,
    /// False when at least one logged ingredient/food had no priced period to
    /// draw from, so the total is a partial sum, not the whole meal.
    pub complete: bool,
}

/// Estimates a meal's cost by summing, over every ingredient in every food it
/// logged, `grams_eaten * nearest_month_price_per_100g / 100`.
///
/// `None` when not a single ingredient in the meal could be priced at all --
/// showing "incomplete" is only useful once there is a partial number to
/// qualify; a meal built entirely from unpriced ingredients has nothing to
/// show instead of a misleading zero. `complete` on the `Some` case tells the
/// caller whether every ingredient contributed, vs. a best-effort partial sum.
pub fn meal_cost_estimate(conn: &Connection, meal_id: &str, base_currency: &str) -> rusqlite::Result<Option<MealCostEstimate>> {
    let eaten_at: Option<i64> = conn
        .query_row(
            "SELECT eaten_at FROM meals WHERE id = ?1 AND deleted_at IS NULL",
            params![meal_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(eaten_at) = eaten_at else {
        return Ok(None);
    };

    let mut stmt = conn.prepare(
        "SELECT food_id, amount FROM meal_foods WHERE meal_id = ?1 AND deleted_at IS NULL",
    )?;
    let foods: Vec<(String, f64)> = stmt
        .query_map(params![meal_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(Result::ok)
        .collect();

    let mut total = 0.0;
    let mut any_priced = false;
    let mut complete = true;

    for (food_id, meal_amount) in foods {
        let Some(components) = food_components(conn, &food_id)? else {
            complete = false;
            continue;
        };
        for (ingredient_id, fraction) in components {
            let grams = meal_amount * fraction;
            match nearest_period_price(conn, &ingredient_id, base_currency, Period::Month, eaten_at, MEAL_COST_LOOKBACK_MONTHS)? {
                Some(price_per_100g) => {
                    total += grams * price_per_100g / 100.0;
                    any_priced = true;
                }
                None => complete = false,
            }
        }
    }

    if !any_priced {
        return Ok(None);
    }
    Ok(Some(MealCostEstimate { total_base: total, complete }))
}

/// One bucket of the food-spend rollup: all meals whose `eaten_at` falls in
/// the same `bucket` window, summed via [`meal_cost_estimate`].
#[derive(Debug, Clone, PartialEq)]
pub struct SpendBucket {
    pub bucket_start_ms: i64,
    pub total_base: f64,
    /// False when at least one meal in the bucket had an incomplete or
    /// entirely-missing cost estimate.
    pub complete: bool,
}

/// Buckets every logged meal in `[since_ms, until_ms)` by `bucket` and sums
/// [`meal_cost_estimate`] within each. Meals with no priced ingredient at all
/// are omitted from their bucket's sum (nothing to add) but still mark it
/// incomplete, so "this week cost 12 EUR" next to a silently-dropped unpriced
/// meal doesn't read as the whole picture.
pub fn food_spend_rollup(
    conn: &Connection,
    base_currency: &str,
    bucket: Period,
    since_ms: i64,
    until_ms: i64,
) -> rusqlite::Result<Vec<SpendBucket>> {
    let mut stmt = conn.prepare(
        "SELECT id, eaten_at FROM meals
         WHERE deleted_at IS NULL AND eaten_at >= ?1 AND eaten_at < ?2
         ORDER BY eaten_at",
    )?;
    let meals: Vec<(String, i64)> = stmt
        .query_map(params![since_ms, until_ms], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(Result::ok)
        .collect();

    let mut buckets: std::collections::BTreeMap<i64, SpendBucket> = std::collections::BTreeMap::new();
    for (meal_id, eaten_at) in meals {
        let (bucket_start, _) = bucket.bounds(eaten_at);
        let entry = buckets.entry(bucket_start).or_insert(SpendBucket {
            bucket_start_ms: bucket_start,
            total_base: 0.0,
            complete: true,
        });
        match meal_cost_estimate(conn, &meal_id, base_currency)? {
            Some(estimate) => {
                entry.total_base += estimate.total_base;
                entry.complete = entry.complete && estimate.complete;
            }
            None => entry.complete = false,
        }
    }
    Ok(buckets.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use chrono::Datelike;

    fn seed_ingredient(conn: &Connection, id: &str, cal: f64, protein: f64) {
        conn.execute(
            "INSERT INTO ingredients (id, name, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at)
             VALUES (?1, ?1, ?2, ?3, 0, 0, 1, 1)",
            params![id, cal, protein],
        )
        .unwrap();
    }

    fn seed_food(conn: &Connection, id: &str, cooked_weight: Option<f64>, components: &[(&str, f64)]) {
        conn.execute(
            "INSERT INTO foods (id, name, created_at, updated_at, cooked_weight) VALUES (?1, ?1, 1, 1, ?2)",
            params![id, cooked_weight],
        )
        .unwrap();
        for (ingredient_id, amount) in components {
            conn.execute(
                "INSERT INTO food_ingredients (food_id, ingredient_id, amount, updated_at) VALUES (?1, ?2, ?3, 1)",
                params![id, ingredient_id, amount],
            )
            .unwrap();
        }
    }

    fn seed_meal(conn: &Connection, id: &str, eaten_at: i64, foods: &[(&str, f64)]) {
        conn.execute(
            "INSERT INTO meals (id, name, eaten_at, created_at, updated_at) VALUES (?1, ?1, ?2, 1, 1)",
            params![id, eaten_at],
        )
        .unwrap();
        for (i, (food_id, amount)) in foods.iter().enumerate() {
            let mf_id = format!("{id}-mf-{i}");
            conn.execute(
                "INSERT INTO meal_foods (id, meal_id, food_id, amount, calories, protein, carbs, fat, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 0, 0, 0, 0, 1)",
                params![mf_id, id, food_id, amount],
            )
            .unwrap();
        }
    }

    fn seed_price(conn: &Connection, id: &str, ingredient_id: &str, price: f64, currency: &str, recorded_at: i64) {
        conn.execute(
            "INSERT OR IGNORE INTO stores (id, name, created_at, updated_at) VALUES ('s1', 'Store', 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ingredient_prices (id, ingredient_id, store_id, price, currency_code, recorded_at, created_at, updated_at)
             VALUES (?1, ?2, 's1', ?3, ?4, ?5, 1, 1)",
            params![id, ingredient_id, price, currency, recorded_at],
        )
        .unwrap();
    }

    fn ms(y: i32, m: u32, d: u32) -> i64 {
        chrono::NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis()
    }

    // --- Period bounds ------------------------------------------------------

    #[test]
    fn day_bounds_are_midnight_to_midnight_utc() {
        let (start, end) = Period::Day.bounds(ms(2026, 3, 15));
        assert_eq!(start, ms(2026, 3, 15) - 12 * 3600 * 1000);
        assert_eq!(end - start, 24 * 3600 * 1000);
    }

    #[test]
    fn week_bounds_start_on_monday() {
        // 2026-03-15 is a Sunday.
        let (start, end) = Period::Week.bounds(ms(2026, 3, 15));
        let start_date = chrono::DateTime::from_timestamp_millis(start).unwrap().date_naive();
        assert_eq!(start_date.weekday(), chrono::Weekday::Mon);
        assert_eq!(end - start, 7 * 24 * 3600 * 1000);
        assert!(start <= ms(2026, 3, 15) && ms(2026, 3, 15) < end);
    }

    #[test]
    fn month_bounds_span_the_calendar_month_including_december() {
        let (start, end) = Period::Month.bounds(ms(2026, 2, 10));
        assert_eq!(start, ms(2026, 2, 1) - 12 * 3600 * 1000);
        assert_eq!(end, ms(2026, 3, 1) - 12 * 3600 * 1000);

        let (dec_start, dec_end) = Period::Month.bounds(ms(2026, 12, 25));
        assert_eq!(dec_start, ms(2026, 12, 1) - 12 * 3600 * 1000);
        assert_eq!(dec_end, ms(2027, 1, 1) - 12 * 3600 * 1000);
    }

    #[test]
    fn year_bounds_span_the_calendar_year() {
        let (start, end) = Period::Year.bounds(ms(2026, 7, 4));
        assert_eq!(start, ms(2026, 1, 1) - 12 * 3600 * 1000);
        assert_eq!(end, ms(2027, 1, 1) - 12 * 3600 * 1000);
    }

    #[test]
    fn previous_steps_back_one_full_window_of_the_same_kind() {
        let month = Period::Month.bounds(ms(2026, 3, 10));
        let prev = Period::Month.previous(month);
        assert_eq!(prev, Period::Month.bounds(ms(2026, 2, 10)));

        let year = Period::Year.bounds(ms(2026, 3, 10));
        let prev_year = Period::Year.previous(year);
        assert_eq!(prev_year, Period::Year.bounds(ms(2025, 3, 10)));
    }

    // --- Batch weight / apportionment ---------------------------------------

    #[test]
    fn batch_weight_prefers_cooked_weight_over_raw_sum() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_ingredient(&conn, "water", 0.0, 0.0);
        seed_food(&conn, "dough", Some(180.0), &[("flour", 100.0), ("water", 100.0)]);
        assert_eq!(food_batch_weight(&conn, "dough").unwrap(), Some(180.0));
    }

    #[test]
    fn batch_weight_falls_back_to_raw_component_sum() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_ingredient(&conn, "water", 0.0, 0.0);
        seed_food(&conn, "dough", None, &[("flour", 100.0), ("water", 50.0)]);
        assert_eq!(food_batch_weight(&conn, "dough").unwrap(), Some(150.0));
    }

    #[test]
    fn batch_weight_is_none_without_any_usable_weight() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_food(&conn, "empty", None, &[]);
        assert_eq!(food_batch_weight(&conn, "empty").unwrap(), None);
    }

    #[test]
    fn grams_in_meal_food_is_the_meal_amount_times_ingredient_share() {
        // 200g eaten of a 150g-batch recipe that is 50g oil out of 150g total:
        // 200 * (50 / 150) = 66.67g of oil eaten.
        let got = grams_of_ingredient_in_meal_food(200.0, 50.0, 150.0);
        assert!((got - 66.666_666_666_666_66).abs() < 1e-9, "{got}");
        assert_eq!(grams_of_ingredient_in_meal_food(200.0, 50.0, 0.0), 0.0);
    }

    #[test]
    fn total_grams_eaten_sums_across_meals_and_blends_multiple_ingredients() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_ingredient(&conn, "oil", 884.0, 0.0);
        // 150g batch: 100g flour + 50g oil.
        seed_food(&conn, "dough", None, &[("flour", 100.0), ("oil", 50.0)]);
        let jan = ms(2026, 1, 10);
        let feb = ms(2026, 2, 10);
        seed_meal(&conn, "m1", jan, &[("dough", 300.0)]); // 200g flour, 100g oil
        seed_meal(&conn, "m2", jan, &[("dough", 150.0)]); // 100g flour, 50g oil
        seed_meal(&conn, "m3", feb, &[("dough", 300.0)]); // outside January

        let (start, end) = Period::Month.bounds(jan);
        assert_eq!(total_grams_eaten(&conn, "flour", start, end).unwrap(), 300.0);
        assert_eq!(total_grams_eaten(&conn, "oil", start, end).unwrap(), 150.0);
    }

    #[test]
    fn total_grams_eaten_ignores_deleted_meals_and_components() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_food(&conn, "dough", None, &[("flour", 100.0)]);
        let jan = ms(2026, 1, 10);
        seed_meal(&conn, "m1", jan, &[("dough", 100.0)]);
        conn.execute("UPDATE meals SET deleted_at = 1 WHERE id = 'm1'", []).unwrap();
        seed_meal(&conn, "m2", jan, &[("dough", 100.0)]);

        let (start, end) = Period::Month.bounds(jan);
        // Only m2 counts: m1 is soft-deleted.
        assert_eq!(total_grams_eaten(&conn, "flour", start, end).unwrap(), 100.0);
    }

    // --- Period price-per-100g -----------------------------------------------

    #[test]
    fn price_per_100g_divides_total_paid_by_total_grams_eaten() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_food(&conn, "bag", None, &[("flour", 1000.0)]); // 1kg bag, batch weight = itself
        let jan = ms(2026, 1, 10);
        seed_meal(&conn, "m1", jan, &[("bag", 500.0)]); // ate 500g of flour
        seed_price(&conn, "p1", "flour", 2.0, "EUR", jan); // paid 2 EUR

        let price = price_per_100g_for_period(&conn, "flour", "EUR", Period::Month, jan).unwrap();
        // 2 EUR / 500g * 100 = 0.40 EUR per 100g.
        assert_eq!(price, Some(0.40));
    }

    #[test]
    fn price_per_100g_blends_a_food_made_of_multiple_ingredients() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_ingredient(&conn, "oil", 884.0, 0.0);
        // 150g batch: 2/3 flour, 1/3 oil.
        seed_food(&conn, "dough", None, &[("flour", 100.0), ("oil", 50.0)]);
        let jan = ms(2026, 1, 10);
        seed_meal(&conn, "m1", jan, &[("dough", 300.0)]); // 200g flour, 100g oil
        seed_price(&conn, "pf", "flour", 1.0, "EUR", jan);
        seed_price(&conn, "po", "oil", 2.0, "EUR", jan);

        let flour_price = price_per_100g_for_period(&conn, "flour", "EUR", Period::Month, jan).unwrap();
        let oil_price = price_per_100g_for_period(&conn, "oil", "EUR", Period::Month, jan).unwrap();
        assert_eq!(flour_price, Some(0.5)); // 1 EUR / 200g * 100
        assert_eq!(oil_price, Some(2.0)); // 2 EUR / 100g * 100
    }

    #[test]
    fn price_per_100g_converts_through_fx_rates_at_the_prices_own_date() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_food(&conn, "bag", None, &[("flour", 1000.0)]);
        let jan5 = ms(2026, 1, 5);
        let jan10 = ms(2026, 1, 10);
        conn.execute(
            "INSERT INTO fx_rates (code, base_code, rate_date, rate_to_base, updated_at) VALUES ('USD','EUR','2026-01-01',0.9,1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fx_rates (code, base_code, rate_date, rate_to_base, updated_at) VALUES ('USD','EUR','2026-01-20',0.5,1)",
            [],
        )
        .unwrap();
        seed_meal(&conn, "m1", jan10, &[("bag", 100.0)]); // 100g eaten
        seed_price(&conn, "p1", "flour", 10.0, "USD", jan5); // 10 USD on Jan 5

        let price = price_per_100g_for_period(&conn, "flour", "EUR", Period::Month, jan10).unwrap();
        // Must use the 2026-01-01 snapshot (0.9), not the later 2026-01-20 one:
        // 10 USD * 0.9 = 9 EUR for 100g eaten -> 9 EUR per 100g.
        assert_eq!(price, Some(9.0));
    }

    #[test]
    fn price_per_100g_is_none_without_grams_eaten_or_without_a_price() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_food(&conn, "bag", None, &[("flour", 1000.0)]);
        let jan = ms(2026, 1, 10);
        // No meals logged at all: no grams eaten.
        seed_price(&conn, "p1", "flour", 2.0, "EUR", jan);
        assert_eq!(price_per_100g_for_period(&conn, "flour", "EUR", Period::Month, jan).unwrap(), None);

        // Grams eaten but no price in the period.
        seed_meal(&conn, "m1", jan, &[("bag", 500.0)]);
        conn.execute("DELETE FROM ingredient_prices", []).unwrap();
        assert_eq!(price_per_100g_for_period(&conn, "flour", "EUR", Period::Month, jan).unwrap(), None);
    }

    // --- Price-per-nutrient-gram ---------------------------------------------

    #[test]
    fn price_per_nutrient_gram_divides_by_the_nutrient_figure() {
        // 0.40 EUR/100g, 10g protein/100g -> 0.04 EUR per gram of protein.
        assert_eq!(price_per_nutrient_gram(0.40, Some(10.0)), Some(0.04));
    }

    #[test]
    fn price_per_nutrient_gram_is_none_for_missing_or_zero_nutrient() {
        assert_eq!(price_per_nutrient_gram(0.40, None), None);
        assert_eq!(price_per_nutrient_gram(0.40, Some(0.0)), None);
    }

    // --- Per-meal cost estimate -----------------------------------------------

    #[test]
    fn meal_cost_matches_hand_computation_when_every_ingredient_is_priced() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_ingredient(&conn, "oil", 884.0, 0.0);
        seed_food(&conn, "dough", None, &[("flour", 100.0), ("oil", 50.0)]); // 150g batch
        // Priming month (December): a single-ingredient "bag" food eaten in
        // an amount equal to its own batch weight, so that month's
        // price-per-100g equals the paid price exactly -- and, being a
        // separate month from the meal being costed, the meal's own
        // consumption cannot feed back into the price it is charged.
        let dec = ms(2025, 12, 10);
        seed_food(&conn, "flour-bag", None, &[("flour", 100.0)]);
        seed_food(&conn, "oil-bag", None, &[("oil", 100.0)]);
        seed_meal(&conn, "priming", dec, &[("flour-bag", 100.0), ("oil-bag", 100.0)]);
        seed_price(&conn, "pf", "flour", 1.0, "EUR", dec); // 1 EUR/100g
        seed_price(&conn, "po", "oil", 2.0, "EUR", dec); // 2 EUR/100g

        // Logged meal in January: 300g of dough = 200g flour + 100g oil.
        let jan = ms(2026, 1, 10);
        seed_meal(&conn, "lunch", jan, &[("dough", 300.0)]);

        let estimate = meal_cost_estimate(&conn, "lunch", "EUR").unwrap().unwrap();
        // 200g flour @ 1 EUR/100g = 2.0; 100g oil @ 2 EUR/100g = 2.0; total 4.0.
        assert!((estimate.total_base - 4.0).abs() < 1e-9, "{estimate:?}");
        assert!(estimate.complete);
    }

    #[test]
    fn meal_cost_is_a_partial_best_effort_sum_when_one_ingredient_is_unpriced() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_ingredient(&conn, "mystery", 100.0, 0.0);
        seed_food(&conn, "dough", None, &[("flour", 100.0), ("mystery", 50.0)]);
        // Priming in December, same reasoning as above: kept out of the
        // costed meal's own month so its consumption cannot change the price
        // it is being charged.
        let dec = ms(2025, 12, 10);
        seed_food(&conn, "flour-bag", None, &[("flour", 100.0)]);
        seed_meal(&conn, "priming", dec, &[("flour-bag", 100.0)]);
        seed_price(&conn, "pf", "flour", 1.0, "EUR", dec); // flour priced, mystery is not

        let jan = ms(2026, 1, 10);
        seed_meal(&conn, "lunch", jan, &[("dough", 150.0)]); // 100g flour, 50g mystery

        let estimate = meal_cost_estimate(&conn, "lunch", "EUR").unwrap().unwrap();
        assert!((estimate.total_base - 1.0).abs() < 1e-9, "{estimate:?}"); // only flour's 1 EUR
        assert!(!estimate.complete);
    }

    #[test]
    fn meal_cost_is_none_when_nothing_in_it_has_ever_been_priced() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "mystery", 100.0, 0.0);
        seed_food(&conn, "dough", None, &[("mystery", 100.0)]);
        let jan = ms(2026, 1, 10);
        seed_meal(&conn, "lunch", jan, &[("dough", 100.0)]);

        assert_eq!(meal_cost_estimate(&conn, "lunch", "EUR").unwrap(), None);
    }

    #[test]
    fn meal_cost_uses_the_nearest_earlier_priced_month_not_a_later_one() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_food(&conn, "flour-bag", None, &[("flour", 100.0)]);
        // Priced in January (1 EUR/100g) and again in March (3 EUR/100g).
        seed_meal(&conn, "jan-prime", ms(2026, 1, 10), &[("flour-bag", 100.0)]);
        seed_price(&conn, "pjan", "flour", 1.0, "EUR", ms(2026, 1, 10));
        seed_meal(&conn, "mar-prime", ms(2026, 3, 10), &[("flour-bag", 100.0)]);
        seed_price(&conn, "pmar", "flour", 3.0, "EUR", ms(2026, 3, 10));

        // A meal logged in February must use January's price, never March's.
        seed_meal(&conn, "feb-lunch", ms(2026, 2, 15), &[("flour-bag", 100.0)]);
        let estimate = meal_cost_estimate(&conn, "feb-lunch", "EUR").unwrap().unwrap();
        assert!((estimate.total_base - 1.0).abs() < 1e-9, "{estimate:?}");
    }

    // --- Food-spend rollup ----------------------------------------------------

    #[test]
    fn food_spend_rollup_buckets_by_day_and_sums_meal_estimates() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "flour", 350.0, 10.0);
        seed_food(&conn, "flour-bag", None, &[("flour", 100.0)]);
        let day1 = ms(2026, 1, 10);
        let day2 = ms(2026, 1, 11);
        seed_price(&conn, "p1", "flour", 1.0, "EUR", day1); // one 1 EUR observation this month

        seed_meal(&conn, "m1", day1, &[("flour-bag", 100.0)]);
        seed_meal(&conn, "m2", day1, &[("flour-bag", 200.0)]);
        seed_meal(&conn, "m3", day2, &[("flour-bag", 100.0)]);
        // The meal cost estimate's price-per-100g is a *month* aggregate: 1
        // EUR paid over 400g eaten this month (100+200+100) = 0.25 EUR/100g,
        // applied to each meal's own grams -- not the day-level grouping the
        // rollup displays at.
        let day1_bucket_grams = 100.0 + 200.0;
        let day2_bucket_grams = 100.0;
        let price_per_100g = 1.0 / 400.0 * 100.0;

        let (start, end) = Period::Month.bounds(day1);
        let buckets = food_spend_rollup(&conn, "EUR", Period::Day, start, end).unwrap();

        let day1_bucket = buckets.iter().find(|b| b.bucket_start_ms == Period::Day.bounds(day1).0).unwrap();
        assert!(
            (day1_bucket.total_base - day1_bucket_grams * price_per_100g / 100.0).abs() < 1e-9,
            "{day1_bucket:?}"
        );
        let day2_bucket = buckets.iter().find(|b| b.bucket_start_ms == Period::Day.bounds(day2).0).unwrap();
        assert!(
            (day2_bucket.total_base - day2_bucket_grams * price_per_100g / 100.0).abs() < 1e-9,
            "{day2_bucket:?}"
        );
    }

    #[test]
    fn food_spend_rollup_marks_a_bucket_incomplete_when_a_meal_has_no_price() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        seed_ingredient(&conn, "mystery", 100.0, 0.0);
        seed_food(&conn, "dough", None, &[("mystery", 100.0)]);
        let day = ms(2026, 1, 10);
        seed_meal(&conn, "m1", day, &[("dough", 100.0)]);

        let (start, end) = Period::Day.bounds(day);
        let buckets = food_spend_rollup(&conn, "EUR", Period::Day, start, end).unwrap();
        assert_eq!(buckets.len(), 1);
        assert!(!buckets[0].complete);
        assert_eq!(buckets[0].total_base, 0.0);
    }
}
