//! Receipt ↔ store.
//!
//! A store is a chain in one country: Lidl Hrvatska and Lidl France are two
//! stores. A receipt is put on its store by, strongest first:
//!
//! 1. its **tax number** (or VAT number) equal to the store's;
//! 2. a **name the store was seen under** (`store_aliases`), unless the store's
//!    own tax number or country says otherwise;
//! 3. exactly one store of its **brand** whose tax number and country do not
//!    contradict it.
//!
//! Otherwise the receipt has no store, and [`recommendations`] says what to do:
//! rename a store that is named only by its brand, create a new one, or link
//! a likely one. A store the user chose for a receipt (`manual`) is never
//! changed by the matcher, and a store an undo rejected is never proposed
//! again for that receipt.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

use super::effects::{self, NewEffect, STORE_DETAIL_COLUMNS};
use super::text::{self, normalize, tax_digits, tokens};
use super::{AUTO, MANUAL};
use crate::receipt_ocr::ParsedReceipt;

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub id: String,
    pub name: String,
    pub brand: Option<String>,
    pub legal_name: Option<String>,
    pub country: Option<String>,
    pub tax_id: Option<String>,
    pub tax_id_kind: Option<String>,
    pub vat_number: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub postcode: Option<String>,
    pub website: Option<String>,
    pub details_source: Option<String>,
}

const STORE_COLUMNS: &str = "id, name, brand, legal_name, country, tax_id, tax_id_kind, vat_number, \
     address, city, postcode, website, details_source";

fn store_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Store> {
    Ok(Store {
        id: r.get(0)?,
        name: r.get(1)?,
        brand: r.get(2)?,
        legal_name: r.get(3)?,
        country: r.get(4)?,
        tax_id: r.get(5)?,
        tax_id_kind: r.get(6)?,
        vat_number: r.get(7)?,
        address: r.get(8)?,
        city: r.get(9)?,
        postcode: r.get(10)?,
        website: r.get(11)?,
        details_source: r.get(12)?,
    })
}

pub fn load(conn: &Connection, id: &str) -> rusqlite::Result<Option<Store>> {
    conn.query_row(
        &format!("SELECT {STORE_COLUMNS} FROM stores WHERE id = ?1 AND deleted_at IS NULL"),
        params![id],
        store_from_row,
    )
    .optional()
}

pub fn live(conn: &Connection) -> rusqlite::Result<Vec<Store>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {STORE_COLUMNS} FROM stores WHERE deleted_at IS NULL ORDER BY name"
    ))?;
    stmt.query_map([], store_from_row)?.collect()
}

impl Store {
    /// The value of one of [`STORE_DETAIL_COLUMNS`].
    fn detail(&self, column: &str) -> Option<&str> {
        match column {
            "brand" => self.brand.as_deref(),
            "legal_name" => self.legal_name.as_deref(),
            "country" => self.country.as_deref(),
            "tax_id" => self.tax_id.as_deref(),
            "tax_id_kind" => self.tax_id_kind.as_deref(),
            "vat_number" => self.vat_number.as_deref(),
            "address" => self.address.as_deref(),
            "city" => self.city.as_deref(),
            "postcode" => self.postcode.as_deref(),
            "website" => self.website.as_deref(),
            _ => None,
        }
    }

    /// The brand the store goes by: its brand, or else its name.
    fn brand_key(&self) -> String {
        normalize(self.brand.as_deref().unwrap_or(&self.name))
    }

    fn tax_numbers(&self) -> Vec<String> {
        [self.tax_id.as_deref(), self.vat_number.as_deref()]
            .into_iter()
            .flatten()
            .map(tax_digits)
            .filter(|t| !t.is_empty())
            .collect()
    }
}

