//! Ember relay server library.
//!
//! Exposes router, storage implementations, and sweeper for integration testing and the server binary.

pub mod access_log;
pub mod error;
pub mod model;
pub mod pow;
pub mod rate_limit;
pub mod routes;
pub mod state;
pub mod storage;
pub mod sweeper;

use axum::extract::DefaultBodyLimit;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};

pub use access_log::{sanitize_route_template, size_bucket, AccessLogEntry, AccessLogRecorder};
pub use error::ServerError;
pub use model::{
    AdminTakedownRequest, BlobMetadataResponse, BlobRecord, BlobState, ClaimResponse,
    CreateBlobRequest, CreateBlobResponse, CreateMailboxRequest, CreateMailboxResponse,
    GetMessagesResponse, MailboxMessageItem, MailboxMetadataResponse, MailboxRecord,
    PowChallengeResponse, PowSolutionPayload, PushMessageRequest, PushMessageResponse,
    ReportRecord, SubmitReportRequest, SubmitReportResponse, TakedownRecord, TransparencyStats,
};
pub use pow::PowManager;
pub use rate_limit::{RateLimitConfig, RateLimiter};
pub use state::{now_secs, AppState};
pub use storage::fs::FilesystemStorage;
pub use storage::mem::MemoryStorage;
pub use storage::Storage;

/// Builds the unified Axum application router (public + admin) with CORS and no-store headers.
pub fn app(state: AppState) -> Router {
    wrap_middleware(routes::router(state.clone()), state)
}

/// Builds the public-only Axum router (does NOT mount any admin routes).
pub fn public_app(state: AppState) -> Router {
    wrap_middleware(routes::public_router(state.clone()), state)
}

/// Builds the dedicated admin-only Axum router (mounts only /v1/admin/* routes).
pub fn admin_app(state: AppState) -> Router {
    wrap_middleware(routes::admin_router(state.clone()), state)
}

fn wrap_middleware(router: Router, state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let access_logger = state.access_logger.clone();
    let trusted_proxies = state.trusted_proxies.clone();

    router
        .layer(axum::middleware::from_fn(move |req, next| {
            let logger = access_logger.clone();
            let proxies = trusted_proxies.clone();
            access_log::access_log_middleware(logger, proxies, req, next)
        }))
        .layer(DefaultBodyLimit::max(1_500_000))
        .layer(cors)
}
