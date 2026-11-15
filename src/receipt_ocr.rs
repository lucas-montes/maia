//! What the vision model reads off a receipt (schema v2).
//!
//! A receipt is read for everything it prints: who issued it (legal name,
//! brand, tax number, address), how it was paid (method, card digits), the
//! totals and VAT, and every line with its quantity, unit, discount and kind.
//! Matching (`crate::matching`) needs all of it: the tax number tells two
//! Lidls apart, the card digits pick the bank line, the weight prices a
//! weighed product.
//!
//! Only `merchant`, `date`, `total`, `currency` and the line `name`, `price`
//! and `currency` are required, as in v1, so a v1 reading still deserialises.
//! Everything else is nullable, because a receipt may not print it, and every
//! list defaults to empty.
//!
//! The JSON keys are the Rust field names (snake_case). The schema is written
//! by hand rather than derived: Gemini's `responseSchema` wants `nullable`
//! and `enum`, which the derive macro does not emit.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ParsedReceipt {
    // --- who issued it
    pub merchant: String,
    #[serde(default)]
    pub brand: Option<String>,
    #[serde(default)]
    pub tax_id: Option<String>,
    #[serde(default)]
    pub tax_id_kind: Option<String>,
    #[serde(default)]
    pub vat_number: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub postcode: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub website: Option<String>,

    // --- the sale
    pub date: String,
    #[serde(default)]
    pub receipt_number: Option<String>,
    #[serde(default)]
    pub payment_method: Option<String>,
    #[serde(default)]
    pub card_last4: Option<String>,
    #[serde(default)]
    pub subtotal: Option<f64>,
    pub total: f64,
    pub currency: String,
    #[serde(default)]
    pub vat: Vec<ParsedVat>,
    #[serde(default)]
    pub discounts: Vec<ParsedDiscount>,
    #[serde(default)]
    pub loyalty: Option<String>,

    // --- the lines
    #[serde(default)]
    pub items: Vec<ParsedItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ParsedItem {
    pub name: String,
    #[serde(default)]
    pub quantity: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub unit_price: Option<f64>,
    /// The line total paid, after the line's own discount.
    pub price: f64,
    /// The line's own discount, as a positive amount taken off.
    #[serde(default)]
    pub discount: Option<f64>,
    #[serde(default)]
    pub vat_rate: Option<f64>,
    #[serde(default)]
    pub code: Option<String>,
    /// `product`, `deposit`, `bag`, `fee` or `other`. Absent reads as product.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub currency: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ParsedVat {
    pub rate: f64,
    #[serde(default)]
    pub base: Option<f64>,
    pub amount: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ParsedDiscount {
    pub description: String,
    pub amount: f64,
}

pub const ITEM_KINDS: [&str; 5] = ["product", "deposit", "bag", "fee", "other"];
pub const UNITS: [&str; 6] = ["pcs", "kg", "g", "l", "ml", "m"];
pub const PAYMENT_METHODS: [&str; 3] = ["card", "cash", "other"];
pub const TAX_ID_KINDS: [&str; 5] = ["oib", "siret", "siren", "vat", "other"];

impl ParsedItem {
    /// The kind, with an absent or unknown one read as a product.
    pub fn kind(&self) -> &str {
        match self.kind.as_deref() {
            Some(k) if ITEM_KINDS.contains(&k) => k,
            _ => "product",
        }
    }
}

fn text(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn opt_text(description: &str) -> Value {
    json!({ "type": "string", "nullable": true, "description": description })
}

fn opt_number(description: &str) -> Value {
    json!({ "type": "number", "nullable": true, "description": description })
}

fn opt_enum(values: &[&str], description: &str) -> Value {
    json!({ "type": "string", "format": "enum", "enum": values, "nullable": true, "description": description })
}

fn object(properties: &[(&str, Value)], required: &[&str]) -> Value {
    let mut props = serde_json::Map::new();
    for (k, v) in properties {
        props.insert((*k).to_string(), v.clone());
    }
    let order: Vec<&str> = properties.iter().map(|(k, _)| *k).collect();
    json!({ "type": "object", "properties": props, "required": required, "propertyOrdering": order })
}

impl ai::StructuredOutput for ParsedReceipt {
    fn to_schema() -> Value {
        let item = object(
            &[
                ("name", text("Product name exactly as printed")),
                (
                    "quantity",
                    opt_number(
                        "How many pieces, or the weight or volume for a weighed line (1.234 for 1,234 kg)",
                    ),
                ),
                (
                    "unit",
                    opt_enum(
                        &UNITS,
                        "Unit of the quantity: pcs for pieces, kg/g for weight, l/ml for volume",
                    ),
                ),
                (
                    "unit_price",
                    opt_number("Price of one unit (per piece or per kg) as printed"),
                ),
                (
                    "price",
                    json!({"type": "number", "description": "Total paid for this line, after this line's own discount"}),
                ),
                (
                    "discount",
                    opt_number(
                        "This line's own discount, as a positive amount taken off; null if none",
                    ),
                ),
                (
                    "vat_rate",
                    opt_number("VAT rate of this line in percent, e.g. 25"),
                ),
                (
                    "code",
                    opt_text("Article code or barcode printed on the line, if any"),
                ),
                (
                    "kind",
                    opt_enum(
                        &ITEM_KINDS,
                        "product for goods; deposit for bottle deposits (povratna naknada, consigne); bag for shopping bags; fee for service fees; other for anything else",
                    ),
                ),
                ("currency", text("ISO 4217 currency code of the line price")),
            ],
            &["name", "price", "currency"],
        );
        let vat = object(
            &[
                (
                    "rate",
                    json!({"type": "number", "description": "VAT rate in percent"}),
                ),
                ("base", opt_number("Amount the rate applies to")),
                (
                    "amount",
                    json!({"type": "number", "description": "VAT amount"}),
                ),
            ],
            &["rate", "amount"],
        );
        let discount = object(
            &[
                ("description", text("What the discount was for")),
                (
                    "amount",
                    json!({"type": "number", "description": "Amount deducted, positive"}),
                ),
            ],
            &["description", "amount"],
        );
        let mut schema = object(
            &[
                (
                    "merchant",
                    text(
                        "Legal name of the company that issued the receipt, as printed (e.g. LIDL HRVATSKA d.o.o. k.d.)",
                    ),
                ),
                (
                    "brand",
                    opt_text("The shop's brand or chain name (e.g. Lidl, Konzum, Carrefour)"),
                ),
                (
                    "tax_id",
                    opt_text(
                        "The company's national tax number as printed: OIB in Croatia, SIRET or SIREN in France",
                    ),
                ),
                (
                    "tax_id_kind",
                    opt_enum(&TAX_ID_KINDS, "Which kind of number tax_id is"),
                ),
                (
                    "vat_number",
                    opt_text("EU VAT number if printed separately (e.g. HR12345678901, FR...)"),
                ),
                ("address", opt_text("Street address of the shop")),
                ("city", opt_text("City of the shop")),
                ("postcode", opt_text("Postal code of the shop")),
                (
                    "country",
                    opt_text("ISO 3166-1 alpha-2 country code of the shop, e.g. HR, FR"),
                ),
                ("branch", opt_text("Shop or branch number, if printed")),
                ("website", opt_text("Website printed on the receipt")),
                (
                    "date",
                    json!({"type": "string", "format": "date-time", "description": "Date and time of the purchase"}),
                ),
                ("receipt_number", opt_text("Receipt or invoice number")),
                (
                    "payment_method",
                    opt_enum(&PAYMENT_METHODS, "How it was paid"),
                ),
                (
                    "card_last4",
                    opt_text("Last four digits of the payment card, if printed"),
                ),
                (
                    "subtotal",
                    opt_number("Total before receipt-wide discounts, if printed"),
                ),
                (
                    "total",
                    json!({"type": "number", "description": "Total amount paid"}),
                ),
                ("currency", text("ISO 4217 currency code of the total")),
                (
                    "vat",
                    json!({"type": "array", "items": vat, "description": "VAT breakdown"}),
                ),
                (
                    "discounts",
                    json!({"type": "array", "items": discount, "description": "Discounts applied to the whole receipt (not to one line)"}),
                ),
                (
                    "loyalty",
                    opt_text("Loyalty card number or points line, if printed"),
                ),
                (
                    "items",
                    json!({"type": "array", "items": item, "description": "Every line of the receipt, in order"}),
                ),
            ],
            &["merchant", "date", "total", "currency", "items"],
        );
        schema["description"] = json!("Everything printed on a shop receipt");
        schema
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai::StructuredOutput;

    #[test]
    fn the_schema_requires_the_v1_core_and_makes_the_rest_nullable() {
        let schema = ParsedReceipt::to_schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(
            schema["required"],
            json!(["merchant", "date", "total", "currency", "items"])
        );
        let props = &schema["properties"];
        assert_eq!(props["tax_id"]["nullable"], true);
        assert_eq!(props["date"]["format"], "date-time");
        assert_eq!(props["payment_method"]["enum"], json!(PAYMENT_METHODS));
        let item = &props["items"]["items"];
        assert_eq!(item["required"], json!(["name", "price", "currency"]));
        assert_eq!(item["properties"]["kind"]["enum"], json!(ITEM_KINDS));
        assert_eq!(item["properties"]["quantity"]["nullable"], true);
        // The order the model writes in is the order a receipt is read in.
        assert_eq!(schema["propertyOrdering"][0], "merchant");
        assert!(props.get("store").is_none() && props.get("products").is_none());
    }

    #[test]
    fn a_v1_reading_still_reads() {
        let raw = r#"{
            "merchant": "Carrefour", "date": "2026-08-15 14:30:00", "total": 42.75, "currency": "EUR",
            "items": [{"name": "Milk", "price": 1.2, "currency": "EUR"}],
            "discounts": [{"description": "loyalty", "amount": 2.0}]
        }"#;
        let parsed: ParsedReceipt = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.items[0].kind(), "product");
        assert_eq!(parsed.items[0].quantity, None);
        assert!(parsed.vat.is_empty());
    }

    #[test]
    fn a_full_croatian_receipt_round_trips() {
        let raw = r#"{
            "merchant": "LIDL HRVATSKA d.o.o. k.d.", "brand": "Lidl", "tax_id": "66089976432",
            "tax_id_kind": "oib", "vat_number": null, "address": "Ulica grada Vukovara 1",
            "city": "Zagreb", "postcode": "10000", "country": "HR", "branch": "0196", "website": "www.lidl.hr",
            "date": "2026-08-01T18:02:00", "receipt_number": "1234/0196/3", "payment_method": "card",
            "card_last4": "6510", "subtotal": 24.19, "total": 23.69, "currency": "EUR",
            "vat": [{"rate": 25, "base": 18.95, "amount": 4.74}],
            "discounts": [{"description": "Lidl Plus", "amount": 0.5}], "loyalty": "Lidl Plus",
            "items": [
                {"name": "BANANE", "quantity": 1.234, "unit": "kg", "unit_price": 1.29, "price": 1.59,
                 "discount": null, "vat_rate": 13, "code": null, "kind": "product", "currency": "EUR"},
                {"name": "POVRATNA NAKNADA", "quantity": 2, "unit": "pcs", "unit_price": 0.07, "price": 0.14,
                 "kind": "deposit", "currency": "EUR"}
            ]
        }"#;
        let parsed: ParsedReceipt = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.card_last4.as_deref(), Some("6510"));
        assert_eq!(parsed.items[1].kind(), "deposit");
        let again: ParsedReceipt =
            serde_json::from_value(serde_json::to_value(&parsed).unwrap()).unwrap();
        assert_eq!(again, parsed);
    }
}
