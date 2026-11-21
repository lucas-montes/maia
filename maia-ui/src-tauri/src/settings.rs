//! The Settings screen's backend.
//!
//! Two stores, split by what a value is:
//!
//! - **Everything that is not a secret** belongs to Nix. The host's flake
//!   imports a JSON file into `services.maia` (`settingsFile` in the module),
//!   and these commands edit that file. Nothing applies until the user runs
//!   the rebuild command we hand back, so the screen compares the file
//!   ("pending") against the rendered `/etc/maia/config.json` ("effective").
//! - **Secrets** (the sync key, model keys, Trading 212 credentials) never go
//!   through Nix. They live in the per-user overlay, are written here, and are
//!   only ever reported back as "set" or "not set".

use std::path::{Path, PathBuf};
use std::process::Command;

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

fn settings_file(deployment: &Object) -> Option<PathBuf> {
    text(deployment.get("nix_settings_file")).map(PathBuf::from)
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

/// The effective configuration, in the vocabulary of the module's options, so
/// the screen can compare it key by key with the pending file.
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

/// `sudo nixos-rebuild switch --flake <repo>#<host>` for the repository that
/// holds [file], and whether git tracks it (a pure flake ignores it if not).
fn rebuild_hint(file: &Path, host: Option<&str>) -> (String, bool) {
    let dir = file.parent().unwrap_or(Path::new("."));
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let root = git(&["rev-parse", "--show-toplevel"]).unwrap_or_else(|| dir.display().to_string());
    let tracked = file
        .file_name()
        .and_then(|name| git(&["ls-files", "--error-unmatch", &name.to_string_lossy()]))
        .is_some();
    let host = host
        .map(str::to_owned)
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok().map(|h| h.trim().to_string()))
        .unwrap_or_else(|| "$(hostname)".into());
    (format!("sudo nixos-rebuild switch --flake {root}#{host}"), tracked)
}

#[tauri::command]
pub fn read_settings() -> SettingsResult {
    let deployment = match deployment() {
        Ok(d) => d,
        Err(e) => return SettingsResult::err(e),
    };
    let overlay = read_json_object(&overlay_path(&deployment)).unwrap_or_default();
    let file = settings_file(&deployment);
    let (pending, pending_error) = match &file {
        Some(path) => match read_json_object(path) {
            Ok(map) => (Value::Object(map), None),
            Err(e) => (Value::Null, Some(e)),
        },
        None => (Value::Null, None),
    };
    let (rebuild_command, tracked) = file
        .as_deref()
        .map(|f| rebuild_hint(f, text(deployment.get("nixos_host")).as_deref()))
        .map(|(cmd, tracked)| (Some(cmd), tracked))
        .unwrap_or((None, false));
    SettingsResult::ok(json!({
        "settingsFile": file.as_ref().map(|p| p.display().to_string()),
        "settingsFileTracked": tracked,
        "rebuildCommand": rebuild_command,
        "overlayFile": overlay_path(&deployment).display().to_string(),
        "effective": effective(&deployment),
        "pending": pending,
        "pendingError": pending_error,
        "secrets": secrets(&overlay, &deployment),
    }))
}

// ---- Nix settings ----

const MODEL_PROVIDERS: &[&str] = &["gemini", "openai"];
const PATH_KEYS: &[&str] = &["mediaDir", "notesDir", "archiveDir"];
const OPTIONAL_PATH_KEYS: &[&str] = &["statementsRoot", "statementsIntakeDir", "seedDir"];
const TASK_KEYS: &[&str] = &["receiptOcr", "receiptMatching", "statementExtraction"];

fn absolute_path(key: &str, value: &Value) -> Result<(), String> {
    match value.as_str() {
        Some(p) if p.starts_with('/') && !p.contains('\n') => Ok(()),
        _ => Err(format!("{key} must be an absolute path")),
    }
}

