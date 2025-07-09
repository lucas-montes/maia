use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
struct GeminiRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction>,
    contents: Vec<Content>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Serialize)]
struct SystemInstruction {
    parts: Vec<TextPart>,
}

#[derive(Debug, Serialize)]
struct TextPart {
    text: String,
}


#[derive(Debug, Serialize)]
struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<Part>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Part {
    /// https://ai.google.dev/gemini-api/docs/text-generation#system-instructions
    Text(TextPart),
    InlineData { inline_data: InlineData },
}

/// https://ai.google.dev/gemini-api/docs/text-generation#multimodal-input
#[derive(Debug, Serialize)]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
struct GenerationConfig {
    #[serde(rename = "responseMimeType", skip_serializing_if = "Option::is_none")]
    response_mime_type: Option<String>,
    #[serde(rename = "responseSchema", skip_serializing_if = "Option::is_none")]
    response_schema: Option<ResponseSchema>,
}

//TODO: create a macro to convert structs to this format
// https://ai.google.dev/gemini-api/docs/structured-output#configuring-a-schema
#[derive(Debug, Serialize)]
struct ResponseSchema {
    #[serde(rename = "type")]
    schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<Box<ResponseSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<HashMap<String, ResponseSchema>>,
    #[serde(rename = "propertyOrdering", skip_serializing_if = "Option::is_none")]
    property_ordering: Option<Vec<String>>,
}

#[derive(Debug)]
struct GoogleModel {
    name: String,
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl GoogleModel {
    fn new(api_key: String) -> Self {
        Self {
            name: "gemini-2.5-flash".to_string(),
            api_key,
            base_url: "https://generativelanguage.googleapis.com/v1beta/models".to_string(),
            client: reqwest::Client::new(),
        }
    }

    fn with_model(mut self, model_name: &str) -> Self {
        self.name = model_name.to_string();
        self
    }

    //TODO: we want to declare the output somewhere instead of using this Value
    async fn generate_content(&self, request: &GeminiRequest) -> Result<serde_json::Value, reqwest::Error> {
        let url = format!("{}{}:generateContent", self.base_url, self.name);

        let response = self.client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        response.json().await
    }
}
