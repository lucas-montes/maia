//! Gemini through Google's Interactions API (`POST /v1beta/interactions`).
//!
//! https://aistudio.google.com/api-reference/interactions-api
//!
//! Only the one-shot, non-streaming shape is used: an `input` of content
//! blocks, an optional `system_instruction`, and a JSON `response_format`. The
//! answer is the text of the `model_output` step(s) in `steps`.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Debug, Serialize)]
pub struct InteractionRequest {
    model: String,
    input: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    /// Always `false`: receipts and statements are private, and nothing here
    /// continues a conversation, so there is no reason for Google to keep them.
    store: bool,
}

impl InteractionRequest {
    pub fn new(
        model: String,
        system_instruction: Option<String>,
        input: Vec<Content>,
        response_format: Option<ResponseFormat>,
    ) -> Self {
        Self { model, input, system_instruction, response_format, store: false }
    }
}

/// One input block.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Text { text: String },
    /// Base64 bytes inline.
    Image { data: String, mime_type: String },
}

/// `response_format: {type: "text", mime_type, schema}`.
#[derive(Debug, Serialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
    mime_type: &'static str,
    schema: serde_json::Value,
}

impl ResponseFormat {
    pub fn json(schema: serde_json::Value) -> Self {
        Self { kind: "text", mime_type: "application/json", schema }
    }
}

#[derive(Debug, Clone)]
pub struct Model {
    name: String,
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

pub const DEFAULT_MODEL: &str = "gemini-3.8-flash";
pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
/// Vision extraction sends a full-size image and waits on a large model, so the
/// ceiling is generous — but it must not be unbounded, or a hung upstream call
/// pins a request forever.
const REQUEST_TIMEOUT_SECS: u64 = 90;

/// Configs saved before the move to the Interactions API point at
/// `…/v1beta/models` (where `generateContent` lives); the interactions
/// endpoint is a sibling of `models`, so that suffix is dropped.
fn normalize_base_url(base_url: &str) -> String {
    let url = base_url.trim().trim_end_matches('/');
    url.strip_suffix("/models").unwrap_or(url).to_string()
}

impl Model {
    pub fn with_base_url(api_key: String, name: String, base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_default();
        Self { name, api_key, base_url: normalize_base_url(&base_url), client }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Asks for JSON matching [schema] and decodes it into `T`. `None` when
    /// the interaction did not complete or its text is not a `T`.
    pub async fn generate_json<T: DeserializeOwned>(
        &self,
        system: Option<String>,
        input: Vec<Content>,
        schema: serde_json::Value,
    ) -> Result<Option<T>, GeminiError> {
        let request = InteractionRequest::new(
            self.name.clone(),
            system,
            input,
            Some(ResponseFormat::json(schema)),
        );
        Ok(self.create_interaction(&request).await?.parse_output())
    }

    /// Sends [request] and decodes the response.
    ///
    /// Non-2xx responses become [`GeminiError::Api`] carrying the status and
    /// Gemini's `error.message`, so a rejected API key reads as
    /// "API key not valid" instead of a deserialization failure.
    pub async fn create_interaction(
        &self,
        request: &InteractionRequest,
    ) -> Result<Interaction, GeminiError> {
        let url = format!("{}/interactions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| {
                    if body.trim().is_empty() {
                        status.canonical_reason().unwrap_or("request failed").to_string()
                    } else {
                        body
                    }
                });
            tracing::error!(%status, %message, "gemini request rejected");
            return Err(GeminiError::Api { status: status.as_u16(), message });
        }

        response.json().await.map_err(GeminiError::Http)
    }
}

/// The parts of the Interaction resource this client reads.
#[derive(Debug, Deserialize)]
pub struct Interaction {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    steps: Vec<Step>,
    #[serde(default)]
    usage: Option<Usage>,
}

