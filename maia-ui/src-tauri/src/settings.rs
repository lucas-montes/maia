//! The Settings screen's backend.
//!
//! - **Everything that is not a secret** is set in the host's NixOS
//!   configuration (`services.maia`) and only shown here, read from the
//!   rendered `/etc/maia/config.json`.
//! - **Secrets** (the sync key, model keys, Trading 212 credentials) never go
//!   through Nix. They live in the per-user overlay, are written here, and are
//!   only ever reported back as "set" or "not set".

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Value, json};

type Object = Map<String, Value>;

/// Envelope the webview reads, matching the other config-style commands.
#[derive(serde::Serialize)]
pub struct SettingsResult {
    success: bool,
    data: Option<Value>,
    error: Option<String>,
}

impl SettingsResult {
    fn ok(data: Value) -> Self {
        Self { success: true, data: Some(data), error: None }
    }
    fn err(error: impl std::fmt::Display) -> Self {
        Self { success: false, data: None, error: Some(error.to_string()) }
    }
}

fn deployment() -> Result<Object, String> {
    maia::Config::read_deployment(None).map_err(|e| e.to_string())
}

fn overlay_path(deployment: &Object) -> PathBuf {
    maia::config::overlay_path_from(deployment)
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

fn read_json_object(path: &Path) -> Result<Object, String> {
    let body = match std::fs::read_to_string(path) {
        Ok(body) => body,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Object::new()),
        Err(e) => return Err(format!("reading {}: {e}", path.display())),
    };
    if body.trim().is_empty() {
        return Ok(Object::new());
    }
    match serde_json::from_str(&body) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(format!("{} must contain a JSON object", path.display())),
        Err(e) => Err(format!("parsing {}: {e}", path.display())),
    }
}

/// The running configuration, in the vocabulary of the module's options, so
/// the screen can name the option that sets each value.
fn effective(deployment: &Object) -> Value {
    let cfg = maia::Config::from_values(deployment.clone(), Object::new());
    let statements_root = cfg
        .statements_dump_dir()
        .parent()
        .map(|p| p.display().to_string());
    let models: Object = cfg
        .models
        .iter()
        .map(|m| {
            (
                m.id.clone(),
                json!({
                    "provider": m.provider.as_str(),
                    "baseUrl": m.base_url.clone().unwrap_or_else(|| m.provider.default_base_url().to_string()),
                    "model": m.model,
                    "vision": m.vision,
                }),
            )
        })
        .collect();
    json!({
        "port": cfg.port(),
        "openFirewall": deployment.get("open_firewall").and_then(Value::as_bool),
        "baseCurrency": cfg.base_currency,
        "dataDir": cfg.db_path.parent().map(|p| p.display().to_string()),
        "database": cfg.db_path.display().to_string(),
        "mediaDir": cfg.media_dir().display().to_string(),
        "notesDir": cfg.notes_dir.as_ref().map(|p| p.display().to_string()),
        "archiveDir": cfg.archive_dir().display().to_string(),
        "statementsRoot": statements_root,
        "statementsIntakeDir": cfg.statements_intake_dir().map(|p| p.display().to_string()),
        "seedDir": cfg.seed_dir.as_ref().map(|p| p.display().to_string()),
        "trading212": {
            "enable": cfg.t212.enabled,
            "environment": cfg.t212.environment,
            "lookbackDays": cfg.t212.lookback_days,
        },
        "models": models,
        "tasks": {
            "receiptOcr": cfg.tasks.receipt_ocr,
            "receiptMatching": cfg.tasks.receipt_matching,
            "statementExtraction": cfg.tasks.statement_extraction,
        },
    })
}

/// Which secrets are set. Never their values.
fn secrets(overlay: &Object, deployment: &Object) -> Value {
    let is_set = |v: Option<&Value>| text(v).is_some();
    let feed = overlay
        .get("statements")
        .and_then(|s| s.get("feeds"))
        .and_then(|f| f.get("trading212"));
    let model_keys: Object = overlay
        .get("models")
        .and_then(Value::as_object)
        .map(|models| {
            models
                .iter()
                .map(|(id, m)| (id.clone(), json!(is_set(m.get("api_key")))))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "syncApiKey": is_set(overlay.get("api_key")),
        // Still answering with a key rendered by an older module: worth
        // saying, because the next rebuild drops it.
        "syncApiKeyFromNix": !is_set(overlay.get("api_key")) && is_set(deployment.get("api_key")),
        "models": model_keys,
        "trading212Key": is_set(feed.and_then(|f| f.get("api_key"))),
        "trading212Secret": is_set(feed.and_then(|f| f.get("api_secret"))),
    })
}

