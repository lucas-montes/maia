//! "Improve with AI": the model's proposals for what the rules left open.
//!
//! On demand only. The model gets the reading, the stores, the transactions
//! the receipt could be for, the ingredients and what the store has already
//! learned, and proposes a store, a transaction and an ingredient for each
//! line, each with a confidence and a reason. **Nothing is applied here.** A
//! proposal is accepted through the same endpoints a manual choice uses,
//! with `source = "ai"`, so it is recorded, editable and undoable like any
//! other decision. A proposal pointing at something that does not exist is
//! dropped, never trusted.

use std::collections::HashSet;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{products, stores, transactions};
use crate::receipt_ocr::ParsedReceipt;

pub const SYSTEM: &str = "You match a shop receipt to a personal finance and nutrition database. \
You get the receipt as read from the picture, the known stores, the bank transactions it may have paid for, \
the known ingredients, and the names this store's lines were already matched under. \
Propose, for the store: an existing store id, or a new store with a clear name (brand plus country or city, e.g. \"Lidl Hrvatska\"), never mixing shops of different countries or tax numbers. \
For the transaction: the id of the bank transaction this receipt paid for, or null when none fits; amount and date must agree. \
For every line: the ingredient id it is, or a short English name for a new ingredient when none fits and it is food, or not_a_product for deposits, bags, fees and non-food. \
Only use ids from the lists given. Give a confidence from 0 to 1 and a one-sentence reason for each. Prefer null to a guess.";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoreProposal {
    pub store_id: Option<String>,
    pub new_name: Option<String>,
    pub brand: Option<String>,
    pub country: Option<String>,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransactionProposal {
    pub transaction_id: Option<String>,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineProposal {
    pub line: usize,
    pub ingredient_id: Option<String>,
    pub new_ingredient_name: Option<String>,
    pub not_a_product: bool,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Proposals {
    pub store: Option<StoreProposal>,
    pub transaction: Option<TransactionProposal>,
    pub lines: Vec<LineProposal>,
}

/// The ids a proposal may name.
#[derive(Debug, Default)]
pub struct Known {
    pub stores: HashSet<String>,
    pub transactions: HashSet<String>,
    pub ingredients: HashSet<String>,
    pub lines: usize,
}

/// The response schema, in Gemini's `responseSchema` dialect.
pub fn schema() -> Value {
    let nullable = |t: &str, d: &str| json!({ "type": t, "nullable": true, "description": d });
    let confidence = json!({ "type": "number", "description": "0 to 1" });
    let reason = json!({ "type": "string", "description": "One sentence" });
    json!({
        "type": "object",
        "properties": {
            "store": {
                "type": "object",
                "properties": {
                    "store_id": nullable("string", "An id from the known stores, or null for a new store"),
                    "new_name": nullable("string", "The name of a new store, when store_id is null"),
                    "brand": nullable("string", "The chain or brand"),
                    "country": nullable("string", "ISO 3166-1 alpha-2"),
                    "confidence": confidence,
                    "reason": reason,
                },
                "required": ["confidence", "reason"],
                "propertyOrdering": ["store_id", "new_name", "brand", "country", "confidence", "reason"],
            },
            "transaction": {
                "type": "object",
                "properties": {
                    "transaction_id": nullable("string", "An id from the candidate transactions, or null"),
                    "confidence": confidence,
                    "reason": reason,
                },
                "required": ["confidence", "reason"],
                "propertyOrdering": ["transaction_id", "confidence", "reason"],
            },
            "lines": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "line": { "type": "integer", "description": "The line number given" },
                        "ingredient_id": nullable("string", "An id from the known ingredients, or null"),
                        "new_ingredient_name": nullable("string", "A short English name when no known ingredient fits and it is food"),
                        "not_a_product": { "type": "boolean", "description": "True for deposits, bags, fees and non-food" },
                        "confidence": confidence,
                        "reason": reason,
                    },
                    "required": ["line", "not_a_product", "confidence", "reason"],
                    "propertyOrdering": ["line", "ingredient_id", "new_ingredient_name", "not_a_product", "confidence", "reason"],
                },
            },
        },
        "required": ["store", "transaction", "lines"],
        "propertyOrdering": ["store", "transaction", "lines"],
    })
}

