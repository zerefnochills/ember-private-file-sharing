//! Ember relay server library.
//!
//! Exposes router, storage implementations, and sweeper for integration testing and the server binary.

pub mod error;
pub mod model;
pub mod routes;
pub mod state;
pub mod storage;
pub mod sweeper;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};

pub use error::ServerError;
pub use model::{
    BlobMetadataResponse, BlobRecord, BlobState, ClaimResponse, CreateBlobRequest,
    CreateBlobResponse,
};
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

    routes::router(state).layer(cors)
}
