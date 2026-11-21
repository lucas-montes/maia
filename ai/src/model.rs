use std::path::Path;

use base64::{Engine as _, engine::general_purpose};
use serde::de::DeserializeOwned;

use crate::{google, openai, schema};

pub trait StructuredOutput {
    fn to_schema() -> serde_json::Value;
}

#[derive(Debug)]
pub enum ModelError {
    ReqwestError(reqwest::Error),
    SerdeError(serde_json::Error),
    Io(std::io::Error),
    /// The model API rejected the request (bad key, quota, malformed request).
    /// Carries the provider's own error message so the cause is legible.
    Api { status: u16, message: String },
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelError::ReqwestError(e) => write!(f, "{e}"),
            ModelError::SerdeError(e) => write!(f, "{e}"),
            ModelError::Io(e) => write!(f, "{e}"),
            ModelError::Api { status, message } => write!(f, "{status}: {message}"),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<reqwest::Error> for ModelError {
    fn from(err: reqwest::Error) -> Self {
        ModelError::ReqwestError(err)
    }
}

impl From<google::GeminiError> for ModelError {
    fn from(err: google::GeminiError) -> Self {
        match err {
            google::GeminiError::Http(e) => ModelError::ReqwestError(e),
            google::GeminiError::Api { status, message } => ModelError::Api { status, message },
        }
    }
}

impl From<openai::OpenAiError> for ModelError {
    fn from(err: openai::OpenAiError) -> Self {
        match err {
            openai::OpenAiError::Http(e) => ModelError::ReqwestError(e),
            openai::OpenAiError::Api { status, message } => ModelError::Api { status, message },
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
    fn extract_image_bytes<T: DeserializeOwned + StructuredOutput>(
        &self,
        bytes: Vec<u8>,
        mime: &str,
    ) -> impl std::future::Future<Output = Result<Option<T>, ModelError>> + Send;
}

pub trait Mllm: Llm + Vllm {}

/// Which wire protocol a model speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// Google's `generateContent` API.
    Gemini,
    /// `POST /chat/completions`: OpenAI, OpenRouter, Ollama, llama.cpp, vLLM.
    OpenAi,
}

impl Provider {
    /// Parses the config spelling. Unknown values are `None` rather than a
    /// guess, so a typo surfaces instead of silently talking the wrong protocol.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "gemini" | "google" => Some(Provider::Gemini),
            "openai" | "openai-compatible" => Some(Provider::OpenAi),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Gemini => "gemini",
            Provider::OpenAi => "openai",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Provider::Gemini => google::DEFAULT_BASE_URL,
            Provider::OpenAi => openai::DEFAULT_BASE_URL,
        }
    }
}

/// Everything needed to build a [`Model`]: one saved entry from the config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
    /// The name the config and the task lists refer to it by, e.g. `gemini-flash`.
    pub id: String,
    pub provider: Provider,
    /// `None` uses the provider's public endpoint.
    pub base_url: Option<String>,
    /// The provider's model id, e.g. `gemini-2.5-flash` or `qwen2.5vl:7b`.
    pub model: String,
    /// Empty for endpoints that need none (a local Ollama).
    pub api_key: String,
    /// Whether it can read images; only these may serve OCR.
    pub vision: bool,
}

/// Which saved model produced a result, as recorded beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    pub id: String,
    pub name: String,
}

impl std::fmt::Display for ModelRef {
    /// `gemini-flash (gemini-2.5-flash)`: the config id, which is what a
    /// person picks in Settings, then the provider's id, which is what
    /// actually answered.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.id, self.name)
    }
}

#[derive(Debug, Clone)]
enum Backend {
    Gemini(google::Model),
    OpenAi(openai::Model),
}

#[derive(Debug, Clone)]
pub struct Model {
    id: String,
    backend: Backend,
}

/// Default MIME for [`Vllm::extract_image`], which takes only a path.
const DEFAULT_IMAGE_MIME: &str = "image/jpeg";

const RECEIPT_INSTRUCTION: &str =
    "Extract all receipt information from this image. Return as structured JSON matching the schema.";

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
        tracing::info!(model = %self.reference(), mime, bytes = bytes.len(), "Extracting image content");
        let image_base64 = general_purpose::STANDARD.encode(&bytes);

        match &self.backend {
            Backend::Gemini(model) => {
                let content = google::Content::new(
                    None,
                    vec![
                        google::Part::Text(google::TextPart::new(RECEIPT_INSTRUCTION.to_string())),
                        google::Part::InlineData {
                            inline_data: google::InlineData::new(mime.to_string(), image_base64),
                        },
                    ],
                );
                let generation_config = Some(google::GenerationConfig::new(
                    Some("application/json".to_string()),
                    Some(T::to_schema()),
                ));
                let request = google::GeminiRequest::new(None, vec![content], generation_config);
                model
                    .generate_content(&request)
                    .await
                    .map(|r| r.inner_to_struct())
                    .map_err(ModelError::from)
            }
            Backend::OpenAi(model) => {
                let value = model
                    .generate_json(
                        None,
                        vec![
                            openai::UserPart::Text(RECEIPT_INSTRUCTION.to_string()),
                            openai::UserPart::Image { mime: mime.to_string(), base64: image_base64 },
                        ],
                        schema::to_json_schema(&T::to_schema()),
                    )
                    .await?;
                // A JSON object that does not fit the struct is "no usable
                // answer", the same as Gemini's `inner_to_struct` treats it.
                Ok(value.and_then(|v| serde_json::from_value(v).ok()))
            }
        }
    }
}

impl Model {
    /// A Gemini model on the public endpoint with the default model id.
    pub fn new(api_key: String) -> Self {
        Self::with_model(api_key, google::DEFAULT_MODEL.to_string())
    }

