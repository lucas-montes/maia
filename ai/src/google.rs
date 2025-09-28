use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::HashMap, rc::Rc};

#[derive(Debug, Serialize)]
pub struct GeminiRequest {
    /// Optional system instructions to guide the model's behavior
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<SystemInstruction>,
    /// The content of the message, the part where we send what we want him to do, ask, or anything
    pub contents: Vec<Content>,
    /// Optional configuration for the generation process, used currently to get a strcutured output
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
}

impl GeminiRequest {
    pub fn new(
        system_instruction: Option<SystemInstruction>,
        contents: Vec<Content>,
        generation_config: Option<GenerationConfig>,
    ) -> Self {
        Self {
            system_instruction,
            contents,
            generation_config,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SystemInstruction {
    parts: Vec<TextPart>,
}

impl SystemInstruction {
    pub fn new(parts: Vec<TextPart>) -> Self {
        Self { parts }
    }
}

#[derive(Debug, Serialize)]
pub struct TextPart {
    text: String,
}

impl TextPart {
    pub fn new(text: String) -> Self {
        Self { text }
    }
}

#[derive(Debug, Serialize)]
pub struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<Part>,
}

impl Content {
    pub fn new(role: Option<String>, parts: Vec<Part>) -> Self {
        Self { role, parts }
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Part {
    /// https://ai.google.dev/gemini-api/docs/text-generation#system-instructions
    Text(TextPart),
    InlineData {
        inline_data: InlineData,
    },
}

/// https://ai.google.dev/gemini-api/docs/text-generation#multimodal-input
#[derive(Debug, Serialize)]
pub struct InlineData {
    mime_type: String,
    data: String,
}

impl InlineData {
    pub fn new(mime_type: String, data: String) -> Self {
        Self { mime_type, data }
    }
}

#[derive(Debug, Serialize)]
pub struct GenerationConfig {
    #[serde(rename = "responseMimeType", skip_serializing_if = "Option::is_none")]
    response_mime_type: Option<String>,
    #[serde(rename = "responseSchema", skip_serializing_if = "Option::is_none")]
    response_schema: Option<serde_json::Value>,
}

impl GenerationConfig {
    pub fn new(
        response_mime_type: Option<String>,
        response_schema: Option<serde_json::Value>,
    ) -> Self {
        Self {
            response_mime_type,
            response_schema,
        }
    }
}

//TODO: create a macro to convert structs to this format
// https://ai.google.dev/gemini-api/docs/structured-output#configuring-a-schema
// #[derive(Debug, Serialize)]
// struct ResponseSchema {
//     #[serde(rename = "type")]
//     schema_type: String,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     items: Option<Box<ResponseSchema>>,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     properties: Option<HashMap<String, ResponseSchema>>,
//     #[serde(rename = "propertyOrdering", skip_serializing_if = "Option::is_none")]
//     property_ordering: Option<Vec<String>>,
// }

#[derive(Debug, Clone)]
pub struct Model {
    name: String,
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl Model {
    pub fn new(api_key: String) -> Self {
        Self {
            name: "gemini-2.5-flash".to_string(),
            api_key,
            base_url: "https://generativelanguage.googleapis.com/v1beta/models".to_string(),
            client: reqwest::Client::new(),
        }
    }

    //TODO: we want to declare the output somewhere instead of using this Value
    pub async fn generate_content(
        &self,
        request: &GeminiRequest,
    ) -> Result<GeminiResponse, reqwest::Error> {
        let url = format!("{}/{}:generateContent", self.base_url, self.name);

        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );

        self.client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(request)
            .send()
            .await?
            .json()
            .await
    }
}
#[derive(Debug, Deserialize)]
pub struct GeminiResponse {
    #[serde(rename = "candidates")]
    candidates: Vec<Candidate>,
    #[serde(rename = "modelVersion")]
    model_version: Option<String>,
    #[serde(rename = "responseId")]
    response_id: Option<String>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<UsageMetadata>,
}

impl GeminiResponse {
    pub fn inner_to_struct<T: DeserializeOwned>(&self) -> Option<T> {
        self.candidates
            .first()
            .and_then(|candidate| candidate.content.parts.first())
            .and_then(|part| serde_json::from_str(&part.text).ok())
    }
}

#[derive(Debug, Deserialize)]
struct Candidate {
    #[serde(rename = "content")]
    content: ContentResponse,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
    #[serde(rename = "index")]
    index: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ContentResponse {
    #[serde(rename = "parts")]
    parts: Vec<PartResponse>,
    #[serde(rename = "role")]
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PartResponse {
    #[serde(rename = "text")]
    text: String,
}

#[derive(Debug, Deserialize)]
struct UsageMetadata {
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "promptTokensDetails")]
    prompt_tokens_details: Option<Vec<PromptTokensDetail>>,
    #[serde(rename = "thoughtsTokenCount")]
    thoughts_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PromptTokensDetail {
    #[serde(rename = "modality")]
    modality: Option<String>,
    #[serde(rename = "tokenCount")]
    token_count: Option<u32>,
}
