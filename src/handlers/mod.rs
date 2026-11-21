pub mod accounts;
pub mod positions;
pub mod agenda;
pub mod archive;
pub mod backup;
pub mod finance_scenarios;
pub mod health;
pub mod lookup;
pub mod matching;
pub mod media;
pub mod pull;
pub mod push;
pub mod read;
pub mod receipt_ocr;
pub mod receipts;
pub mod statements;

/// The models a task tried before the one that answered, for a response body:
/// `[{ "model": "<id> (<name>)", "error": "…" }]`. Empty when the first model
/// answered.
pub fn attempts_json(attempts: &[ai::Attempt]) -> serde_json::Value {
    serde_json::Value::Array(
        attempts
            .iter()
            .map(|a| serde_json::json!({ "model": a.model.to_string(), "error": a.error }))
            .collect(),
    )
}
