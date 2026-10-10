//! HTTP client for the sync server.
//!
//! Writes leave this process entirely. The app opens the database read-only, so
//! anything it changes has to be asked of the server, which is the only writer.
//!
//! Going through Rust rather than the webview's `fetch` matters: the webview
//! runs under an origin the server's CORS layer has to be told about, and
//! `http://tauri.localhost` is not something a loopback service should have to
//! accommodate. From here the request is an ordinary local HTTP call.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// How long a write may take before it is reported as a failure.
///
/// Generous, because receipt archiving moves files and the OCR upload is not
/// used from here, but bounded: a request that hangs forever leaves the user
/// watching a spinner with no explanation.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Why a write failed, phrased for someone looking at the app.
///
/// The server reports errors in three different shapes -- a JSON `{"message"}`
/// from its own helpers, a bare string from axum's extractors, and an empty
/// body -- so they are flattened here rather than at every call site.
#[derive(Debug)]
pub enum ApiError {
    /// The server could not be reached at all.
    ///
    /// Almost always means `maia.service` is not running.
    Unreachable(String),
    /// The server answered, but with an error status.
    Status { code: u16, message: String },
}

impl ApiError {
    pub fn to_message(&self) -> String {
        match self {
            ApiError::Unreachable(detail) => format!(
                "Could not reach the sync server. Is maia.service running? ({detail})"
            ),
            ApiError::Status { code, message } => format!("Sync server returned {code}: {message}"),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_message())
    }
}

fn client() -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        // The server is on loopback; a proxy configured for the desktop would
        // only get in the way of reaching it.
        .no_proxy()
        .build()
        .map_err(|e| ApiError::Unreachable(e.to_string()))
}

/// Pulls the `message` out of whatever the server sent.
///
/// An error status with a body that is not the expected JSON still has to say
/// something useful, so a raw body is passed through rather than discarded.
fn error_message(status: u16, body: &str) -> ApiError {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        // Some failures genuinely have no body, and "returned 500:" with
        // nothing after it reads like a rendering bug rather than an error.
        return ApiError::Status {
            code: status,
            message: "no detail given".to_string(),
        };
    }
    let message = if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        match json.get("message").and_then(|m| m.as_str()) {
            Some(m) => m.to_string(),
            // A JSON error without a `message` key is still better than nothing.
            None => trimmed.to_string(),
        }
    } else {
        // axum's extractor rejections are plain text.
        trimmed.to_string()
    };
    ApiError::Status { code: status, message }
}

/// POSTs [body] to [path] relative to the configured server and returns the
/// response, which every push endpoint answers with a small JSON object.
pub async fn post<B: Serialize, R: DeserializeOwned>(
    path: &str,
    body: &B,
) -> Result<R, ApiError> {
    let base = server_base();
    let url = format!("{base}{path}");
    let api_key = sync_server::Config::shared(None)
        .map(|c| c.api_key.clone())
        .unwrap_or_default();

    let response = client()?
        .post(&url)
        .bearer_auth(api_key)
        .json(body)
        .send()
        .await
        .map_err(|e| ApiError::Unreachable(e.to_string()))?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(error_message(status.as_u16(), &text));
    }

    serde_json::from_str(&text).map_err(|e| {
        ApiError::Status {
            code: status.as_u16(),
            message: format!("could not read the response: {e}"),
        }
    })
}

/// The server's base URL, always on loopback.
///
/// The config records a bind address rather than a URL, and a bind address is
/// not always a valid destination -- `0.0.0.0` is a wildcard to listen on, not
/// somewhere to connect to -- so the host is replaced here rather than at each
/// call site.
fn server_base() -> String {
    let port = sync_server::Config::shared(None)
        .map(|c| c.port())
        .unwrap_or(sync_server::config::DEFAULT_PORT);
    format!("http://127.0.0.1:{port}")
}

/// Whether the server is up, for the status line in the UI.
///
/// Async rather than `reqwest::blocking`, which would need a feature flag and
/// a runtime of its own inside a process that already has one.
pub async fn is_reachable() -> bool {
    match client() {
        Ok(c) => c
            .get(format!("{}/health", server_base()))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_error_bodies_yield_their_message() {
        let err = error_message(409, r#"{"message":"Receipt is already archived"}"#);
        assert!(matches!(&err, ApiError::Status { code: 409, message } if message == "Receipt is already archived"));
    }

    /// axum's extractors reject with plain text, not the app's JSON envelope.
    #[test]
    fn plain_text_error_bodies_are_kept() {
        let err = error_message(415, "Expected request with `Content-Type: application/json`");
        match err {
            ApiError::Status { code, message } => {
                assert_eq!(code, 415);
                assert!(message.contains("Content-Type"));
            }
            _ => panic!("expected a status error"),
        }
    }

    /// An error with no body at all must still produce something showable.
    #[test]
    fn empty_error_bodies_do_not_render_as_nothing() {
        match error_message(500, "") {
            ApiError::Status { code, message } => {
                assert_eq!(code, 500);
                assert!(!message.is_empty());
            }
            _ => panic!("expected a status error"),
        }
    }

    #[test]
    fn json_without_a_message_key_still_says_something() {
        match error_message(400, r#"{"detail":"nope"}"#) {
            ApiError::Status { message, .. } => assert!(message.contains("nope")),
            _ => panic!("expected a status error"),
        }
    }

    /// A bind address is not a destination; the loopback host has to replace it.
    #[test]
    fn the_base_url_never_uses_the_wildcard_bind_address() {
        let base = server_base();
        assert!(base.starts_with("http://127.0.0.1:"), "got {base}");
    }

    /// The message has to name the service, since "could not connect" on its
    /// own gives the user nothing to act on.
    #[test]
    fn unreachable_errors_mention_the_service() {
        let message = ApiError::Unreachable("connection refused".into()).to_message();
        assert!(message.contains("maia.service"), "got {message}");
    }
}