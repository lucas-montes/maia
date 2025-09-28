use serde::de::DeserializeOwned;

use crate::google;

#[derive(Debug, Clone)]
pub struct Model {
    model: google::Model,
}

impl Model {
    pub fn new(api_key: String) -> Self {
        let model = google::Model::new(api_key);
        Self { model }
    }

    pub async fn extract_receipt<T: DeserializeOwned>(
        &self,
        image_base64: String,
    ) -> Result<Option<T>, reqwest::Error> {
        //TODO: maybe change this first sentence as the system instruction and use some id to pass multiple images at once
        let content = google::Content::new(
            None,
            vec![
                google::Part::Text(google::TextPart::new(
                    "Extract all receipt information from this image. Return as structured JSON matching the schema.".to_string()
                )),
                google::Part::InlineData {
                    inline_data: google::InlineData::new(
                        "image/jpeg".to_string(), // or "image/png"
                        image_base64,
                    ),
                }
            ]
        );

        let receipt = serde_json::json!({
          "type": "object",
          "properties": {
            "store": { "type": "string" },
            "date": { "type": "string", "format": "date-time" },
            "total": { "type": "number" },
            "currency": { "type": "string" },
            "products": {
              "type": "array",
              "items": {
                "type": "object",
                "properties": {
                  "name": { "type": "string" },
                  "price": { "type": "number" },
                  "currency": { "type": "string" }
                },
                "propertyOrdering": ["name", "price", "currency"]
              }
            },
            "discounts": {
              "type": "array",
              "items": {
                "type": "object",
                "properties": {
                  "description": { "type": "string" },
                  "amount": { "type": "number" }
                },
                "propertyOrdering": ["description", "amount"]
              }
            }
          },
          "propertyOrdering": ["store", "date", "total", "currency", "products", "discounts"]
        });
        let generation_config = Some(google::GenerationConfig::new(
            Some("application/json".to_string()),
            Some(receipt),
        ));

        let request = google::GeminiRequest::new(None, vec![content], generation_config);

        self.model
            .generate_content(&request)
            .await
            .map(|r| r.inner_to_struct())
    }
}
