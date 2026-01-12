use serde::{Deserialize, Serialize};
use shared::GeminiSchema;

/// Complete receipt information extracted from an image
#[derive(GeminiSchema, Debug, Clone, Serialize, Deserialize)]
#[description = "Receipt information"]
pub struct Receipt {
    #[description = "Store name"]
    store: String,

    #[description = "Purchase date"]
    #[gemini(format = "date-time")]
    date: String,

    #[description = "Total amount"]
    total: f64,

    #[description = "Currency code"]
    currency: String,

    #[description = "List of products"]
    products: Vec<ReceiptProduct>,

    #[description = "List of discounts"]
    discounts: Vec<Discount>,
}

/// A product from a receipt
#[derive(GeminiSchema, Debug, Clone, Serialize, Deserialize)]
#[description = "A product from the receipt"]
pub struct ReceiptProduct {
    #[description = "Product name"]
    name: String,

    #[description = "Product price"]
    price: f64,

    #[description = "Currency code"]
    currency: String,
}

/// A discount applied to a receipt
#[derive(GeminiSchema, Debug, Clone, Serialize, Deserialize)]
#[description = "A discount applied"]
pub struct Discount {
    #[description = "Discount description"]
    description: String,

    #[description = "Discount amount"]
    amount: f64,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Receipt;

    #[test]
    fn test_receipt_schema_matches_model_rs() {
        // Generate schema using the macro
        let generated_schema = Receipt::json_schema();

        // Expected schema from model.rs
        let expected_schema = json!({
            "type": "object",
            "properties": {
                "store": { "type": "string", "description": "Store name" },
                "date": { "type": "string", "format": "date-time", "description": "Purchase date" },
                "total": { "type": "number", "description": "Total amount" },
                "currency": { "type": "string", "description": "Currency code" },
                "products": {
                    "type": "array",
                    "description": "List of products",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Product name" },
                            "price": { "type": "number", "description": "Product price" },
                            "currency": { "type": "string", "description": "Currency code" }
                        },
                        "required": ["name", "price", "currency"],
                        "propertyOrdering": ["name", "price", "currency"]
                    }
                },
                "discounts": {
                    "type": "array",
                    "description": "List of discounts",
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string", "description": "Discount description" },
                            "amount": { "type": "number", "description": "Discount amount" }
                        },
                        "required": ["description", "amount"],
                        "propertyOrdering": ["description", "amount"]
                    }
                }
            },
            "required": ["store", "date", "total", "currency", "products", "discounts"],
            "propertyOrdering": ["store", "date", "total", "currency", "products", "discounts"]
        });

        // Assert top-level structure
        assert_eq!(generated_schema["type"], expected_schema["type"]);
        assert_eq!(generated_schema["required"], expected_schema["required"]);
        assert_eq!(
            generated_schema["propertyOrdering"],
            expected_schema["propertyOrdering"]
        );

        // Assert store property
        assert_eq!(generated_schema["properties"]["store"]["type"], "string");
        assert_eq!(
            generated_schema["properties"]["store"]["description"],
            "Store name"
        );

        // Assert date property with format
        assert_eq!(generated_schema["properties"]["date"]["type"], "string");
        assert_eq!(
            generated_schema["properties"]["date"]["format"],
            "date-time"
        );
        assert_eq!(
            generated_schema["properties"]["date"]["description"],
            "Purchase date"
        );

        // Assert total property
        assert_eq!(generated_schema["properties"]["total"]["type"], "number");
        assert_eq!(
            generated_schema["properties"]["total"]["description"],
            "Total amount"
        );

        // Assert currency property
        assert_eq!(generated_schema["properties"]["currency"]["type"], "string");
        assert_eq!(
            generated_schema["properties"]["currency"]["description"],
            "Currency code"
        );

        // Assert products array
        let products = &generated_schema["properties"]["products"];
        assert_eq!(products["type"], "array");
        assert_eq!(products["description"], "List of products");

        let products_items = &products["items"];
        assert_eq!(products_items["type"], "object");
        assert_eq!(products_items["properties"]["name"]["type"], "string");
        assert_eq!(products_items["properties"]["price"]["type"], "number");
        assert_eq!(products_items["properties"]["currency"]["type"], "string");
        assert_eq!(
            products_items["required"],
            json!(["name", "price", "currency"])
        );
        assert_eq!(
            products_items["propertyOrdering"],
            json!(["name", "price", "currency"])
        );

        // Assert discounts array
        let discounts = &generated_schema["properties"]["discounts"];
        assert_eq!(discounts["type"], "array");
        assert_eq!(discounts["description"], "List of discounts");

        let discounts_items = &discounts["items"];
        assert_eq!(discounts_items["type"], "object");
        assert_eq!(
            discounts_items["properties"]["description"]["type"],
            "string"
        );
        assert_eq!(discounts_items["properties"]["amount"]["type"], "number");
        assert_eq!(
            discounts_items["required"],
            json!(["description", "amount"])
        );
        assert_eq!(
            discounts_items["propertyOrdering"],
            json!(["description", "amount"])
        );
    }
}
