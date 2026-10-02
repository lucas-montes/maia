use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub db_path: PathBuf,
    pub api_key: String,
    /// Key for the vision model used to parse receipt images. Empty means OCR is
    /// not configured: image uploads still succeed, but the parse endpoints
    /// report `503` instead of calling the model.
    pub model_api_key: String,
    /// Vision model id, e.g. `gemini-2.5-flash`.
    pub model_name: String,
}

impl Config {
    pub fn new(db_path: PathBuf, api_key: String) -> Self {
        Self {
            bind_addr: "0.0.0.0:3030".to_string(),
            db_path,
            api_key,
            model_api_key: String::new(),
            model_name: "gemini-2.5-flash".to_string(),
        }
    }

    pub fn with_bind_addr(mut self, addr: impl Into<String>) -> Self {
        self.bind_addr = addr.into();
        self
    }

    /// Enables receipt OCR. An empty [key] leaves OCR unconfigured.
    pub fn with_model(mut self, key: impl Into<String>, name: impl Into<String>) -> Self {
        self.model_api_key = key.into().trim().to_string();
        self.model_name = name.into();
        self
    }

    /// True when a model key was supplied, i.e. receipt OCR can run.
    pub fn ocr_enabled(&self) -> bool {
        !self.model_api_key.is_empty()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:0".to_string(),
            db_path: PathBuf::from(":memory:"),
            api_key: "test-key".to_string(),
            model_api_key: String::new(),
            model_name: "gemini-2.5-flash".to_string(),
        }
    }
}