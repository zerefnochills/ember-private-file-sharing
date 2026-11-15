//! Error types and HTTP responses for Ember relay server.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Not found")]
    NotFound,

    #[error("Expired or burned")]
    Gone,

    #[error("Unauthorized: invalid token")]
    Unauthorized,

    #[error("Conflict: blob already claimed")]
    Conflict(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Payload too large: {0}")]
    PayloadTooLarge(String),

    #[error("Internal storage error: {0}")]
    Storage(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ServerError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ServerError::Gone => (StatusCode::GONE, "gone"),
            ServerError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ServerError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ServerError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ServerError::PayloadTooLarge(_) => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
            ServerError::Storage(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        let body = Json(json!({
            "error": code,
            "message": self.to_string(),
        }));

        (status, body).into_response()
    }
}

impl From<ember_core::EmberError> for ServerError {
    fn from(err: ember_core::EmberError) -> Self {
        ServerError::BadRequest(err.to_string())
    }
}
