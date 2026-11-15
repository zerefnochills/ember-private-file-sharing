//! Application state for Ember relay server.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ember_protocol::{DEFAULT_MAX_CIPHERTEXT_BYTES, MAX_LIFETIME_SECS, MAX_PART_SIZE};

use crate::storage::Storage;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub max_total_bytes: u64,
    pub max_part_bytes: u64,
    pub max_ttl_secs: u64,
    pub upload_timeout_secs: u64,
    pub claim_window_secs: u64,
}

impl AppState {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            max_total_bytes: DEFAULT_MAX_CIPHERTEXT_BYTES,
            max_part_bytes: MAX_PART_SIZE as u64,
            max_ttl_secs: MAX_LIFETIME_SECS,
            upload_timeout_secs: 3600, // 1 hour for uncompleted uploads
            claim_window_secs: 600,   // 10 minutes claim window
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