/// What a reading says about who issued it, cleaned.
#[derive(Debug, Clone, Default)]
pub struct Issuer {
    pub merchant: String,
    pub brand: Option<String>,
    pub country: Option<String>,
    pub tax_numbers: Vec<String>,
    /// The values a reading can fill a store's identifier columns with.
    pub details: Vec<(&'static str, String)>,
}

fn clean(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

impl Issuer {
    pub fn of(r: &ParsedReceipt) -> Self {
        let country = clean(&r.country)
            .map(|c| c.to_uppercase())
            .filter(|c| c.len() == 2);
        let tax_id = clean(&r.tax_id).map(|t| text::tax_key(&t));
        let vat = clean(&r.vat_number).map(|t| text::tax_key(&t));
        let tax_numbers = [tax_id.as_deref(), vat.as_deref()]
            .into_iter()
            .flatten()
            .map(tax_digits)
            .filter(|t| !t.is_empty())
            .collect();
        let mut details = Vec::new();
        let mut push = |column: &'static str, value: Option<String>| {
            if let Some(v) = value {
                details.push((column, v));
            }
        };
        push("brand", clean(&r.brand));
        push("legal_name", clean(&Some(r.merchant.clone())));
        push("country", country.clone());
        push("tax_id", tax_id);
        push(
            "tax_id_kind",
            clean(&r.tax_id_kind).map(|k| k.to_lowercase()),
        );
        push("vat_number", vat);
        push("address", clean(&r.address));
        push("city", clean(&r.city));
        push("postcode", clean(&r.postcode));
        push("website", clean(&r.website));
        Self {
            merchant: r.merchant.trim().to_string(),
            brand: clean(&r.brand),
            country,
            tax_numbers,
            details,
        }
    }

    fn alias(&self) -> String {
        normalize(&self.merchant)
    }

    /// True when the store's own facts say it is another shop than this one.
    fn contradicts(&self, store: &Store) -> bool {
        let theirs = store.tax_numbers();
        if !theirs.is_empty()
            && !self.tax_numbers.is_empty()
            && !theirs.iter().any(|t| self.tax_numbers.contains(t))
        {
            return true;
        }
        matches!((&store.country, &self.country), (Some(a), Some(b)) if !a.eq_ignore_ascii_case(b))
    }

    fn same_tax(&self, store: &Store) -> bool {
        !self.tax_numbers.is_empty()
            && store
                .tax_numbers()
                .iter()
                .any(|t| self.tax_numbers.contains(t))
    }

    /// The store is of this receipt's brand: its brand (or name) is the
    /// reading's brand, or every word of it is in the merchant's name.
    fn same_brand(&self, store: &Store) -> bool {
        let key = store.brand_key();
        if key.is_empty() {
            return false;
        }
        if self.brand.as_deref().map(normalize).as_deref() == Some(key.as_str()) {
            return true;
        }
        let words = tokens(&key);
        let merchant = tokens(&self.merchant);
        !words.is_empty() && words.iter().all(|w| merchant.contains(w))
    }

    /// A name for a new store built from this receipt: the merchant without its
    /// legal form, or the brand with the country when that is all there is.
    pub fn proposed_name(&self) -> String {
        let from_merchant = text::title(&normalize(&self.merchant));
        let brand = self.brand.clone().unwrap_or_default();
        let base = if from_merchant.is_empty() {
            text::title(&brand)
        } else {
            from_merchant
        };
        match &self.country {
            Some(c) if !brand.is_empty() && normalize(&base) == normalize(&brand) => {
                format!("{base} {c}")
            }
            _ => base,
        }
    }
}

/// The receipt's current store and how it was decided.
pub fn current(
    conn: &Connection,
    receipt_id: &str,
) -> rusqlite::Result<Option<(Option<String>, Option<String>)>> {
    conn.query_row(
        "SELECT store_id, store_source FROM receipts WHERE id = ?1 AND deleted_at IS NULL",
        params![receipt_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
}

/// The store the rules give this reading, if exactly one.
pub fn find(
    conn: &Connection,
    receipt_id: &str,
    issuer: &Issuer,
) -> rusqlite::Result<Option<Store>> {
    let stores = live(conn)?;
    let allowed = |s: &Store| -> rusqlite::Result<bool> {
        Ok(!effects::is_rejected(
            conn,
            receipt_id,
            effects::REJECT_STORE,
            &s.id,
            None,
        )?)
    };

    // 1. The tax number.
    for s in &stores {
        if issuer.same_tax(s) && allowed(s)? {
            return Ok(Some(s.clone()));
        }
    }
    // 2. A name the store was seen under.
    let alias = issuer.alias();
    if !alias.is_empty() {
        let by_alias: Option<String> = conn
            .query_row(
                "SELECT store_id FROM store_aliases WHERE alias = ?1",
                params![alias],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(s) = by_alias.and_then(|id| stores.iter().find(|s| s.id == id)) {
            if !issuer.contradicts(s) && allowed(s)? {
                return Ok(Some(s.clone()));
            }
        }
    }
    // 3. Exactly one store of the brand that nothing contradicts.
    let mut fits = Vec::new();
    for s in &stores {
        if issuer.same_brand(s) && !issuer.contradicts(s) && allowed(s)? {
            fits.push(s);
        }
    }
    Ok(match fits.as_slice() {
        [one] => Some((*one).clone()),
        _ => None,
    })
}

/// Puts the receipt on `store_id`, learns its merchant name as an alias of
/// the store, and fills the store's empty identifier columns from the reading.
/// Records each change as an effect. Runs in the caller's transaction.
pub fn assign(
    conn: &Connection,
    receipt_id: &str,
    store_id: &str,
    issuer: Option<&Issuer>,
    source: &str,
    now: i64,
) -> anyhow::Result<()> {
    let Some(store) = load(conn, store_id)? else {
        anyhow::bail!("store {store_id} not found");
    };
    let (before_id, before_source) = current(conn, receipt_id)?.unwrap_or((None, None));
    if before_id.as_deref() != Some(store_id) {
        if let Some(old) = &before_id {
            effects::supersede(
                conn,
                receipt_id,
                effects::STORE_RESOLVED,
                Some(old),
                None,
                now,
            )?;
            // Moving the receipt off a store the matcher chose says it was wrong.
            if before_source.as_deref() != Some(MANUAL) {
                effects::reject(conn, receipt_id, effects::REJECT_STORE, old, None, now)?;
            }
        }
        conn.execute(
            "UPDATE receipts SET store_id = ?1, store_source = ?2 WHERE id = ?3",
            params![store_id, source, receipt_id],
        )?;
        effects::record(
            conn,
            NewEffect {
                receipt_id,
                kind: effects::STORE_RESOLVED,
                target_table: "stores",
                target_id: store_id,
                line: None,
                before: json!({ "storeId": before_id, "storeSource": before_source }),
                after: json!({ "storeId": store_id }),
                source,
            },
            now,
        )?;
    } else if source == MANUAL && before_source.as_deref() != Some(MANUAL) {
        conn.execute(
            "UPDATE receipts SET store_source = ?1 WHERE id = ?2",
            params![MANUAL, receipt_id],
        )?;
    }

    let Some(issuer) = issuer else { return Ok(()) };
    let alias = issuer.alias();
    if !alias.is_empty() {
        let existing: Option<String> = conn
            .query_row(
                "SELECT store_id FROM store_aliases WHERE alias = ?1",
                params![alias],
                |r| r.get(0),
            )
            .optional()?;
        let learn = match existing.as_deref() {
            None => true,
            // A manual choice moves a name another store had.
            Some(other) => other != store_id && source == MANUAL,
        };
        if learn {
            conn.execute(
                "INSERT INTO store_aliases (alias, store_id, source, receipt_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(alias) DO UPDATE SET store_id = excluded.store_id, source = excluded.source,
                     receipt_id = excluded.receipt_id, created_at = excluded.created_at",
                params![alias, store_id, source, receipt_id, now],
            )?;
            effects::record(
                conn,
                NewEffect {
                    receipt_id,
                    kind: effects::ALIAS_LEARNED,
                    target_table: "store_aliases",
                    target_id: &alias,
                    line: None,
                    before: json!({ "storeId": existing }),
                    after: json!({ "storeId": store_id, "merchant": issuer.merchant }),
                    source,
                },
                now,
            )?;
        }
    }

    fill(conn, receipt_id, &store, issuer, source, now)?;
    Ok(())
}

/// Fills the store's empty identifier columns from the reading. A filled one
/// is never overwritten: a different value would be another shop.
fn fill(
    conn: &Connection,
    receipt_id: &str,
    store: &Store,
    issuer: &Issuer,
    source: &str,
    now: i64,
) -> anyhow::Result<()> {
    if issuer.contradicts(store) {
        return Ok(());
    }
    let mut before = serde_json::Map::new();
    let mut after = serde_json::Map::new();
    for (column, value) in &issuer.details {
        if store.detail(column).is_some() {
            continue;
        }
        if *column == "tax_id" {
            // The tax number is unique among live stores.
            let taken: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM stores WHERE tax_id = ?1 AND id <> ?2 AND deleted_at IS NULL)",
                params![value, store.id],
                |r| r.get(0),
            )?;
            if taken {
                continue;
            }
        }
        before.insert((*column).to_string(), Value::Null);
        after.insert((*column).to_string(), json!(value));
    }
    if after.is_empty() {
        return Ok(());
    }
    for (column, value) in &after {
        // `column` comes from STORE_DETAIL_COLUMNS via `Issuer::of`.
        debug_assert!(STORE_DETAIL_COLUMNS.contains(&column.as_str()));
        conn.execute(
            &format!("UPDATE stores SET {column} = ?1, details_source = ?2 WHERE id = ?3"),
            params![value.as_str(), source, store.id],
        )?;
    }
    effects::record(
        conn,
        NewEffect {
            receipt_id,
            kind: effects::STORE_FILLED,
            target_table: "stores",
            target_id: &store.id,
            line: None,
            before: Value::Object(before),
            after: Value::Object(after),
            source,
        },
        now,
    )?;
    Ok(())
}

/// Creates a store from the receipt's reading and puts the receipt on it.
pub fn create(
    conn: &Connection,
    receipt_id: &str,
    issuer: &Issuer,
    name: Option<&str>,
    source: &str,
    now: i64,
) -> anyhow::Result<String> {
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| issuer.proposed_name());
    if name.is_empty() {
        anyhow::bail!("a store needs a name");
    }
    let id = uuid::Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO stores (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![id, name, now],
    )?;
    effects::record(
        conn,
        NewEffect {
            receipt_id,
            kind: effects::STORE_CREATED,
            target_table: "stores",
            target_id: &id,
            line: None,
            before: effects::none(),
            after: json!({ "name": name }),
            source,
        },
        now,
    )?;
    assign(conn, receipt_id, &id, Some(issuer), source, now)?;
    Ok(id)
}

/// Renames a store. With a receipt, the rename is an effect of it.
pub fn rename(
    conn: &Connection,
    store_id: &str,
    name: &str,
    receipt_id: Option<&str>,
    source: &str,
    now: i64,
) -> anyhow::Result<()> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("a store needs a name");
    }
    let Some(store) = load(conn, store_id)? else {
        anyhow::bail!("store {store_id} not found");
    };
    if store.name == name {
        return Ok(());
    }
    // `updated_at` moves: the phone pulls the store's name.
    conn.execute(
        "UPDATE stores SET name = ?1, updated_at = ?2 WHERE id = ?3",
        params![name, now, store_id],
    )?;
    if let Some(receipt_id) = receipt_id {
        effects::record(
            conn,
            NewEffect {
                receipt_id,
                kind: effects::STORE_RENAMED,
                target_table: "stores",
                target_id: store_id,
                line: None,
                before: json!({ "name": store.name }),
                after: json!({ "name": name }),
                source,
            },
            now,
        )?;
    }
    Ok(())
}

