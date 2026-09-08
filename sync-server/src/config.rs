use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub db_path: PathBuf,
    pub api_key: String,
}

impl Config {
    pub fn new(db_path: PathBuf, api_key: String) -> Self {
        Self {
            bind_addr: "0.0.0.0:3030".to_string(),
            db_path,
            api_key,
        }
    }

    pub fn with_bind_addr(mut self, addr: impl Into<String>) -> Self {
        self.bind_addr = addr.into();
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:0".to_string(),
            db_path: PathBuf::from(":memory:"),
            api_key: "test-key".to_string(),
        }
    }
}
