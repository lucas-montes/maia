use std::path::Path;

use base64::{Engine as _, engine::general_purpose};
use serde::de::DeserializeOwned;

use crate::google;

pub trait StructuredOutput {
    fn to_schema() -> serde_json::Value;
}

#[derive(Debug)]
pub enum ModelError {
    ReqwestError(reqwest::Error),
    SerdeError(serde_json::Error),
    Io(std::io::Error),
}

impl From<reqwest::Error> for ModelError {
    fn from(err: reqwest::Error) -> Self {
        ModelError::ReqwestError(err)
    }
}

impl From<serde_json::Error> for ModelError {
    fn from(err: serde_json::Error) -> Self {
        ModelError::SerdeError(err)
    }
}

impl From<std::io::Error> for ModelError {
    fn from(err: std::io::Error) -> Self {
        ModelError::Io(err)
    }
}

pub trait Llm {}
pub trait Vllm {
    fn extract_image<T: DeserializeOwned + StructuredOutput>(
        &self,
        image_path: &Path,
    ) -> impl std::future::Future<Output = Result<Option<T>, ModelError>> + Send;
}

pub trait Mllm: Llm + Vllm {}

#[derive(Debug, Clone)]
pub struct Model {
    model: google::Model,
}

impl Vllm for Model {
    async fn extract_image<T: DeserializeOwned + StructuredOutput>(
        &self,
        image_path: &Path,
    ) -> Result<Option<T>, ModelError> {
        tracing::info!(?image_path, "Handling receipt file");

        let image_base64 =
            std::fs::read(image_path).map(|bytes| general_purpose::STANDARD.encode(&bytes))?;

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

        let receipt = T::to_schema();
        let generation_config = Some(google::GenerationConfig::new(
            Some("application/json".to_string()),
            Some(receipt),
        ));

        let request = google::GeminiRequest::new(None, vec![content], generation_config);

        self.model
            .generate_content(&request)
            .await
            .map(|r| r.inner_to_struct())
            .map_err(ModelError::from)
    }
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