    /// A Gemini model bound to a specific model id, e.g. `gemini-2.5-flash`.
    pub fn with_model(api_key: String, name: String) -> Self {
        Self::with_base_url(api_key, name, google::DEFAULT_BASE_URL.to_string())
    }

    /// A Gemini model with the base URL overridden, which is what tests point
    /// at a local mock.
    pub fn with_base_url(api_key: String, name: String, base_url: String) -> Self {
        Self {
            id: name.clone(),
            backend: Backend::Gemini(google::Model::with_base_url(api_key, name, base_url)),
        }
    }

    pub fn from_spec(spec: &ModelSpec) -> Self {
        let base_url = spec
            .base_url
            .clone()
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(|| spec.provider.default_base_url().to_string());
        let backend = match spec.provider {
            Provider::Gemini => Backend::Gemini(google::Model::with_base_url(
                spec.api_key.clone(),
                spec.model.clone(),
                base_url,
            )),
            Provider::OpenAi => Backend::OpenAi(openai::Model::new(
                spec.api_key.clone(),
                spec.model.clone(),
                base_url,
            )),
        };
        Self { id: spec.id.clone(), backend }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The provider's model id.
    pub fn name(&self) -> &str {
        match &self.backend {
            Backend::Gemini(model) => model.name(),
            Backend::OpenAi(model) => model.name(),
        }
    }

    pub fn reference(&self) -> ModelRef {
        ModelRef { id: self.id.clone(), name: self.name().to_string() }
    }

    /// Asks a text question and reads the answer as JSON matching [schema]
    /// (Gemini's `responseSchema` dialect; converted for other providers).
    /// [system] says what the model is for; [prompt] carries the data. `None`
    /// when the model returns no content or content that is not JSON.
    pub async fn generate_json(
        &self,
        system: String,
        prompt: String,
        schema: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, ModelError> {
        match &self.backend {
            Backend::Gemini(model) => {
                let content = google::Content::new(
                    Some("user".to_string()),
                    vec![google::Part::Text(google::TextPart::new(prompt))],
                );
                let generation_config = Some(google::GenerationConfig::new(
                    Some("application/json".to_string()),
                    Some(schema),
                ));
                let request = google::GeminiRequest::new(
                    Some(google::SystemInstruction::new(system)),
                    vec![content],
                    generation_config,
                );
                model
                    .generate_content(&request)
                    .await
                    .map(|r| r.inner_to_struct())
                    .map_err(ModelError::from)
            }
            Backend::OpenAi(model) => model
                .generate_json(
                    Some(system),
                    vec![openai::UserPart::Text(prompt)],
                    schema::to_json_schema(&schema),
                )
                .await
                .map_err(ModelError::from),
        }
    }
}

/// One model that was tried and did not produce an answer.
#[derive(Debug, Clone)]
pub struct Attempt {
    pub model: ModelRef,
    pub error: String,
}

/// A result together with the model that produced it, and the ones that were
/// tried first and failed.
#[derive(Debug, Clone)]
pub struct Generated<T> {
    pub value: T,
    pub model: ModelRef,
    pub attempts: Vec<Attempt>,
}

#[derive(Debug)]
pub enum ChainError {
    /// No model is assigned to the task.
    Empty,
    /// Every model was tried; each attempt says why it failed.
    AllFailed(Vec<Attempt>),
}

impl std::fmt::Display for ChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainError::Empty => write!(f, "no model is assigned to this task"),
            ChainError::AllFailed(attempts) => {
                write!(f, "every model failed: ")?;
                for (i, attempt) in attempts.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{}: {}", attempt.model, attempt.error)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ChainError {}

/// An ordered fallback list for one task.
///
/// Each model is tried in turn until one returns a usable answer. Every kind
/// of failure moves on — a rejected key, a quota, a timeout, a 5xx, or an
/// answer that is empty or does not fit the schema — because each of those is
/// specific to the model that gave it, and the next one may well succeed.
#[derive(Debug, Clone, Default)]
pub struct ModelChain {
    models: Vec<Model>,
}

impl ModelChain {
    pub fn new(models: Vec<Model>) -> Self {
        Self { models }
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn models(&self) -> &[Model] {
        &self.models
    }

    pub async fn extract_image_bytes<T: DeserializeOwned + StructuredOutput>(
        &self,
        bytes: Vec<u8>,
        mime: &str,
    ) -> Result<Generated<T>, ChainError> {
        self.run(|model| {
            let bytes = bytes.clone();
            async move { model.extract_image_bytes::<T>(bytes, mime).await }
        })
        .await
    }

    pub async fn generate_json(
        &self,
        system: &str,
        prompt: &str,
        schema: &serde_json::Value,
    ) -> Result<Generated<serde_json::Value>, ChainError> {
        self.run(|model| async move {
            model
                .generate_json(system.to_string(), prompt.to_string(), schema.clone())
                .await
        })
        .await
    }

    async fn run<'a, T, F, Fut>(&'a self, call: F) -> Result<Generated<T>, ChainError>
    where
        F: Fn(&'a Model) -> Fut,
        Fut: std::future::Future<Output = Result<Option<T>, ModelError>>,
    {
        if self.models.is_empty() {
            return Err(ChainError::Empty);
        }
        let mut attempts = Vec::new();
        for model in &self.models {
            let error = match call(model).await {
                Ok(Some(value)) => {
                    return Ok(Generated { value, model: model.reference(), attempts });
                }
                Ok(None) => "the model gave no usable answer".to_string(),
                Err(e) => e.to_string(),
            };
            tracing::warn!(model = %model.reference(), %error, "model failed; trying the next one");
            attempts.push(Attempt { model: model.reference(), error });
        }
        Err(ChainError::AllFailed(attempts))
    }
}
