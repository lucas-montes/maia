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
    /// The model API rejected the request (bad key, quota, malformed request).
    /// Carries Gemini's own `error.message` so the cause is legible.
    Api { status: u16, message: String },
}

impl From<reqwest::Error> for ModelError {
    fn from(err: reqwest::Error) -> Self {
        ModelError::ReqwestError(err)
    }
}

impl From<crate::google::GeminiError> for ModelError {
    fn from(err: crate::google::GeminiError) -> Self {
        match err {
            crate::google::GeminiError::Http(e) => ModelError::ReqwestError(e),
            crate::google::GeminiError::Api { status, message } => {
                ModelError::Api { status, message }
            }
        }
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

/// Vision-language model able to read structured data out of an image.
pub trait Vllm {
    /// Extracts structured JSON from the image at [image_path].
    ///
    /// Thin wrapper over [`Vllm::extract_image_bytes`] for callers that only
    /// have a path. Prefer the bytes variant when the image is already in
    /// memory — it avoids a redundant read.
    fn extract_image<T: DeserializeOwned + StructuredOutput>(
        &self,
        image_path: &Path,
    ) -> impl std::future::Future<Output = Result<Option<T>, ModelError>> + Send;

    /// Extracts structured JSON from raw [bytes], labelled with [mime].
    ///
    /// This is the entry point a server handler wants: the uploaded bytes are
    /// already in hand, so nothing needs to be re-read from disk.
    fn extract_image_bytes<T: DeserializeOwned + StructuredOutput>(
        &self,
        bytes: Vec<u8>,
        mime: &str,
    ) -> impl std::future::Future<Output = Result<Option<T>, ModelError>> + Send;
}

pub trait Mllm: Llm + Vllm {}

#[derive(Debug, Clone)]
pub struct Model {
    model: google::Model,
}

/// Default MIME for [`Vllm::extract_image`], which takes only a path.
const DEFAULT_IMAGE_MIME: &str = "image/jpeg";

impl Vllm for Model {
    async fn extract_image<T: DeserializeOwned + StructuredOutput>(
        &self,
        image_path: &Path,
    ) -> Result<Option<T>, ModelError> {
        tracing::info!(?image_path, "Handling receipt file");
        let bytes = tokio::fs::read(image_path).await.map_err(ModelError::Io)?;
        self.extract_image_bytes(bytes, DEFAULT_IMAGE_MIME).await
    }

    async fn extract_image_bytes<T: DeserializeOwned + StructuredOutput>(
        &self,
        bytes: Vec<u8>,
        mime: &str,
    ) -> Result<Option<T>, ModelError> {
        tracing::info!(mime, bytes = bytes.len(), "Extracting image content");

        let image_base64 = general_purpose::STANDARD.encode(&bytes);

        //TODO: maybe change this first sentence as the system instruction and use some id to pass multiple images at once
        let content = google::Content::new(
            None,
            vec![
                google::Part::Text(google::TextPart::new(
                    "Extract all receipt information from this image. Return as structured JSON matching the schema.".to_string()
                )),
                google::Part::InlineData {
                    inline_data: google::InlineData::new(mime.to_string(), image_base64),
                }
            ]
        );

        let schema = T::to_schema();
        let generation_config = Some(google::GenerationConfig::new(
            Some("application/json".to_string()),
            Some(schema),
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

    /// Builds a model bound to a specific model id, e.g. `gemini-2.5-flash`.
    pub fn with_model(api_key: String, name: String) -> Self {
        let model = google::Model::with_model(api_key, name);
        Self { model }
    }

    /// Full control over the transport: model id and base URL both overrideable.
    pub fn with_base_url(api_key: String, name: String, base_url: String) -> Self {
        let model = google::Model::with_base_url(api_key, name, base_url);
        Self { model }
    }

    pub async fn extract_receipt<T: DeserializeOwned>(
        &self,
        image_base64: String,
    ) -> Result<Option<T>, ModelError> {
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
            .map_err(ModelError::from)
    }
}