/// The data the model reasons over, and the ids it may answer with.
pub fn prompt(
    conn: &Connection,
    receipt_id: &str,
    reading: &ParsedReceipt,
) -> anyhow::Result<(String, Known)> {
    let mut known = Known {
        lines: reading.items.len(),
        ..Default::default()
    };

    let store_list: Vec<Value> = stores::live(conn)?
        .into_iter()
        .map(|s| {
            known.stores.insert(s.id.clone());
            json!({ "id": s.id, "name": s.name, "brand": s.brand, "country": s.country, "tax_id": s.tax_id, "city": s.city })
        })
        .collect();

    let candidates: Vec<Value> = transactions::candidates(conn, receipt_id, reading)?
        .into_iter()
        .take(15)
        .map(|c| {
            known.transactions.insert(c.transaction_id.clone());
            json!({
                "id": c.transaction_id, "description": c.label, "amount": c.amount, "currency": c.currency_code,
                "date": super::text::day_of(c.date).map(|d| d.to_string()), "account": c.account_name,
            })
        })
        .collect();

    let ingredients: Vec<Value> = {
        let mut stmt = conn.prepare(
            "SELECT id, name, brand, calories_per100g, protein_per100g FROM ingredients
             WHERE deleted_at IS NULL AND is_archived = 0 ORDER BY name LIMIT 400",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, f64>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, brand, kcal, protein) = row?;
            known.ingredients.insert(id.clone());
            out.push(json!({ "id": id, "name": name, "brand": brand, "kcal_100g": kcal, "protein_100g": protein }));
        }
        out
    };

    let learned: Vec<Value> = match stores::current(conn, receipt_id)?.and_then(|(s, _)| s) {
        Some(store_id) => {
            let mut stmt = conn.prepare(
                "SELECT a.printed, i.name FROM product_aliases a JOIN ingredients i ON i.id = a.ingredient_id
                 WHERE a.store_id = ?1 LIMIT 200",
            )?;
            stmt.query_map(params![store_id], |r| Ok(json!({ "printed": r.get::<_, String>(0)?, "ingredient": r.get::<_, String>(1)? })))?
                .collect::<Result<_, _>>()?
        }
        None => vec![],
    };

    let lines: Vec<Value> = products::lines(conn, receipt_id, reading)?
        .into_iter()
        .map(|l| {
            json!({
                "line": l.line, "printed": l.name, "kind": l.kind, "quantity": l.quantity, "unit": l.unit,
                "price": l.price, "already_matched_to": l.matched.map(|m| m.ingredient_name),
            })
        })
        .collect();

    let mut receipt = serde_json::to_value(reading)?;
    if let Some(o) = receipt.as_object_mut() {
        o.remove("items");
    }
    let prompt = json!({
        "receipt": receipt,
        "lines": lines,
        "known_stores": store_list,
        "candidate_transactions": candidates,
        "known_ingredients": ingredients,
        "names_this_store_already_learned": learned,
    });
    Ok((serde_json::to_string_pretty(&prompt)?, known))
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn confidence(v: &Value) -> f64 {
    v.get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0)
}

