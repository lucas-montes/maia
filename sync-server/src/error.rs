use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

#[derive(Serialize)]
pub struct ErrorBody {
    pub message: String,
}

pub fn error_response(status: StatusCode, message: impl Into<String>) -> impl IntoResponse {
    let body = ErrorBody {
        message: message.into(),
    };
    (status, Json(body))
}