#[tauri::command]
pub fn read_settings() -> SettingsResult {
    let deployment = match deployment() {
        Ok(d) => d,
        Err(e) => return SettingsResult::err(e),
    };
    let overlay = read_json_object(&overlay_path(&deployment)).unwrap_or_default();
    SettingsResult::ok(json!({
        "overlayFile": overlay_path(&deployment).display().to_string(),
        "effective": effective(&deployment),
        "secrets": secrets(&overlay, &deployment),
    }))
}

// ---- Secrets ----

/// A secret update: absent keeps what is stored, `null` clears it, a string
/// replaces it. A blank string is treated as absent, so a form submitted with
/// an untouched password box never wipes a key.
#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SecretsPatch {
    #[serde(default, deserialize_with = "tri_state")]
    pub sync_api_key: Option<Option<String>>,
    /// Model id → key. Removing a model in Settings sends `null` for its key.
    #[serde(default)]
    pub models: Map<String, Value>,
    #[serde(default, deserialize_with = "tri_state")]
    pub trading212_key: Option<Option<String>>,
    #[serde(default, deserialize_with = "tri_state")]
    pub trading212_secret: Option<Option<String>>,
}

fn tri_state<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::<String>::deserialize(d)?))
}

/// What a tri-state does to one slot of [map].
fn set_secret(map: &mut Object, key: &str, update: &Option<Option<String>>) {
    match update {
        None => {}
        Some(None) => {
            map.remove(key);
        }
        Some(Some(value)) if value.trim().is_empty() => {}
        Some(Some(value)) => {
            map.insert(key.to_string(), json!(value.trim()));
        }
    }
}

fn child<'a>(map: &'a mut Object, key: &str) -> &'a mut Object {
    let entry = map.entry(key.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    entry.as_object_mut().expect("just made an object")
}

fn apply_secrets(root: &mut Object, patch: &SecretsPatch) {
    set_secret(root, "api_key", &patch.sync_api_key);
    for (id, value) in &patch.models {
        let update = match value {
            Value::Null => Some(None),
            Value::String(s) => Some(Some(s.clone())),
            _ => None,
        };
        let models = child(root, "models");
        let model = child(models, id);
        set_secret(model, "api_key", &update);
        if model.is_empty() {
            models.remove(id);
        }
    }
    if root.get("models").and_then(Value::as_object).is_some_and(Map::is_empty) {
        root.remove("models");
    }
    if patch.trading212_key.is_some() || patch.trading212_secret.is_some() {
        let feed = child(child(child(root, "statements"), "feeds"), "trading212");
        set_secret(feed, "api_key", &patch.trading212_key);
        set_secret(feed, "api_secret", &patch.trading212_secret);
    }
}

/// Writes secrets to the overlay (owner-only, atomically). Returns the new
/// set/unset flags, never values.
#[tauri::command]
pub fn save_secrets(patch: SecretsPatch) -> SettingsResult {
    let deployment = match deployment() {
        Ok(d) => d,
        Err(e) => return SettingsResult::err(e),
    };
    let path = overlay_path(&deployment);
    if let Err(e) = maia::config::update_overlay(&path, |root| apply_secrets(root, &patch)) {
        return SettingsResult::err(format!("writing {}: {e}", path.display()));
    }
    let overlay = read_json_object(&path).unwrap_or_default();
    SettingsResult::ok(secrets(&overlay, &deployment))
}

