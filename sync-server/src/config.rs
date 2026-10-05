use std::path::PathBuf;

/// Environment variables read by [`Config::from_env`].
pub mod env_keys {
    pub const BIND_ADDR: &str = "MAIA_BIND_ADDR";
    pub const DB_PATH: &str = "MAIA_DB_PATH";
    pub const MEDIA_DIR: &str = "MAIA_MEDIA_DIR";
    pub const SEED_DIR: &str = "MAIA_SEED_DIR";
    pub const API_KEY: &str = "MAIA_API_KEY";
    pub const MODEL_API_KEY: &str = "MAIA_MODEL_API_KEY";
    pub const MODEL_NAME: &str = "MAIA_MODEL_NAME";
}

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
    /// Where exercise images and videos are read from, serving `GET /media/*`.
    ///
    /// `None` falls back to `<db_dir>/exercise_media`, which is what a single
    /// checkout with the database beside the media wants.
    pub media_dir: Option<PathBuf>,
    /// Where the exercise catalogue is seeded from on first boot, looked for as
    /// `<dir>/parsed`. `None` disables seeding.
    ///
    /// Separate from [`Config::media_dir`] because seeding reads from the repo
    /// checkout while media is served from wherever the deployment put it.
    pub seed_dir: Option<PathBuf>,
}

impl Config {
    pub fn new(db_path: PathBuf, api_key: String) -> Self {
        Self {
            bind_addr: "0.0.0.0:3030".to_string(),
            db_path,
            api_key,
            model_api_key: String::new(),
            model_name: "gemini-2.5-flash".to_string(),
            media_dir: None,
            seed_dir: None,
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

    pub fn with_media_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.media_dir = dir;
        self
    }

    pub fn with_seed_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.seed_dir = dir;
        self
    }

    /// Reads configuration from the environment, falling back to the defaults
    /// that suit a `cargo tauri dev` checkout.
    ///
    /// Every variable is optional. That matters because the desktop app does not
    /// set any of them: it builds a [`Config`] directly from `maia.json`, so the
    /// checkout keeps working exactly as before.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// [`Config::from_env`] with the variable source injected, so the resolution
    /// order can be tested without touching the process environment.
    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let var = |key: &str| lookup(key).filter(|v| !v.trim().is_empty());

        let db_path = var(env_keys::DB_PATH)
            .map(PathBuf::from)
            // Matches `get_database_path` in maia-ui: a database beside the
            // checkout, resolved relative to the working directory.
            .unwrap_or_else(|| PathBuf::from("maia.db"));

        let cfg = Self::new(db_path, var(env_keys::API_KEY).unwrap_or_else(|| "fitfat-sync-key".into()));

        let cfg = match var(env_keys::BIND_ADDR) {
            Some(addr) => cfg.with_bind_addr(addr),
            None => cfg,
        };

        let model_name = var(env_keys::MODEL_NAME).unwrap_or_else(|| "gemini-2.5-flash".into());
        let cfg = match var(env_keys::MODEL_API_KEY) {
            Some(key) => cfg.with_model(key, model_name),
            // A model name on its own changes nothing: without a key OCR stays
            // off rather than becoming a request that always fails.
            None if var(env_keys::MODEL_NAME).is_some() => cfg.with_model("", model_name),
            None => cfg,
        };

        cfg.with_media_dir(var(env_keys::MEDIA_DIR).map(PathBuf::from))
            .with_seed_dir(var(env_keys::SEED_DIR).map(PathBuf::from))
    }

    /// True when a model key was supplied, i.e. receipt OCR can run.
    pub fn ocr_enabled(&self) -> bool {
        !self.model_api_key.is_empty()
    }

    /// Directory holding exercise images and videos for `GET /media/*`.
    ///
    /// Defaults to `<db_dir>/exercise_media`.
    pub fn media_dir(&self) -> PathBuf {
        self.media_dir
            .clone()
            .unwrap_or_else(|| self.db_path.parent().unwrap_or_else(|| std::path::Path::new(".")).join("exercise_media"))
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
            media_dir: None,
            seed_dir: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Builds a lookup over a fixed variable set, so tests never mutate the
    /// process environment (which is `unsafe` to write in edition 2024 and
    /// would race across the parallel test runner anyway).
    fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn no_variables_falls_back_to_dev_defaults() {
        let cfg = Config::from_lookup(lookup(&[]));
        assert_eq!(cfg.db_path, PathBuf::from("maia.db"));
        assert_eq!(cfg.bind_addr, "0.0.0.0:3030");
        assert_eq!(cfg.api_key, "fitfat-sync-key");
        assert_eq!(cfg.media_dir, None);
        assert_eq!(cfg.seed_dir, None);
        assert_eq!(cfg.media_dir(), PathBuf::from("exercise_media"));
    }

    #[test]
    fn every_variable_is_honoured() {
        let cfg = Config::from_lookup(lookup(&[
            (env_keys::BIND_ADDR, "127.0.0.1:9999"),
            (env_keys::DB_PATH, "/var/lib/maia/maia.db"),
            (env_keys::MEDIA_DIR, "/srv/media"),
            (env_keys::SEED_DIR, "/srv/data"),
            (env_keys::API_KEY, "from-env"),
            (env_keys::MODEL_API_KEY, "  gemini-key  "),
            (env_keys::MODEL_NAME, "gemini-2.5-pro"),
        ]));
        assert_eq!(cfg.bind_addr, "127.0.0.1:9999");
        assert_eq!(cfg.db_path, PathBuf::from("/var/lib/maia/maia.db"));
        assert_eq!(cfg.api_key, "from-env");
        // An explicit media dir must win over the database-relative default.
        assert_eq!(cfg.media_dir, Some(PathBuf::from("/srv/media")));
        assert_eq!(cfg.media_dir(), PathBuf::from("/srv/media"));
        assert_eq!(cfg.seed_dir, Some(PathBuf::from("/srv/data")));
        assert_eq!(cfg.model_api_key, "gemini-key");
        assert_eq!(cfg.model_name, "gemini-2.5-pro");
        assert!(cfg.ocr_enabled());
    }

    #[test]
    fn blank_variables_fall_through_to_defaults() {
        let cfg = Config::from_lookup(lookup(&[
            (env_keys::DB_PATH, ""),
            (env_keys::API_KEY, "   "),
            (env_keys::MEDIA_DIR, ""),
        ]));
        assert_eq!(cfg.db_path, PathBuf::from("maia.db"));
        assert_eq!(cfg.api_key, "fitfat-sync-key");
        assert_eq!(cfg.media_dir, None);
    }

    #[test]
    fn model_name_alone_leaves_ocr_disabled() {
        let cfg = Config::from_lookup(lookup(&[(env_keys::MODEL_NAME, "gemini-2.5-pro")]));
        assert_eq!(cfg.model_name, "gemini-2.5-pro");
        assert!(
            !cfg.ocr_enabled(),
            "without a key OCR must stay off rather than always fail"
        );
    }

    #[test]
    fn media_dir_defaults_beside_the_database() {
        let cfg = Config::new(PathBuf::from("/var/lib/maia/maia.db"), "k".into());
        assert_eq!(cfg.media_dir(), PathBuf::from("/var/lib/maia/exercise_media"));
    }
}
