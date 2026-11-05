use ai::StructuredOutput;
use serde::{Deserialize, Serialize};

/// Structured output requested from the vision model for a receipt image.
///
/// Field names are the contract with the mobile client: `BudgetSyncService`
/// builds its draft expense from `total`, `currency`, `merchant` and `date`, and
/// `ReceiptViewerScreen` renders the whole map generically. The historical
/// `shared::purchases::schema::Receipt` used `store`/`products` instead and was
/// never reachable from this crate (`shared` keeps that module private), so the
/// shape is defined here rather than reused.
#[derive(StructuredOutput, Debug, Clone, Serialize, Deserialize)]
#[description = "Receipt information"]
pub struct ParsedReceipt {
    #[description = "Name of the store or merchant that issued the receipt"]
    pub merchant: String,

    #[description = "Date and time of the purchase"]
    #[gemini(format = "date-time")]
    pub date: String,

    #[description = "Total amount paid"]
    pub total: f64,

    #[description = "ISO 4217 currency code of the total"]
    pub currency: String,

    #[description = "Individual line items on the receipt"]
    pub items: Vec<ParsedItem>,

    #[description = "Discounts and promotions applied to the receipt"]
    pub discounts: Vec<ParsedDiscount>,
}

/// A single purchased line on a receipt.
#[derive(StructuredOutput, Debug, Clone, Serialize, Deserialize)]
#[description = "A line item from the receipt"]
pub struct ParsedItem {
    #[description = "Product name as printed"]
    pub name: String,

    #[description = "Price paid for this line"]
    pub price: f64,

    #[description = "ISO 4217 currency code of the line price"]
    pub currency: String,
}

/// A discount applied to the receipt total.
#[derive(StructuredOutput, Debug, Clone, Serialize, Deserialize)]
#[description = "A discount applied"]
pub struct ParsedDiscount {
    #[description = "What the discount was for"]
    pub description: String,

    #[description = "Amount deducted"]
    pub amount: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_uses_merchant_and_items() {
        let schema = ParsedReceipt::to_schema();

        assert_eq!(schema["type"], "object");
        assert_eq!(
            schema["required"],
            serde_json::json!([
                "merchant",
                "date",
                "total",
                "currency",
                "items",
                "discounts"
            ])
        );
        assert_eq!(
            schema["propertyOrdering"],
            serde_json::json!([
                "merchant",
                "date",
                "total",
                "currency",
                "items",
                "discounts"
            ])
        );

        let props = &schema["properties"];
        assert_eq!(props["merchant"]["type"], "string");
        assert_eq!(
            props["merchant"]["description"],
            "Name of the store or merchant that issued the receipt"
        );
        assert_eq!(props["date"]["type"], "string");
        assert_eq!(props["date"]["format"], "date-time");
        assert_eq!(props["total"]["type"], "number");
        assert_eq!(props["currency"]["type"], "string");

        // Nested arrays describe their element object and keep ordering.
        let items = &props["items"];
        assert_eq!(items["type"], "array");
        assert_eq!(items["items"]["type"], "object");
        assert_eq!(items["items"]["properties"]["name"]["type"], "string");
        assert_eq!(items["items"]["properties"]["price"]["type"], "number");
        assert_eq!(
            items["items"]["required"],
            serde_json::json!(["name", "price", "currency"])
        );
        assert_eq!(
            items["items"]["propertyOrdering"],
            serde_json::json!(["name", "price", "currency"])
        );

        let discounts = &props["discounts"];
        assert_eq!(discounts["type"], "array");
        assert_eq!(discounts["items"]["type"], "object");
        assert_eq!(
            discounts["items"]["properties"]["description"]["type"],
            "string"
        );
        assert_eq!(
            discounts["items"]["properties"]["amount"]["type"],
            "number"
        );
        assert_eq!(
            discounts["items"]["required"],
            serde_json::json!(["description", "amount"])
        );
        assert_eq!(
            discounts["items"]["propertyOrdering"],
            serde_json::json!(["description", "amount"])
        );

        // The old names must be gone: the mobile client reads merchant/items.
        assert!(props.get("store").is_none(), "legacy `store` key present");
        assert!(
            props.get("products").is_none(),
            "legacy `products` key present"
        );
    }

    #[test]
    fn round_trips_model_output() {
        let raw = r#"{
            "merchant": "Carrefour",
            "date": "2026-08-15 14:30:00",
            "total": 42.75,
            "currency": "EUR",
            "items": [{"name": "Milk", "price": 1.2, "currency": "EUR"}],
            "discounts": [{"description": "loyalty", "amount": 2.0}]
        }"#;
        let parsed: ParsedReceipt = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.merchant, "Carrefour");
        assert_eq!(parsed.total, 42.75);
        assert_eq!(parsed.currency, "EUR");
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].name, "Milk");
        assert_eq!(parsed.discounts.len(), 1);

        // Re-serialising keeps the keys the mobile client depends on.
        let json = serde_json::to_value(&parsed).unwrap();
        assert_eq!(json["merchant"], "Carrefour");
        assert!(json.get("items").is_some());
        assert!(json.get("products").is_none());
    }
}