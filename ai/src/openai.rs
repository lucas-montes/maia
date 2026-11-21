//! OpenAI-compatible chat completions: OpenAI itself, OpenRouter, Ollama,
//! llama.cpp, vLLM — anything that answers `POST {base}/chat/completions`.
//!
//! Structured output is asked for with `response_format: json_schema`. Not
//! every compatible server honours it (older Ollama builds, some OpenRouter
//! routes), so a 400 on that request is retried once with plain
//! `json_object` and the schema spelled out in the system prompt instead.

use serde_json::{Value, json};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Same ceiling as the Gemini backend: a local vision model on a CPU can be
/// slow, but a hung upstream must not pin a request forever.
const REQUEST_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone)]
pub struct Model {
    name: String,
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

/// Failure from an OpenAI-compatible request.
#[derive(Debug)]
pub enum OpenAiError {
    Http(reqwest::Error),
    Api { status: u16, message: String },
}

impl std::fmt::Display for OpenAiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenAiError::Http(e) => write!(f, "{e}"),
            OpenAiError::Api { status, message } => write!(f, "openai-compatible {status}: {message}"),
        }
    }
}

impl std::error::Error for OpenAiError {}

impl From<reqwest::Error> for OpenAiError {
    fn from(err: reqwest::Error) -> Self {
        OpenAiError::Http(err)
    }
}

/// What a request carries besides the prompt text.
pub enum UserPart {
    Text(String),
    /// Raw base64, labelled with its MIME type; sent as a data URI.
    Image { mime: String, base64: String },
}

impl Model {
    /// An empty [api_key] sends no `Authorization` header at all, which is what
    /// a local Ollama expects.
    pub fn new(api_key: String, name: String, base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_default();
        Self {
            name,
            api_key: api_key.trim().to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Asks for JSON matching [schema] (JSON Schema, not Gemini's dialect —
    /// see [`crate::schema::to_json_schema`]) and returns it parsed. `None`
    /// when the model answers with no content or content that is not JSON.
    pub async fn generate_json(
        &self,
        system: Option<String>,
        parts: Vec<UserPart>,
        schema: Value,
    ) -> Result<Option<Value>, OpenAiError> {
        let content: Vec<Value> = parts
            .into_iter()
            .map(|part| match part {
                UserPart::Text(text) => json!({ "type": "text", "text": text }),
                UserPart::Image { mime, base64 } => json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:{mime};base64,{base64}") }
                }),
            })
            .collect();

        let strict = self
            .send(&self.body(system.clone(), &content, json!({
                "type": "json_schema",
                "json_schema": { "name": "output", "schema": schema, "strict": false }
            })))
            .await;
        match strict {
            Err(OpenAiError::Api { status: 400, ref message }) => {
                tracing::warn!(model = %self.name, %message, "json_schema rejected; retrying with json_object");
                let system = format!(
                    "{}Answer with a single JSON object matching this JSON Schema, and nothing else:\n{}",
                    system.map(|s| format!("{s}\n\n")).unwrap_or_default(),
                    schema_text(&schema),
                );
                self.send(&self.body(Some(system), &content, json!({ "type": "json_object" })))
                    .await
            }
            other => other,
        }
    }

    fn body(&self, system: Option<String>, content: &[Value], response_format: Value) -> Value {
        let mut messages = Vec::new();
        if let Some(system) = system {
            messages.push(json!({ "role": "system", "content": system }));
        }
        messages.push(json!({ "role": "user", "content": content }));
        json!({
            "model": self.name,
            "messages": messages,
            "response_format": response_format,
        })
    }

    async fn send(&self, body: &Value) -> Result<Option<Value>, OpenAiError> {
        let mut request = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(body);
        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }
        let response = request.send().await?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .or_else(|| v.get("error"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| {
                    if text.trim().is_empty() {
                        status.canonical_reason().unwrap_or("request failed").to_string()
                    } else {
                        text
                    }
                });
            tracing::error!(%status, %message, model = %self.name, "openai-compatible request rejected");
            return Err(OpenAiError::Api { status: status.as_u16(), message });
        }

        let reply: Value = response.json().await?;
        Ok(reply
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .and_then(parse_json_answer))
    }
}

fn schema_text(schema: &Value) -> String {
    serde_json::to_string(schema).unwrap_or_default()
}

/// The JSON in a model's answer. Local models in `json_object` mode sometimes
/// wrap it in a Markdown fence despite being told not to, so a fence is
/// tolerated rather than counted as a failure.
fn parse_json_answer(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.strip_suffix("```"))
        .unwrap_or(trimmed)
        .trim();
    serde_json::from_str(unfenced).ok().filter(Value::is_object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_read_with_or_without_a_fence() {
        assert_eq!(parse_json_answer(r#"{"a":1}"#), Some(json!({"a":1})));
        assert_eq!(parse_json_answer("```json\n{\"a\":1}\n```"), Some(json!({"a":1})));
        assert_eq!(parse_json_answer("```\n{\"a\":1}\n```"), Some(json!({"a":1})));
        assert_eq!(parse_json_answer("sure! here it is"), None);
        assert_eq!(parse_json_answer("[1,2]"), None, "the answer must be an object");
    }
}
