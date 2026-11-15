//! Ember relay server library.
//!
//! Exposes router, storage implementations, and sweeper for integration testing and the server binary.

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

pub use error::ServerError;
pub use model::{
    BlobMetadataResponse, BlobRecord, BlobState, ClaimResponse, CreateBlobRequest,
    CreateBlobResponse, PowChallengeResponse, PowSolutionPayload,
};
pub use pow::PowManager;
pub use rate_limit::{RateLimitConfig, RateLimiter};
pub use state::AppState;
pub use storage::fs::FilesystemStorage;
pub use storage::mem::MemoryStorage;
pub use storage::Storage;

/// Builds the Axum application router with CORS and no-store headers.
pub fn app(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    routes::router(state)
        .layer(DefaultBodyLimit::max(1_500_000))
        .layer(cors)
}
