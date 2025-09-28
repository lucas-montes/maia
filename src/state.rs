use ai::Model;
use shared::database::Database;

use crate::config::Config;

#[derive(Clone)]
pub struct State {
    db: Database,
    config: Config,
    model: Option<Model>,
}

impl State {
    pub fn new(db: Database, config: Config, model: Option<Model>) -> Self {
        Self { db, config, model }
    }

    pub fn database(&self) -> &Database {
        &self.db
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn model(&self) -> Option<&Model> {
        self.model.as_ref()
    }

    pub fn initialize() -> Self {
        let config = match Config::load() {
            Ok(cfg) => cfg,
            Err(err) => {
                tracing::error!(err=%err, "Failed to load config, using defaults");
                let config = Config::default();
                if let Err(e) = config.save() {
                    tracing::error!(err=%e, "Failed to save default config");
                }
                config
            }
        };
        let database = match Database::new(config.database()) {
            Ok(db) => db,
            Err(err) => {
                tracing::error!(err=%err, "Failed to open database");
                panic!("Cannot continue without a database");
            }
        };
        let model = config.model_api().map(Model::new);
        Self::new(database, config, model)
    }
}