/// Edits a store's identifier columns by hand (the Stores view). `None`
/// leaves a column alone and an empty string clears it.
pub fn edit_details(
    conn: &Connection,
    store_id: &str,
    details: &[(String, Option<String>)],
) -> anyhow::Result<()> {
    if load(conn, store_id)?.is_none() {
        anyhow::bail!("store {store_id} not found");
    }
    for (column, value) in details {
        if !STORE_DETAIL_COLUMNS.contains(&column.as_str()) {
            anyhow::bail!("unknown store column {column}");
        }
        let Some(value) = value else { continue };
        let value = value.trim();
        let value = match column.as_str() {
            "tax_id" | "vat_number" => text::tax_key(value),
            "country" => value.to_uppercase(),
            _ => value.to_string(),
        };
        let value = (!value.is_empty()).then_some(value);
        conn.execute(
            &format!("UPDATE stores SET {column} = ?1, details_source = 'manual' WHERE id = ?2"),
            params![value, store_id],
        )?;
    }
    Ok(())
}

/// The matcher's pass over one receipt: finds its store, unless the user
/// chose one. Runs in the caller's transaction. Returns the store it chose.
pub fn resolve(
    conn: &Connection,
    receipt_id: &str,
    reading: &ParsedReceipt,
    now: i64,
) -> anyhow::Result<Option<String>> {
    let Some((current_id, current_source)) = current(conn, receipt_id)? else {
        return Ok(None);
    };
    if current_source.as_deref() == Some(MANUAL) || current_source.as_deref() == Some(super::AI) {
        return Ok(current_id);
    }
    let issuer = Issuer::of(reading);
    match find(conn, receipt_id, &issuer)? {
        Some(store) => {
            assign(conn, receipt_id, &store.id, Some(&issuer), AUTO, now)?;
            Ok(Some(store.id))
        }
        None => {
            // A new reading no longer fits the store the matcher chose before.
            if let Some(old) = current_id {
                effects::supersede(
                    conn,
                    receipt_id,
                    effects::STORE_RESOLVED,
                    Some(&old),
                    None,
                    now,
                )?;
                conn.execute(
                    "UPDATE receipts SET store_id = NULL, store_source = NULL WHERE id = ?1",
                    params![receipt_id],
                )?;
            }
            Ok(None)
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Recommendation {
    /// The store is named only by its brand; the receipt says which shop it is.
    #[serde(rename_all = "camelCase")]
    Rename {
        store_id: String,
        current: String,
        proposed: String,
    },
    /// Create a store for this receipt. `conflictsWith` is a store of the same
    /// brand that the receipt's country or tax number rules out.
    #[serde(rename_all = "camelCase")]
    NewStore {
        proposed: String,
        brand: Option<String>,
        country: Option<String>,
        tax_id: Option<String>,
        conflicts_with: Option<Store>,
    },
    /// A store that looks like this receipt's, which the rules could not settle.
    #[serde(rename_all = "camelCase")]
    SameStore { store: Store },
}

/// What to do about the receipt's store, computed on read.
pub fn recommendations(
    conn: &Connection,
    receipt_id: &str,
    reading: &ParsedReceipt,
) -> anyhow::Result<Vec<Recommendation>> {
    let issuer = Issuer::of(reading);
    let (store_id, _) = current(conn, receipt_id)?.unwrap_or((None, None));
    let mut out = Vec::new();

    if let Some(store) = store_id
        .as_deref()
        .map(|id| load(conn, id))
        .transpose()?
        .flatten()
    {
        let proposed = issuer.proposed_name();
        let name = normalize(&store.name);
        let only_brand = store.brand.as_deref().map(normalize).as_deref() == Some(name.as_str())
            || issuer.brand.as_deref().map(normalize).as_deref() == Some(name.as_str());
        if only_brand
            && !proposed.is_empty()
            && normalize(&proposed) != normalize(&store.name)
            && one_country(conn, &store.id)?
        {
            out.push(Recommendation::Rename {
                store_id: store.id.clone(),
                current: store.name.clone(),
                proposed,
            });
        }
        return Ok(out);
    }

    let mut conflicting = None;
    for s in live(conn)? {
        if effects::is_rejected(conn, receipt_id, effects::REJECT_STORE, &s.id, None)? {
            continue;
        }
        let likely = issuer.same_brand(&s)
            || text::overlap(&tokens(&s.name), &tokens(&issuer.merchant)) >= 0.5;
        if !likely {
            continue;
        }
        if issuer.contradicts(&s) {
            conflicting.get_or_insert(s);
        } else {
            out.push(Recommendation::SameStore { store: s });
        }
    }
    out.push(Recommendation::NewStore {
        proposed: issuer.proposed_name(),
        brand: issuer.brand.clone(),
        country: issuer.country.clone(),
        tax_id: issuer
            .details
            .iter()
            .find(|(c, _)| *c == "tax_id")
            .map(|(_, v)| v.clone()),
        conflicts_with: conflicting,
    });
    Ok(out)
}

/// True when every receipt on the store that says where it is says the same
/// country, so naming the store after that country is safe.
fn one_country(conn: &Connection, store_id: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT parsed_json FROM receipts WHERE store_id = ?1 AND deleted_at IS NULL AND parsed_json IS NOT NULL",
    )?;
    let countries: std::collections::HashSet<String> = stmt
        .query_map(params![store_id], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .filter_map(|j| serde_json::from_str::<ParsedReceipt>(&j).ok())
        .filter_map(|p| p.country.map(|c| c.trim().to_uppercase()))
        .filter(|c| !c.is_empty())
        .collect();
    Ok(countries.len() <= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    type Conn = r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>;

    fn conn() -> Conn {
        let conn = crate::db::init_memory().unwrap().get().unwrap();
        for (id, name) in [("lidl", "lidl"), ("konzum", "konzum"), ("spar", "spar")] {
            conn.execute(
                "INSERT INTO stores (id, name, created_at, updated_at) VALUES (?1, ?2, 1, 1)",
                params![id, name],
            )
            .unwrap();
        }
        conn
    }

    fn receipt(conn: &Connection, id: &str, reading: &ParsedReceipt) {
        conn.execute(
            "INSERT INTO receipts (id, local_path, parsed, parsed_json, created_at, updated_at) VALUES (?1, '', 1, ?2, 1, 1)",
            params![id, serde_json::to_string(reading).unwrap()],
        )
        .unwrap();
    }

    fn lidl_hr() -> ParsedReceipt {
        ParsedReceipt {
            merchant: "LIDL HRVATSKA d.o.o. k.d.".into(),
            brand: Some("Lidl".into()),
            tax_id: Some("66089976432".into()),
            tax_id_kind: Some("oib".into()),
            country: Some("HR".into()),
            city: Some("Zagreb".into()),
            total: 10.0,
            currency: "EUR".into(),
            ..Default::default()
        }
    }

    fn lidl_fr() -> ParsedReceipt {
        ParsedReceipt {
            merchant: "LIDL SNC".into(),
            brand: Some("Lidl".into()),
            tax_id: Some("34326262200000".into()),
            tax_id_kind: Some("siret".into()),
            country: Some("FR".into()),
            total: 10.0,
            currency: "EUR".into(),
            ..Default::default()
        }
    }

    fn store_of(conn: &Connection, receipt: &str) -> (Option<String>, Option<String>) {
        current(conn, receipt).unwrap().unwrap()
    }

    #[test]
    fn a_croatian_lidl_lands_on_lidl_and_fills_it() {
        let conn = conn();
        receipt(&conn, "r1", &lidl_hr());
        assert_eq!(
            resolve(&conn, "r1", &lidl_hr(), 5).unwrap().as_deref(),
            Some("lidl")
        );
        assert_eq!(
            store_of(&conn, "r1"),
            (Some("lidl".into()), Some(AUTO.into()))
        );
        let s = load(&conn, "lidl").unwrap().unwrap();
        assert_eq!(s.country.as_deref(), Some("HR"));
        assert_eq!(s.tax_id.as_deref(), Some("66089976432"));
        assert_eq!(s.legal_name.as_deref(), Some("LIDL HRVATSKA d.o.o. k.d."));
        // ... and it is recommended to be named after the shop it is.
        let recs = recommendations(&conn, "r1", &lidl_hr()).unwrap();
        assert_eq!(
            recs,
            vec![Recommendation::Rename {
                store_id: "lidl".into(),
                current: "lidl".into(),
                proposed: "Lidl Hrvatska".into()
            }]
        );
        let kinds: Vec<String> = effects::for_receipt(&conn, "r1")
            .unwrap()
            .into_iter()
            .map(|e| e.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                effects::STORE_RESOLVED,
                effects::ALIAS_LEARNED,
                effects::STORE_FILLED
            ]
        );
    }

    #[test]
    fn a_french_lidl_never_lands_on_the_croatian_one() {
        let conn = conn();
        receipt(&conn, "r1", &lidl_hr());
        resolve(&conn, "r1", &lidl_hr(), 5).unwrap();
        receipt(&conn, "r2", &lidl_fr());
        assert_eq!(resolve(&conn, "r2", &lidl_fr(), 6).unwrap(), None);
        let recs = recommendations(&conn, "r2", &lidl_fr()).unwrap();
        match recs.last().unwrap() {
            Recommendation::NewStore {
                proposed,
                conflicts_with,
                country,
                ..
            } => {
                assert_eq!(proposed, "Lidl FR");
                assert_eq!(country.as_deref(), Some("FR"));
                assert_eq!(conflicts_with.as_ref().map(|s| s.id.as_str()), Some("lidl"));
            }
            other => panic!("expected a new store, got {other:?}"),
        }
        // Creating it puts the receipt there, and the next French receipt
        // finds it by its tax number.
        let id = create(
            &conn,
            "r2",
            &Issuer::of(&lidl_fr()),
            Some("Lidl France"),
            MANUAL,
            7,
        )
        .unwrap();
        receipt(&conn, "r3", &lidl_fr());
        assert_eq!(resolve(&conn, "r3", &lidl_fr(), 8).unwrap(), Some(id));
    }

    #[test]
    fn the_second_croatian_receipt_is_found_by_its_tax_number_even_without_a_brand() {
        let conn = conn();
        receipt(&conn, "r1", &lidl_hr());
        resolve(&conn, "r1", &lidl_hr(), 5).unwrap();
        let mut plain = lidl_hr();
        plain.brand = None;
        plain.merchant = "L. HRV".into();
        receipt(&conn, "r2", &plain);
        assert_eq!(
            resolve(&conn, "r2", &plain, 6).unwrap().as_deref(),
            Some("lidl")
        );
    }

    #[test]
    fn a_manual_store_is_kept_and_an_undone_one_is_not_proposed_again() {
        let conn = conn();
        receipt(&conn, "r1", &lidl_hr());
        resolve(&conn, "r1", &lidl_hr(), 5).unwrap();
        let resolved = effects::for_receipt(&conn, "r1").unwrap()[0].clone();
        effects::undo(&conn, &resolved.id, 6).unwrap();
        assert_eq!(store_of(&conn, "r1"), (None, None));
        // The fill and the alias went with it.
        let s = load(&conn, "lidl").unwrap().unwrap();
        assert_eq!(s.tax_id, None);
        assert!(
            effects::for_receipt(&conn, "r1")
                .unwrap()
                .iter()
                .all(|e| e.undone_at.is_some())
        );
        // The matcher does not put it back.
        assert_eq!(resolve(&conn, "r1", &lidl_hr(), 7).unwrap(), None);

        assign(&conn, "r1", "spar", None, MANUAL, 8).unwrap();
        assert_eq!(
            resolve(&conn, "r1", &lidl_hr(), 9).unwrap().as_deref(),
            Some("spar")
        );
    }

    #[test]
    fn a_rename_is_an_effect_that_can_be_undone_until_renamed_again() {
        let conn = conn();
        receipt(&conn, "r1", &lidl_hr());
        rename(&conn, "lidl", "Lidl Hrvatska", Some("r1"), MANUAL, 5).unwrap();
        let effect = effects::for_receipt(&conn, "r1").unwrap()[0].clone();
        rename(&conn, "lidl", "Lidl HR", None, MANUAL, 6).unwrap();
        assert!(matches!(
            effects::undo(&conn, &effect.id, 7),
            Err(effects::UndoError::Changed(_))
        ));
        rename(&conn, "lidl", "Lidl Hrvatska", None, MANUAL, 8).unwrap();
        effects::undo(&conn, &effect.id, 9).unwrap();
        assert_eq!(load(&conn, "lidl").unwrap().unwrap().name, "lidl");
    }

    #[test]
    fn a_created_store_can_be_undone_while_nothing_uses_it() {
        let conn = conn();
        receipt(&conn, "r1", &lidl_fr());
        let id = create(&conn, "r1", &Issuer::of(&lidl_fr()), None, MANUAL, 5).unwrap();
        let created = effects::for_receipt(&conn, "r1")
            .unwrap()
            .into_iter()
            .find(|e| e.kind == effects::STORE_CREATED)
            .unwrap();
        effects::undo(&conn, &created.id, 6).unwrap();
        assert!(load(&conn, &id).unwrap().is_none());
        assert_eq!(store_of(&conn, "r1"), (None, None));
    }

    #[test]
    fn two_stores_of_the_brand_settle_nothing_and_are_both_suggested() {
        let conn = conn();
        conn.execute(
            "INSERT INTO stores (id, name, brand, created_at, updated_at) VALUES ('lidl2', 'Lidl Zagreb', 'Lidl', 1, 1)",
            [],
        )
        .unwrap();
        let mut r = lidl_hr();
        r.tax_id = None;
        r.country = None;
        receipt(&conn, "r1", &r);
        assert_eq!(resolve(&conn, "r1", &r, 5).unwrap(), None);
        let same: Vec<String> = recommendations(&conn, "r1", &r)
            .unwrap()
            .into_iter()
            .filter_map(|x| match x {
                Recommendation::SameStore { store } => Some(store.id),
                _ => None,
            })
            .collect();
        // In name order: "Lidl Zagreb" sorts before "lidl".
        assert_eq!(same, ["lidl2", "lidl"]);
    }
}
