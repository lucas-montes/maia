//! Receipt line ↔ ingredient, and the price the line records.
//!
//! A receipt is in the shop's language and rarely prints a barcode, so a name
//! that merely looks alike only *suggests*. What is trusted is what the user
//! already matched at the same store: every match teaches the store's
//! `product_aliases` that this printed name (and this article code) is that
//! ingredient, in that package. The next receipt from the store prices the
//! same line by itself.
//!
//! The price comes from the reading: the line total after its own discount;
//! a weighed line (`1.234 kg`) is priced by its weight, a counted one per
//! piece. Each price and each learned name is an effect of the receipt.
//! Undoing a price also forgets the name it was learned or applied from, and
//! rejects that ingredient for that line, so it is not learned again from it.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

use super::effects::{self, Effect, NewEffect, UndoError};
use super::text::{self, normalize, overlap, tokens};
use super::transactions::receipt_day;
use super::{AUTO, MANUAL};
use crate::receipt_ocr::{ParsedItem, ParsedReceipt};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineMatch {
    pub ingredient_id: String,
    pub ingredient_name: String,
    pub ingredient_price_id: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngredientCandidate {
    pub id: String,
    pub name: String,
    pub brand: Option<String>,
    pub score: f64,
    /// The store already knows this printed name as this ingredient.
    pub learned: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineView {
    pub line: usize,
    pub name: String,
    pub kind: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub price: f64,
    pub currency: String,
    pub matched: Option<LineMatch>,
    pub candidates: Vec<IngredientCandidate>,
}

/// The key a printed name is learned under.
pub fn printed_key(item: &ParsedItem) -> String {
    normalize(&item.name)
}

fn code_key(item: &ParsedItem) -> Option<String> {
    item.code
        .as_deref()
        .map(|c| {
            c.chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
        })
        .filter(|c| c.len() >= 4)
        .map(|c| format!("code:{c}"))
}

/// What the line cost, and how many grams that bought when the reading says.
pub fn price_of(item: &ParsedItem) -> (f64, Option<f64>) {
    let q = item.quantity.filter(|q| *q > 0.0);
    match (item.unit.as_deref(), q) {
        (Some("kg") | Some("l"), Some(q)) => (item.price, Some(q * 1000.0)),
        (Some("g") | Some("ml"), Some(q)) => (item.price, Some(q)),
        (_, Some(q)) if q > 1.0 => (item.unit_price.unwrap_or(item.price / q), None),
        _ => (item.price, None),
    }
}

/// A live match of the line, unless the line now reads as something else.
fn current_match(
    conn: &Connection,
    receipt_id: &str,
    line: usize,
    item: &ParsedItem,
) -> rusqlite::Result<Option<LineMatch>> {
    let row: Option<(String, String, Option<String>, String, Option<String>)> = conn
        .query_row(
            "SELECT m.printed, m.ingredient_id, m.ingredient_price_id, m.source, i.name
             FROM receipt_line_matches m LEFT JOIN ingredients i ON i.id = m.ingredient_id
             WHERE m.receipt_id = ?1 AND m.line = ?2",
            params![receipt_id, line as i64],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    Ok(row
        .filter(|(printed, ..)| *printed == printed_key(item))
        .map(
            |(_, ingredient_id, ingredient_price_id, source, name)| LineMatch {
                ingredient_id,
                ingredient_name: name.unwrap_or_default(),
                ingredient_price_id,
                source,
            },
        ))
}

fn store_of(conn: &Connection, receipt_id: &str) -> rusqlite::Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT store_id FROM receipts WHERE id = ?1",
            params![receipt_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten())
}

struct Alias {
    ingredient_id: String,
    package_grams: Option<f64>,
}

fn alias(conn: &Connection, store_id: &str, item: &ParsedItem) -> rusqlite::Result<Option<Alias>> {
    for key in code_key(item)
        .into_iter()
        .chain(std::iter::once(printed_key(item)))
    {
        let found = conn
            .query_row(
                "SELECT a.ingredient_id, a.package_grams FROM product_aliases a
                 JOIN ingredients i ON i.id = a.ingredient_id AND i.deleted_at IS NULL
                 WHERE a.store_id = ?1 AND a.printed = ?2",
                params![store_id, key],
                |r| {
                    Ok(Alias {
                        ingredient_id: r.get(0)?,
                        package_grams: r.get(1)?,
                    })
                },
            )
            .optional()?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

/// Every line of the reading, what it is matched to, and for an unmatched
/// product the ingredients it may be, best first.
pub fn lines(
    conn: &Connection,
    receipt_id: &str,
    reading: &ParsedReceipt,
) -> rusqlite::Result<Vec<LineView>> {
    let store = store_of(conn, receipt_id)?;
    let ingredients: Vec<(String, String, Option<String>, Option<String>)> = {
        let mut stmt = conn.prepare(
            "SELECT id, name, brand, barcode FROM ingredients WHERE deleted_at IS NULL AND is_archived = 0",
        )?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<_, _>>()?
    };
    let mut out = Vec::with_capacity(reading.items.len());
    for (i, item) in reading.items.iter().enumerate() {
        let matched = current_match(conn, receipt_id, i, item)?;
        let mut candidates = Vec::new();
        if matched.is_none() && item.kind() == "product" {
            let learned = match &store {
                Some(s) => alias(conn, s, item)?.map(|a| a.ingredient_id),
                None => None,
            };
            let printed = tokens(&item.name);
            let code = item.code.as_deref().map(text::tax_key);
            for (id, name, brand, barcode) in &ingredients {
                if effects::is_rejected(
                    conn,
                    receipt_id,
                    effects::REJECT_INGREDIENT,
                    id,
                    Some(i as i64),
                )? {
                    continue;
                }
                let mut words = tokens(name);
                words.extend(brand.as_deref().map(tokens).unwrap_or_default());
                let mut score = overlap(&printed, &words);
                if code.is_some() && barcode.as_deref().map(text::tax_key) == code {
                    score += 2.0;
                }
                let is_learned = learned.as_deref() == Some(id.as_str());
                if is_learned {
                    score += 3.0;
                }
                if score > 0.0 {
                    candidates.push(IngredientCandidate {
                        id: id.clone(),
                        name: name.clone(),
                        brand: brand.clone(),
                        score,
                        learned: is_learned,
                    });
                }
            }
            candidates.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            candidates.truncate(5);
        }
        out.push(LineView {
            line: i,
            name: item.name.clone(),
            kind: item.kind().to_string(),
            quantity: item.quantity,
            unit: item.unit.clone(),
            price: item.price,
            currency: if item.currency.is_empty() {
                reading.currency.clone()
            } else {
                item.currency.clone()
            },
            matched,
            candidates,
        });
    }
    Ok(out)
}

/// What a match records beyond the line.
pub struct MatchInput<'a> {
    pub ingredient_id: &'a str,
    /// Grams in the package, when the reading does not say (a counted line).
    pub package_grams: Option<f64>,
    pub source_url: Option<&'a str>,
    /// Learn the printed name (and code) for the store. A rule applying a
    /// learned name does not learn it again.
    pub learn: bool,
    pub source: &'a str,
}

/// Matches one line to an ingredient and records its price, at the receipt's
/// store, dated on the receipt's day. Runs in the caller's transaction.
pub fn match_line(
    conn: &Connection,
    receipt_id: &str,
    reading: &ParsedReceipt,
    line: usize,
    input: MatchInput<'_>,
    now: i64,
) -> anyhow::Result<String> {
    let Some(item) = reading.items.get(line) else {
        anyhow::bail!("the receipt has no line {line}");
    };
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ingredients WHERE id = ?1 AND deleted_at IS NULL)",
        params![input.ingredient_id],
        |r| r.get(0),
    )?;
    if !exists {
        anyhow::bail!("ingredient {} not found", input.ingredient_id);
    }
    // A price needs a store. A manual match on a receipt with none makes one
    // from the reading, as the old match did by name.
    let store_id = match store_of(conn, receipt_id)? {
        Some(s) => s,
        None => match super::stores::resolve(conn, receipt_id, reading, now)? {
            Some(s) => s,
            None => super::stores::create(
                conn,
                receipt_id,
                &super::stores::Issuer::of(reading),
                None,
                input.source,
                now,
            )?,
        },
    };

    // Replacing an earlier match of the line ends it first.
    if let Some(previous) = current_match(conn, receipt_id, line, item)? {
        for e in effects::for_receipt(conn, receipt_id)? {
            if e.undone_at.is_none()
                && e.line == Some(line as i64)
                && e.kind == effects::PRICE_RECORDED
            {
                undo_price(conn, &e, false, now)?;
            }
        }
        if previous.ingredient_id != input.ingredient_id && previous.source != MANUAL {
            effects::reject(
                conn,
                receipt_id,
                effects::REJECT_INGREDIENT,
                &previous.ingredient_id,
                Some(line as i64),
                now,
            )?;
        }
    }

    let (price, grams_read) = price_of(item);
    let package_grams = grams_read
        .or(input.package_grams)
        .or_else(|| last_package(conn, input.ingredient_id).ok().flatten());
    let currency = if item.currency.is_empty() {
        reading.currency.clone()
    } else {
        item.currency.clone()
    };
    let created_at: i64 = conn.query_row(
        "SELECT created_at FROM receipts WHERE id = ?1",
        params![receipt_id],
        |r| r.get(0),
    )?;
    // Noon on the receipt's day: one observation per ingredient, store and day.
    let recorded_at = text::day_start_ms(receipt_day(reading, created_at)) + 12 * 3_600_000;

    let before: Option<(String, f64, String, Option<f64>, Option<String>, Option<i64>)> = conn
        .query_row(
            "SELECT id, price, currency_code, package_grams, source_url, deleted_at FROM ingredient_prices
             WHERE ingredient_id = ?1 AND store_id = ?2 AND recorded_at = ?3",
            params![input.ingredient_id, store_id, recorded_at],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()?;
    conn.execute(
        "INSERT INTO ingredient_prices
            (id, ingredient_id, store_id, price, currency_code, package_grams, recorded_at, source_url, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, NULL)
         ON CONFLICT(ingredient_id, store_id, recorded_at) DO UPDATE SET
            price = excluded.price, currency_code = excluded.currency_code,
            package_grams = excluded.package_grams, source_url = excluded.source_url,
            updated_at = excluded.updated_at, deleted_at = NULL",
        params![
            uuid::Uuid::now_v7().to_string(),
            input.ingredient_id,
            store_id,
            price,
            currency,
            package_grams,
            recorded_at,
            input.source_url,
            now
        ],
    )?;
    let price_id: String = conn.query_row(
        "SELECT id FROM ingredient_prices WHERE ingredient_id = ?1 AND store_id = ?2 AND recorded_at = ?3",
        params![input.ingredient_id, store_id, recorded_at],
        |r| r.get(0),
    )?;
    let printed = printed_key(item);
    conn.execute(
        "INSERT INTO receipt_line_matches (receipt_id, line, printed, ingredient_id, ingredient_price_id, source, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(receipt_id, line) DO UPDATE SET printed = excluded.printed, ingredient_id = excluded.ingredient_id,
             ingredient_price_id = excluded.ingredient_price_id, source = excluded.source, created_at = excluded.created_at",
        params![receipt_id, line as i64, printed, input.ingredient_id, price_id, input.source, now],
    )?;
    effects::record(
        conn,
        NewEffect {
            receipt_id,
            kind: effects::PRICE_RECORDED,
            target_table: "ingredient_prices",
            target_id: &price_id,
            line: Some(line as i64),
            before: match &before {
                Some((_, price, currency, grams, url, deleted)) => json!({
                    "existed": deleted.is_none(),
                    "price": price, "currencyCode": currency, "packageGrams": grams, "sourceUrl": url,
                }),
                None => json!({ "existed": false }),
            },
            after: json!({
                "ingredientId": input.ingredient_id, "storeId": store_id, "price": price,
                "currencyCode": currency, "packageGrams": package_grams, "printed": item.name,
            }),
            source: input.source,
        },
        now,
    )?;

    if input.learn {
        let keys: Vec<String> = std::iter::once(printed.clone())
            .chain(code_key(item))
            .filter(|k| !k.is_empty())
            .collect();
        for key in keys {
            learn(
                conn,
                receipt_id,
                line,
                &store_id,
                &key,
                &item.name,
                input.ingredient_id,
                package_grams.filter(|_| grams_read.is_none()),
                input.source,
                now,
            )?;
        }
    }
    Ok(price_id)
}

#[allow(clippy::too_many_arguments)]
fn learn(
    conn: &Connection,
    receipt_id: &str,
    line: usize,
    store_id: &str,
    key: &str,
    printed_as: &str,
    ingredient_id: &str,
    package_grams: Option<f64>,
    source: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let before: Option<(String, Option<f64>)> = conn
        .query_row(
            "SELECT ingredient_id, package_grams FROM product_aliases WHERE store_id = ?1 AND printed = ?2",
            params![store_id, key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if before
        .as_ref()
        .is_some_and(|(i, g)| i == ingredient_id && *g == package_grams)
    {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO product_aliases (store_id, printed, ingredient_id, package_grams, source, receipt_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(store_id, printed) DO UPDATE SET ingredient_id = excluded.ingredient_id,
             package_grams = excluded.package_grams, source = excluded.source,
             receipt_id = excluded.receipt_id, updated_at = excluded.updated_at",
        params![store_id, key, ingredient_id, package_grams, source, receipt_id, now],
    )?;
    effects::record(
        conn,
        NewEffect {
            receipt_id,
            kind: effects::PRODUCT_ALIAS_LEARNED,
            target_table: "product_aliases",
            target_id: &format!("{store_id}|{key}"),
            line: Some(line as i64),
            before: match before {
                Some((i, g)) => json!({ "ingredientId": i, "packageGrams": g }),
                None => json!({}),
            },
            after: json!({ "ingredientId": ingredient_id, "packageGrams": package_grams, "printed": printed_as, "storeId": store_id, "key": key }),
            source,
        },
        now,
    )?;
    Ok(())
}

/// The package size last recorded for an ingredient, at any store.
fn last_package(conn: &Connection, ingredient_id: &str) -> rusqlite::Result<Option<f64>> {
    Ok(conn
        .query_row(
            "SELECT package_grams FROM ingredient_prices
             WHERE ingredient_id = ?1 AND package_grams IS NOT NULL AND deleted_at IS NULL
             ORDER BY recorded_at DESC LIMIT 1",
            params![ingredient_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten())
}

/// The rule's pass: every product line the store already knows by its printed
/// name or code is priced, unless the line is matched or that ingredient was
/// rejected for it. Returns how many lines it priced.
pub fn resolve(
    conn: &Connection,
    receipt_id: &str,
    reading: &ParsedReceipt,
    now: i64,
) -> anyhow::Result<usize> {
    let Some(store_id) = store_of(conn, receipt_id)? else {
        return Ok(0);
    };
    let mut priced = 0;
    for (i, item) in reading.items.iter().enumerate() {
        if item.kind() != "product" || current_match(conn, receipt_id, i, item)?.is_some() {
            continue;
        }
        let Some(a) = alias(conn, &store_id, item)? else {
            continue;
        };
        if effects::is_rejected(
            conn,
            receipt_id,
            effects::REJECT_INGREDIENT,
            &a.ingredient_id,
            Some(i as i64),
        )? {
            continue;
        }
        match_line(
            conn,
            receipt_id,
            reading,
            i,
            MatchInput {
                ingredient_id: &a.ingredient_id,
                package_grams: a.package_grams,
                source_url: None,
                learn: false,
                source: AUTO,
            },
            now,
        )?;
        priced += 1;
    }
    Ok(priced)
}

/// Undoes a recorded price: restores what the day's row held before, or
/// deletes it, and drops the line's match. With `forget`, the name the line
/// was learned under goes too, and the ingredient is rejected for the line.
fn undo_price(conn: &Connection, effect: &Effect, forget: bool, now: i64) -> Result<(), UndoError> {
    let target = effect.target_id.as_str();
    let row: Option<(f64, Option<f64>)> = conn
        .query_row(
            "SELECT price, package_grams FROM ingredient_prices WHERE id = ?1 AND deleted_at IS NULL",
            params![target],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let after_price = effect.after.get("price").and_then(Value::as_f64);
    if let Some((price, _)) = row {
        if after_price.is_some_and(|p| (p - price).abs() > 1e-9) {
            return Err(UndoError::Changed("the price was edited".into()));
        }
    }
    let before = &effect.before;
    if before.get("existed").and_then(Value::as_bool) == Some(true) {
        conn.execute(
            "UPDATE ingredient_prices SET price = ?1, currency_code = ?2, package_grams = ?3, source_url = ?4, updated_at = ?5
             WHERE id = ?6",
            params![
                before.get("price").and_then(Value::as_f64),
                before.get("currencyCode").and_then(Value::as_str),
                before.get("packageGrams").and_then(Value::as_f64),
                before.get("sourceUrl").and_then(Value::as_str),
                now,
                target
            ],
        )?;
    } else {
        // `updated_at` moves: the phone pulls prices and must see the delete.
        conn.execute(
            "UPDATE ingredient_prices SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, target],
        )?;
    }
    let line = effect.line.unwrap_or(-1);
    conn.execute(
        "DELETE FROM receipt_line_matches WHERE receipt_id = ?1 AND line = ?2 AND ingredient_price_id = ?3",
        params![effect.receipt_id, line, target],
    )?;
    conn.execute(
        "UPDATE receipt_effects SET undone_at = ?1 WHERE id = ?2",
        params![now, effect.id],
    )?;
    if forget {
        if let Some(ingredient) = effect.after.get("ingredientId").and_then(Value::as_str) {
            effects::reject(
                conn,
                &effect.receipt_id,
                effects::REJECT_INGREDIENT,
                ingredient,
                Some(line),
                now,
            )?;
            // The name the line was priced from: learned here, or applied here.
            if let (Some(store), Some(printed)) = (
                effect.after.get("storeId").and_then(Value::as_str),
                effect.after.get("printed").and_then(Value::as_str),
            ) {
                let key = normalize(printed);
                conn.execute(
                    "DELETE FROM product_aliases WHERE store_id = ?1 AND printed = ?2 AND ingredient_id = ?3",
                    params![store, key, ingredient],
                )?;
                conn.execute(
                    "DELETE FROM product_aliases WHERE store_id = ?1 AND printed LIKE 'code:%' AND ingredient_id = ?2
                       AND receipt_id = ?3",
                    params![store, ingredient, effect.receipt_id],
                )?;
                conn.execute(
                    "UPDATE receipt_effects SET undone_at = ?1
                     WHERE receipt_id = ?2 AND kind = ?3 AND line = ?4 AND undone_at IS NULL",
                    params![now, effect.receipt_id, effects::PRODUCT_ALIAS_LEARNED, line],
                )?;
            }
        }
    }
    Ok(())
}

/// Undo for the two product effects, called from [`effects::undo`].
pub fn undo_effect(conn: &Connection, effect: &Effect, now: i64) -> Result<(), UndoError> {
    match effect.kind.as_str() {
        effects::PRICE_RECORDED => undo_price(conn, effect, true, now),
        effects::PRODUCT_ALIAS_LEARNED => {
            let store = effect.after.get("storeId").and_then(Value::as_str);
            let key = effect.after.get("key").and_then(Value::as_str);
            let ingredient = effect.after.get("ingredientId").and_then(Value::as_str);
            match effect.before.get("ingredientId").and_then(Value::as_str) {
                // A name that meant another ingredient before goes back to it.
                Some(previous) => {
                    conn.execute(
                        "UPDATE product_aliases SET ingredient_id = ?1, package_grams = ?2, updated_at = ?3
                         WHERE store_id = ?4 AND printed = ?5 AND ingredient_id = ?6",
                        params![
                            previous,
                            effect.before.get("packageGrams").and_then(Value::as_f64),
                            now,
                            store,
                            key,
                            ingredient
                        ],
                    )?;
                }
                None => {
                    conn.execute(
                        "DELETE FROM product_aliases WHERE store_id = ?1 AND printed = ?2 AND ingredient_id = ?3",
                        params![store, key, ingredient],
                    )?;
                }
            }
            conn.execute(
                "UPDATE receipt_effects SET undone_at = ?1 WHERE id = ?2",
                params![now, effect.id],
            )?;
            Ok(())
        }
        _ => Err(UndoError::Db(format!(
            "not a product effect: {}",
            effect.kind
        ))),
    }
}

/// A manual match from the receipt's page: records the price and learns the name.
pub fn match_manually(
    conn: &Connection,
    receipt_id: &str,
    line: usize,
    ingredient_id: &str,
    package_grams: Option<f64>,
    source_url: Option<&str>,
    source: &str,
    now: i64,
) -> anyhow::Result<String> {
    let Some(reading) = super::reading(conn, receipt_id)? else {
        anyhow::bail!("the receipt has not been read");
    };
    match_line(
        conn,
        receipt_id,
        &reading,
        line,
        MatchInput {
            ingredient_id,
            package_grams,
            source_url,
            learn: true,
            source,
        },
        now,
    )
}

/// Ends a line's match from the receipt's page, forgetting the name with it.
pub fn unmatch(
    conn: &Connection,
    receipt_id: &str,
    line: usize,
    now: i64,
) -> Result<(), UndoError> {
    let live: Vec<Effect> = effects::for_receipt(conn, receipt_id)?
        .into_iter()
        .filter(|e| {
            e.undone_at.is_none()
                && e.line == Some(line as i64)
                && e.kind == effects::PRICE_RECORDED
        })
        .collect();
    if live.is_empty() {
        return Err(UndoError::NotFound(format!("a match on line {line}")));
    }
    for e in live {
        undo_price(conn, &e, true, now)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Conn = r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>;

    fn conn() -> Conn {
        let conn = crate::db::init_memory().unwrap().get().unwrap();
        conn.execute(
            "INSERT INTO stores (id, name, created_at, updated_at) VALUES ('lidl', 'lidl', 1, 1)",
            [],
        )
        .unwrap();
        for (id, name, brand) in [
            ("banana", "Banana", None),
            ("mozz", "Mozzarella Grated Xxl", Some("Milbona")),
        ] {
            conn.execute(
                "INSERT INTO ingredients (id, name, brand, calories_per100g, protein_per100g, carbs_per100g, fat_per100g, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 1, 1, 1, 1, 1, 1)",
                params![id, name, brand],
            )
            .unwrap();
        }
        conn
    }

    fn reading() -> ParsedReceipt {
        ParsedReceipt {
            merchant: "LIDL HRVATSKA d.o.o. k.d.".into(),
            brand: Some("Lidl".into()),
            date: "2026-09-01T10:00:00".into(),
            total: 4.0,
            currency: "EUR".into(),
            items: vec![
                ParsedItem {
                    name: "BANANE".into(),
                    quantity: Some(1.234),
                    unit: Some("kg".into()),
                    unit_price: Some(1.29),
                    price: 1.59,
                    currency: "EUR".into(),
                    ..Default::default()
                },
                ParsedItem {
                    name: "MILBONA MOZZARELLA RIBANA".into(),
                    quantity: Some(2.0),
                    unit: Some("pcs".into()),
                    unit_price: Some(1.99),
                    price: 3.98,
                    currency: "EUR".into(),
                    ..Default::default()
                },
                ParsedItem {
                    name: "POVRATNA NAKNADA".into(),
                    price: 0.14,
                    kind: Some("deposit".into()),
                    currency: "EUR".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn receipt(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO receipts (id, local_path, parsed, parsed_json, store_id, store_source, created_at, updated_at)
             VALUES (?1, '', 1, ?2, 'lidl', 'manual', 1, 1)",
            params![id, serde_json::to_string(&reading()).unwrap()],
        )
        .unwrap();
    }

    fn price(conn: &Connection, id: &str) -> (f64, Option<f64>, Option<i64>) {
        conn.query_row(
            "SELECT price, package_grams, deleted_at FROM ingredient_prices WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
    }

    #[test]
    fn a_weighed_line_is_priced_by_its_weight_and_a_counted_one_per_piece() {
        assert_eq!(price_of(&reading().items[0]), (1.59, Some(1234.0)));
        assert_eq!(price_of(&reading().items[1]), (1.99, None));
    }

    #[test]
    fn a_line_matched_once_prices_itself_on_the_next_receipt() {
        let conn = conn();
        receipt(&conn, "r1");
        let id = match_manually(&conn, "r1", 0, "banana", None, None, MANUAL, 5).unwrap();
        assert_eq!(price(&conn, &id), (1.59, Some(1234.0), None));

        receipt(&conn, "r2");
        // Another day, so another observation.
        let mut later = reading();
        later.date = "2026-09-08T10:00:00".into();
        conn.execute(
            "UPDATE receipts SET parsed_json = ?1 WHERE id = 'r2'",
            params![serde_json::to_string(&later).unwrap()],
        )
        .unwrap();
        assert_eq!(resolve(&conn, "r2", &later, 6).unwrap(), 1);
        let lines = lines(&conn, "r2", &later).unwrap();
        assert_eq!(
            lines[0]
                .matched
                .as_ref()
                .map(|m| (m.ingredient_id.as_str(), m.source.as_str())),
            Some(("banana", AUTO))
        );
        // The deposit is never a product.
        assert!(lines[2].candidates.is_empty() && lines[2].matched.is_none());
    }

    #[test]
    fn a_name_that_looks_alike_is_only_suggested() {
        let conn = conn();
        receipt(&conn, "r1");
        assert_eq!(resolve(&conn, "r1", &reading(), 5).unwrap(), 0);
        let lines = lines(&conn, "r1", &reading()).unwrap();
        assert_eq!(lines[1].candidates[0].id, "mozz");
        assert!(!lines[1].candidates[0].learned);
    }

    #[test]
    fn undoing_a_price_forgets_the_name_and_keeps_it_forgotten() {
        let conn = conn();
        receipt(&conn, "r1");
        match_manually(&conn, "r1", 1, "mozz", Some(150.0), None, MANUAL, 5).unwrap();
        receipt(&conn, "r2");
        let mut later = reading();
        later.date = "2026-09-08T10:00:00".into();
        assert_eq!(resolve(&conn, "r2", &later, 6).unwrap(), 1);
        let auto = effects::for_receipt(&conn, "r2")
            .unwrap()
            .into_iter()
            .find(|e| e.kind == effects::PRICE_RECORDED)
            .unwrap();
        // The package size learned with the name came along.
        assert_eq!(auto.after["packageGrams"], 150.0);
        effects::undo(&conn, &auto.id, 7).unwrap();
        assert_eq!(price(&conn, &auto.target_id).2, Some(7));
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM product_aliases", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "the name is forgotten");
        assert_eq!(resolve(&conn, "r2", &later, 8).unwrap(), 0);
        let lines = lines(&conn, "r2", &later).unwrap();
        assert!(
            lines[1].candidates.iter().all(|c| c.id != "mozz"),
            "and the ingredient is not offered for that line again"
        );
    }

    #[test]
    fn matching_the_same_day_twice_keeps_one_price_and_undo_restores_the_first() {
        let conn = conn();
        receipt(&conn, "r1");
        let first = match_manually(&conn, "r1", 1, "mozz", Some(150.0), None, MANUAL, 5).unwrap();
        receipt(&conn, "r2");
        let mut dearer = reading();
        dearer.items[1].unit_price = Some(2.49);
        conn.execute(
            "UPDATE receipts SET parsed_json = ?1 WHERE id = 'r2'",
            params![serde_json::to_string(&dearer).unwrap()],
        )
        .unwrap();
        let second = match_manually(&conn, "r2", 1, "mozz", None, None, MANUAL, 6).unwrap();
        assert_eq!(
            first, second,
            "one observation per ingredient, store and day"
        );
        assert_eq!(price(&conn, &first).0, 2.49);
        let e = effects::for_receipt(&conn, "r2")
            .unwrap()
            .into_iter()
            .find(|e| e.kind == effects::PRICE_RECORDED)
            .unwrap();
        effects::undo(&conn, &e.id, 7).unwrap();
        assert_eq!(price(&conn, &first), (1.99, Some(150.0), None));
    }
}