fn valid_model_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Checks one top-level key of a patch the way the module's option types
/// would, so a bad value is refused here instead of failing the rebuild.
/// `null` always passes: it removes the key, putting the module default back.
fn validate_key(key: &str, value: &Value) -> Result<(), String> {
    if value.is_null() {
        return Ok(());
    }
    match key {
        "port" => match value.as_u64() {
            Some(1..=65535) => Ok(()),
            _ => Err("port must be between 1 and 65535".into()),
        },
        "openFirewall" => value.as_bool().map(|_| ()).ok_or("openFirewall must be true or false".into()),
        "baseCurrency" => match value.as_str() {
            Some(c) if c.len() == 3 && c.chars().all(|ch| ch.is_ascii_alphabetic()) => Ok(()),
            _ => Err("baseCurrency must be a three-letter code such as EUR".into()),
        },
        k if PATH_KEYS.contains(&k) || OPTIONAL_PATH_KEYS.contains(&k) => absolute_path(k, value),
        "trading212" => {
            let feed = value.as_object().ok_or("trading212 must be an object")?;
            for (k, v) in feed {
                match (k.as_str(), v) {
                    (_, Value::Null) => {}
                    ("enable", Value::Bool(_)) => {}
                    ("environment", Value::String(e)) if e == "live" || e == "demo" => {}
                    ("lookbackDays", v) if v.as_i64().is_some_and(|d| (1..=3650).contains(&d)) => {}
                    ("enable", _) => return Err("trading212.enable must be true or false".into()),
                    ("environment", _) => return Err("trading212.environment must be live or demo".into()),
                    ("lookbackDays", _) => return Err("trading212.lookbackDays must be between 1 and 3650".into()),
                    (other, _) => return Err(format!("trading212.{other} is not a setting")),
                }
            }
            Ok(())
        }
        "models" => {
            let models = value.as_object().ok_or("models must be an object")?;
            for (id, model) in models {
                if !valid_model_id(id) {
                    return Err(format!("model id `{id}`: letters, digits, '-', '_' and '.' only"));
                }
                let model = model.as_object().ok_or(format!("model `{id}` must be an object"))?;
                for (k, v) in model {
                    match (k.as_str(), v) {
                        ("provider", Value::String(p)) if MODEL_PROVIDERS.contains(&p.as_str()) => {}
                        ("provider", _) => return Err(format!("model `{id}`: provider must be gemini or openai")),
                        ("baseUrl", Value::String(u)) if u.starts_with("http://") || u.starts_with("https://") => {}
                        ("baseUrl", Value::Null) => {}
                        ("baseUrl", _) => return Err(format!("model `{id}`: base URL must start with http:// or https://")),
                        ("model", Value::String(m)) if !m.trim().is_empty() => {}
                        ("model", _) => return Err(format!("model `{id}`: the model name is required")),
                        ("vision", Value::Bool(_)) => {}
                        ("vision", _) => return Err(format!("model `{id}`: vision must be true or false")),
                        (other, _) => return Err(format!("model `{id}`: `{other}` is not a setting")),
                    }
                }
                if !model.contains_key("model") {
                    return Err(format!("model `{id}`: the model name is required"));
                }
            }
            Ok(())
        }
        "tasks" => {
            let tasks = value.as_object().ok_or("tasks must be an object")?;
            for (k, v) in tasks {
                if !TASK_KEYS.contains(&k.as_str()) {
                    return Err(format!("tasks.{k} is not a task"));
                }
                let ok = v.is_null()
                    || v.as_array().is_some_and(|ids| ids.iter().all(|id| id.as_str().is_some_and(valid_model_id)));
                if !ok {
                    return Err(format!("tasks.{k} must be a list of model ids"));
                }
            }
            Ok(())
        }
        other => Err(format!("`{other}` is not editable from Settings")),
    }
}

/// Applies [patch] to [current]. Objects one level down (`trading212`,
/// `tasks`) merge key by key; `models` is replaced whole so a deleted model
/// goes away; `null` anywhere removes the key.
fn apply_patch(mut current: Object, patch: Object) -> Object {
    for (key, value) in patch {
        match (key.as_str(), value) {
            (_, Value::Null) => {
                current.remove(&key);
            }
            ("trading212" | "tasks", Value::Object(incoming)) => {
                let mut merged = current
                    .get(&key)
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                for (k, v) in incoming {
                    if v.is_null() {
                        merged.remove(&k);
                    } else {
                        merged.insert(k, v);
                    }
                }
                if merged.is_empty() {
                    current.remove(&key);
                } else {
                    current.insert(key, Value::Object(merged));
                }
            }
            (_, value) => {
                current.insert(key, value);
            }
        }
    }
    current
}

/// What the module would refuse about the file as a whole: tasks naming
/// models that do not exist, and OCR naming a model that cannot see.
fn validate_whole(settings: &Object) -> Result<(), String> {
    let models = settings.get("models").and_then(Value::as_object);
    let tasks = settings.get("tasks").and_then(Value::as_object);
    for task in TASK_KEYS {
        let ids = tasks
            .and_then(|t| t.get(*task))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for id in ids.iter().filter_map(Value::as_str) {
            let Some(model) = models.and_then(|m| m.get(id)) else {
                return Err(format!("tasks.{task} names `{id}`, which is not a saved model"));
            };
            if *task == "receiptOcr" && model.get("vision").and_then(Value::as_bool) != Some(true) {
                return Err(format!("`{id}` cannot read images, so it cannot serve receipt OCR"));
            }
        }
    }
    Ok(())
}

fn write_atomically(path: &Path, value: &Object) -> std::io::Result<()> {
    let mut body = serde_json::to_string_pretty(&Value::Object(value.clone()))?;
    body.push('\n');
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, body)?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

fn save_nix_settings_at(path: &Path, patch: Object) -> Result<Object, String> {
    for (key, value) in &patch {
        validate_key(key, value)?;
    }
    let current = read_json_object(path)?;
    let next = apply_patch(current, patch);
    validate_whole(&next)?;
    write_atomically(path, &next).map_err(|e| format!("writing {}: {e}", path.display()))?;
    Ok(next)
}