// ---- Model test ----

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModelProbe {
    pub id: String,
    pub provider: String,
    #[serde(default)]
    pub base_url: Option<String>,
    pub model: String,
    /// A key typed but not saved yet; else the stored one for [id] is used.
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Sends one tiny structured request to a configured model,
/// with a key typed but not saved yet if there is one, and reports how long
/// it took or why it failed.
#[tauri::command]
pub async fn test_model(probe: ModelProbe) -> SettingsResult {
    let Some(provider) = maia::ai::Provider::parse(&probe.provider) else {
        return SettingsResult::err(format!("unknown provider {}", probe.provider));
    };
    let stored = deployment()
        .ok()
        .map(|d| read_json_object(&overlay_path(&d)).unwrap_or_default())
        .and_then(|o| text(o.get("models").and_then(|m| m.get(&probe.id)).and_then(|m| m.get("api_key"))));
    let api_key = probe
        .api_key
        .filter(|k| !k.trim().is_empty())
        .or(stored)
        .unwrap_or_default();
    let model = maia::ai::Model::from_spec(&maia::ai::ModelSpec {
        id: probe.id,
        provider,
        base_url: probe.base_url.filter(|u| !u.trim().is_empty()),
        model: probe.model,
        api_key,
        vision: false,
    });
    let started = std::time::Instant::now();
    let answer = model
        .generate_json(
            "You are a connectivity check.".into(),
            "Answer with {\"ok\": true}.".into(),
            json!({ "type": "object", "properties": { "ok": { "type": "boolean" } }, "required": ["ok"] }),
        )
        .await;
    let latency_ms = started.elapsed().as_millis() as u64;
    match answer {
        Ok(Some(_)) => SettingsResult::ok(json!({ "ok": true, "latencyMs": latency_ms })),
        Ok(None) => SettingsResult::ok(json!({
            "ok": false, "latencyMs": latency_ms,
            "error": "it answered, but not with JSON — it may not support structured output",
        })),
        Err(e) => SettingsResult::ok(json!({ "ok": false, "latencyMs": latency_ms, "error": e.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(value: Value) -> Object {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn secrets_set_keep_and_clear() {
        let mut root = obj(json!({
            "api_key": "old-sync",
            "models": { "a": { "api_key": "ka" }, "b": { "api_key": "kb" } },
            "statements": { "feeds": { "trading212": { "api_key": "tk", "api_secret": "ts" } } },
            "unrelated": true
        }));
        let patch: SecretsPatch = serde_json::from_value(json!({
            "syncApiKey": "  new-sync  ",
            "models": { "a": "", "b": null, "c": "kc" },
            "trading212Secret": null
        }))
        .unwrap();
        apply_secrets(&mut root, &patch);
        assert_eq!(root["api_key"], "new-sync");
        assert_eq!(root["models"]["a"]["api_key"], "ka", "a blank box keeps the stored key");
        assert!(root["models"].get("b").is_none(), "null clears it, and an empty entry goes");
        assert_eq!(root["models"]["c"]["api_key"], "kc");
        assert_eq!(root["statements"]["feeds"]["trading212"], json!({ "api_key": "tk" }));
        assert_eq!(root["unrelated"], true);

        let flags = secrets(&root, &Object::new());
        assert_eq!(flags["syncApiKey"], true);
        assert_eq!(flags["models"], json!({ "a": true, "c": true }));
        assert_eq!(flags["trading212Key"], true);
        assert_eq!(flags["trading212Secret"], false);
        assert!(!flags.to_string().contains("new-sync"), "values are never reported");
    }

    #[test]
    fn effective_speaks_the_module_vocabulary() {
        let deployment = obj(json!({
            "database": "/var/lib/maia/maia.db",
            "bind_addr": "0.0.0.0:3031",
            "open_firewall": true,
            "statements": { "dump_dir": "/home/u/.local/share/maia/statements/dump", "intake_dir": "/home/u/in" },
            "models": { "flash": { "provider": "gemini", "model": "gemini-2.5-flash", "vision": true } },
            "tasks": { "receipt_ocr": ["flash"] }
        }));
        let e = effective(&deployment);
        assert_eq!(e["port"], 3031);
        assert_eq!(e["openFirewall"], true);
        assert_eq!(e["dataDir"], "/var/lib/maia");
        assert_eq!(e["statementsRoot"], "/home/u/.local/share/maia/statements");
        assert_eq!(e["statementsIntakeDir"], "/home/u/in");
        assert_eq!(e["models"]["flash"]["baseUrl"], "https://generativelanguage.googleapis.com/v1beta");
        assert_eq!(e["tasks"]["receiptOcr"], json!(["flash"]));
    }
}
