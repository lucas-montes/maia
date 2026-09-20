use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::server::AppState;

#[derive(Serialize)]
struct ErrorBody {
    message: String,
}

/// Exercise media files (`GET /media/*`, `GET /exercises/*.{jpg,mp4}`) are
/// shared catalog assets, not user data, so plain `<img>` / `<video>` tags
/// (which cannot send an `Authorization` header) may fetch them without a
/// Bearer key. Every JSON API route stays protected.
fn is_public_media(method: &axum::http::Method, path: &str) -> bool {
    if *method != axum::http::Method::GET {
        return false;
    }
    if path.starts_with("/media/") {
        return true;
    }
    if let Some(rest) = path.strip_prefix("/exercises/") {
        return rest.ends_with(".jpg") || rest.ends_with(".mp4");
    }
    false
}

pub async fn bearer_auth(State(state): State<AppState>, req: Request<Body>, next: Next) -> Response {
    if req.uri().path() == "/health" {
        return next.run(req).await;
    }
    if is_public_media(req.method(), req.uri().path()) {
        return next.run(req).await;
    }
    let Some(auth) = req.headers().get("authorization") else {
        return unauthorized("Missing Authorization header");
    };
    let Ok(value) = auth.to_str() else {
        return unauthorized("Invalid Authorization header");
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return unauthorized("Invalid Authorization scheme");
    };
    if token != state.config.api_key {
        return unauthorized("Invalid API key");
    }
    next.run(req).await
}

fn unauthorized(msg: &str) -> Response {
    let body = ErrorBody {
        message: msg.to_string(),
    };
    (StatusCode::UNAUTHORIZED, Json(body)).into_response()
}