/// Edits the Nix settings file. Returns the file as written.
#[tauri::command]
pub fn save_nix_settings(patch: Value) -> SettingsResult {
    let Value::Object(patch) = patch else {
        return SettingsResult::err("settings must be a JSON object");
    };
    let deployment = match deployment() {
        Ok(d) => d,
        Err(e) => return SettingsResult::err(e),
    };
    let Some(path) = settings_file(&deployment) else {
        return SettingsResult::err(
            "no settings file: set services.maia.settingsFile in your NixOS configuration and rebuild",
        );
    };
    match save_nix_settings_at(&path, patch) {
        Ok(written) => SettingsResult::ok(Value::Object(written)),
        Err(e) => SettingsResult::err(e),
    }
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

/// Sends one tiny structured request to a model as the form describes it,
/// which need not be rebuilt yet, and reports how long it took or why it
/// failed.
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

    fn temp_file(name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("maia-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn values_are_checked_like_the_module_types() {
        assert!(validate_key("port", &json!(3030)).is_ok());
        assert!(validate_key("port", &json!(0)).is_err());
        assert!(validate_key("port", &json!(70000)).is_err());
        assert!(validate_key("baseCurrency", &json!("usd")).is_ok());
        assert!(validate_key("baseCurrency", &json!("EURO")).is_err());
        assert!(validate_key("mediaDir", &json!("relative/dir")).is_err());
        assert!(validate_key("statementsIntakeDir", &json!("/home/lucas/Downloads")).is_ok());
        assert!(validate_key("trading212", &json!({ "environment": "staging" })).is_err());
        assert!(validate_key("trading212", &json!({ "lookbackDays": 4000 })).is_err());
        assert!(validate_key("models", &json!({ "bad id": { "model": "x" } })).is_err());
        assert!(validate_key("models", &json!({ "ok": { "provider": "openai", "baseUrl": "ftp://x", "model": "m" } })).is_err());
        assert!(validate_key("models", &json!({ "ok": { "provider": "openai" } })).is_err(), "model is required");
        assert!(validate_key("tasks", &json!({ "receiptOcr": ["a", "b"] })).is_ok());
        assert!(validate_key("tasks", &json!({ "somethingElse": [] })).is_err());
        assert!(validate_key("dataDir", &json!("/srv")).is_err(), "the state directory is not editable here");
        assert!(validate_key("apiKey", &json!("x")).is_err(), "a secret never goes to Nix");
        assert!(validate_key("port", &Value::Null).is_ok(), "null resets to the default");
    }

    #[test]
    fn a_save_merges_into_the_file_and_keeps_what_it_did_not_touch() {
        let path = temp_file(
            "maia-settings.json",
            r#"{ "port": 3030, "custom": "kept", "trading212": { "enable": true, "lookbackDays": 90 },
                 "models": { "old": { "model": "x" } }, "tasks": { "receiptOcr": [], "receiptMatching": ["old"] } }"#,
        );
        let written = save_nix_settings_at(
            &path,
            obj(json!({
                "port": 4040,
                "trading212": { "lookbackDays": 30 },
                "models": { "flash": { "provider": "gemini", "model": "gemini-2.5-flash", "vision": true } },
                "tasks": { "receiptMatching": ["flash"], "receiptOcr": ["flash"] },
                "baseCurrency": null
            })),
        )
        .unwrap();
        let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk, Value::Object(written));
        assert_eq!(on_disk["port"], 4040);
        assert_eq!(on_disk["custom"], "kept");
        assert_eq!(on_disk["trading212"], json!({ "enable": true, "lookbackDays": 30 }));
        assert!(on_disk["models"].get("old").is_none(), "models are replaced whole, so a removal sticks");
        assert_eq!(on_disk["tasks"]["receiptMatching"], json!(["flash"]));
        assert!(std::fs::read_to_string(&path).unwrap().ends_with("}\n"));
    }

    #[test]
    fn a_save_is_refused_when_a_task_names_a_missing_or_blind_model() {
        let path = temp_file("s.json", r#"{ "models": { "text": { "provider": "openai", "model": "m" } } }"#);
        let missing = save_nix_settings_at(&path, obj(json!({ "tasks": { "receiptMatching": ["nope"] } })));
        assert!(missing.unwrap_err().contains("not a saved model"));
        let blind = save_nix_settings_at(&path, obj(json!({ "tasks": { "receiptOcr": ["text"] } })));
        assert!(blind.unwrap_err().contains("cannot read images"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{ "models": { "text": { "provider": "openai", "model": "m" } } }"#,
            "a refused save leaves the file alone"
        );
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
        assert_eq!(e["models"]["flash"]["baseUrl"], "https://generativelanguage.googleapis.com/v1beta/models");
        assert_eq!(e["tasks"]["receiptOcr"], json!(["flash"]));
    }
}