/// Reads the model's answer, keeping only what names things that exist.
pub fn validate(raw: &Value, known: &Known) -> Proposals {
    let store = raw.get("store").filter(|s| s.is_object()).and_then(|s| {
        let store_id = text(s, "store_id").filter(|id| known.stores.contains(id));
        let new_name = if store_id.is_none() {
            text(s, "new_name")
        } else {
            None
        };
        (store_id.is_some() || new_name.is_some()).then(|| StoreProposal {
            store_id,
            new_name,
            brand: text(s, "brand"),
            country: text(s, "country")
                .map(|c| c.to_uppercase())
                .filter(|c| c.len() == 2),
            confidence: confidence(s),
            reason: text(s, "reason").unwrap_or_default(),
        })
    });
    let transaction = raw
        .get("transaction")
        .filter(|t| t.is_object())
        .and_then(|t| {
            text(t, "transaction_id")
                .filter(|id| known.transactions.contains(id))
                .map(|id| TransactionProposal {
                    transaction_id: Some(id),
                    confidence: confidence(t),
                    reason: text(t, "reason").unwrap_or_default(),
                })
        });
    let mut seen = HashSet::new();
    let lines = raw
        .get("lines")
        .and_then(Value::as_array)
        .map(|a| a.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|l| {
            let line = l.get("line").and_then(Value::as_u64)? as usize;
            if line >= known.lines || !seen.insert(line) {
                return None;
            }
            let ingredient_id =
                text(l, "ingredient_id").filter(|id| known.ingredients.contains(id));
            let not_a_product = l
                .get("not_a_product")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let new_ingredient_name = if ingredient_id.is_none() && !not_a_product {
                text(l, "new_ingredient_name")
            } else {
                None
            };
            (ingredient_id.is_some() || new_ingredient_name.is_some() || not_a_product).then(|| {
                LineProposal {
                    line,
                    ingredient_id,
                    new_ingredient_name,
                    not_a_product,
                    confidence: confidence(l),
                    reason: text(l, "reason").unwrap_or_default(),
                }
            })
        })
        .collect();
    Proposals {
        store,
        transaction,
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> Known {
        Known {
            stores: ["lidl".to_string()].into(),
            transactions: ["t1".to_string()].into(),
            ingredients: ["banana".to_string()].into(),
            lines: 3,
        }
    }

    #[test]
    fn a_proposal_naming_something_unknown_is_dropped() {
        let raw = json!({
            "store": { "store_id": "made-up", "new_name": null, "confidence": 0.9, "reason": "x" },
            "transaction": { "transaction_id": "t-unknown", "confidence": 0.9, "reason": "x" },
            "lines": [
                { "line": 0, "ingredient_id": "banana", "not_a_product": false, "confidence": 2.0, "reason": "bananas" },
                { "line": 1, "ingredient_id": "ghost", "not_a_product": false, "confidence": 0.8, "reason": "?" },
                { "line": 2, "not_a_product": true, "confidence": 0.9, "reason": "deposit" },
                { "line": 7, "ingredient_id": "banana", "not_a_product": false, "confidence": 0.9, "reason": "out of range" },
                { "line": 0, "ingredient_id": "banana", "not_a_product": false, "confidence": 0.9, "reason": "twice" },
            ],
        });
        let p = validate(&raw, &known());
        assert_eq!(
            p.store, None,
            "an unknown store id with no new name is nothing"
        );
        assert_eq!(p.transaction, None);
        assert_eq!(p.lines.len(), 2);
        assert_eq!(p.lines[0].ingredient_id.as_deref(), Some("banana"));
        assert_eq!(p.lines[0].confidence, 1.0, "confidence is clamped");
        assert!(p.lines[1].not_a_product);
    }

    #[test]
    fn a_new_store_and_a_new_ingredient_are_kept_as_names() {
        let raw = json!({
            "store": { "store_id": null, "new_name": "Lidl France", "brand": "Lidl", "country": "fr", "confidence": 0.7, "reason": "SIRET" },
            "transaction": { "transaction_id": "t1", "confidence": 0.6, "reason": "amount and day" },
            "lines": [ { "line": 1, "ingredient_id": null, "new_ingredient_name": "Grated mozzarella", "not_a_product": false, "confidence": 0.5, "reason": "food" } ],
        });
        let p = validate(&raw, &known());
        let s = p.store.unwrap();
        assert_eq!(
            (s.new_name.as_deref(), s.country.as_deref()),
            (Some("Lidl France"), Some("FR"))
        );
        assert_eq!(p.transaction.unwrap().transaction_id.as_deref(), Some("t1"));
        assert_eq!(
            p.lines[0].new_ingredient_name.as_deref(),
            Some("Grated mozzarella")
        );
    }

    #[test]
    fn the_schema_requires_the_three_parts() {
        assert_eq!(
            schema()["required"],
            json!(["store", "transaction", "lines"])
        );
    }
}