impl Interaction {
    /// The concatenated text of every `model_output` step, or `None` when the
    /// interaction did not complete (`failed`, `incomplete`, …) or said nothing.
    pub fn output_text(&self) -> Option<String> {
        if let Some(status) = self.status.as_deref().filter(|s| *s != "completed") {
            tracing::warn!(id = ?self.id, status, "gemini interaction did not complete");
            return None;
        }
        let text: String = self
            .steps
            .iter()
            .filter(|step| step.kind == "model_output")
            .flat_map(|step| &step.content)
            .filter(|block| block.kind == "text")
            .filter_map(|block| block.text.as_deref())
            .collect();
        if let Some(usage) = &self.usage {
            tracing::debug!(
                id = ?self.id,
                input = usage.total_input_tokens,
                output = usage.total_output_tokens,
                thought = usage.total_thought_tokens,
                "gemini usage"
            );
        }
        (!text.trim().is_empty()).then_some(text)
    }

    pub fn parse_output<T: DeserializeOwned>(&self) -> Option<T> {
        self.output_text().and_then(|text| serde_json::from_str(&text).ok())
    }
}

#[derive(Debug, Deserialize)]
struct Step {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    total_input_tokens: Option<u32>,
    total_output_tokens: Option<u32>,
    total_thought_tokens: Option<u32>,
}

/// Failure from a Gemini request.
///
/// Split out from [`reqwest::Error`] because a non-2xx response (rejected key,
/// bad request, quota) carries Gemini's own `error.message`, which is far more
/// useful than a bare status — and which decoding the error envelope as a
/// success would otherwise hide.
#[derive(Debug)]
pub enum GeminiError {
    /// Transport-level failure, or a response that did not match the schema.
    Http(reqwest::Error),
    /// The API answered with a non-2xx status.
    Api { status: u16, message: String },
}

impl std::fmt::Display for GeminiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeminiError::Http(e) => write!(f, "{e}"),
            GeminiError::Api { status, message } => write!(f, "gemini {status}: {message}"),
        }
    }
}

impl std::error::Error for GeminiError {}

impl From<reqwest::Error> for GeminiError {
    fn from(err: reqwest::Error) -> Self {
        GeminiError::Http(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_models_base_url_points_at_the_api_root() {
        for url in [
            "https://generativelanguage.googleapis.com/v1beta/models",
            "https://generativelanguage.googleapis.com/v1beta/models/",
            "https://generativelanguage.googleapis.com/v1beta/",
            " https://generativelanguage.googleapis.com/v1beta ",
        ] {
            assert_eq!(normalize_base_url(url), "https://generativelanguage.googleapis.com/v1beta");
        }
    }

    #[test]
    fn request_has_the_interactions_shape() {
        let request = InteractionRequest::new(
            "gemini-3.8-flash".into(),
            Some("be terse".into()),
            vec![
                Content::Text { text: "read this".into() },
                Content::Image { data: "AAAA".into(), mime_type: "image/png".into() },
            ],
            Some(ResponseFormat::json(json!({ "type": "object" }))),
        );
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "model": "gemini-3.8-flash",
                "system_instruction": "be terse",
                "input": [
                    { "type": "text", "text": "read this" },
                    { "type": "image", "data": "AAAA", "mime_type": "image/png" }
                ],
                "response_format": { "type": "text", "mime_type": "application/json", "schema": { "type": "object" } },
                "store": false
            })
        );
    }

    fn interaction(value: serde_json::Value) -> Interaction {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn output_skips_thoughts_and_joins_model_text() {
        let it = interaction(json!({
            "id": "v1_x", "status": "completed",
            "steps": [
                { "type": "thought", "signature": "s" },
                { "type": "model_output", "content": [{ "type": "text", "text": "{\"a\":" }, { "type": "text", "text": "1}" }] }
            ]
        }));
        assert_eq!(it.parse_output::<serde_json::Value>(), Some(json!({ "a": 1 })));
    }

    #[test]
    fn an_unfinished_interaction_has_no_output() {
        let it = interaction(json!({
            "status": "incomplete",
            "steps": [{ "type": "model_output", "content": [{ "type": "text", "text": "{\"a\":" }] }]
        }));
        assert_eq!(it.output_text(), None);
    }
}
