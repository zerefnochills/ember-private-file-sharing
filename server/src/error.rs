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

    #[error("Proof of work required: {0}")]
    PowRequired(String),

    #[error("Invalid proof of work: {0}")]
    PowInvalid(String),

    #[error("Proof of work challenge expired: {0}")]
    PowExpired(String),

    #[error("Proof of work challenge already consumed")]
    PowReplayed,

    #[error("Rate limit exceeded")]
    RateLimited { retry_after_secs: u64 },
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
            ServerError::PowRequired(_) => (StatusCode::BAD_REQUEST, "pow_required"),
            ServerError::PowInvalid(_) => (StatusCode::BAD_REQUEST, "invalid_pow"),
            ServerError::PowExpired(_) => (StatusCode::BAD_REQUEST, "challenge_expired"),
            ServerError::PowReplayed => (StatusCode::BAD_REQUEST, "pow_replayed"),
            ServerError::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
        };

        let mut res = match &self {
            ServerError::RateLimited { retry_after_secs } => {
                let body = Json(json!({
                    "error": code,
                    "message": self.to_string(),
                    "retry_after": retry_after_secs,
                }));
                let mut response = (status, body).into_response();
                response.headers_mut().insert(
                    axum::http::header::RETRY_AFTER,
                    retry_after_secs.to_string().parse().unwrap(),
                );
                response
            }
            _ => {
                let body = Json(json!({
                    "error": code,
                    "message": self.to_string(),
                }));
                (status, body).into_response()
            }
        };

        res.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
        res
    }
}

impl From<ember_core::EmberError> for ServerError {
    fn from(err: ember_core::EmberError) -> Self {
        ServerError::BadRequest(err.to_string())
    }
}
