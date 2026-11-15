//! Application state for Ember relay server.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ember_protocol::{DEFAULT_MAX_CIPHERTEXT_BYTES, MAX_LIFETIME_SECS, MAX_PART_SIZE};

use crate::pow::{PowManager, DEFAULT_POW_DIFFICULTY};
use crate::rate_limit::{RateLimitConfig, RateLimiter};
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
    pub pow: Arc<PowManager>,
    pub rate_limiter: Arc<RateLimiter>,
}

impl AppState {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            max_total_bytes: DEFAULT_MAX_CIPHERTEXT_BYTES,
            max_part_bytes: MAX_PART_SIZE as u64,
            max_ttl_secs: MAX_LIFETIME_SECS,
            upload_timeout_secs: 3600, // 1 hour for uncompleted uploads
            claim_window_secs: 600,    // 10 minutes claim window
            pow: Arc::new(PowManager::new(DEFAULT_POW_DIFFICULTY)),
            rate_limiter: Arc::new(RateLimiter::new(RateLimitConfig::default())),
        }
    }

    pub fn with_pow_and_rate_limit(
        storage: Arc<dyn Storage>,
        difficulty: u32,
        rate_limit_config: RateLimitConfig,
    ) -> Self {
        Self {
            storage,
            max_total_bytes: DEFAULT_MAX_CIPHERTEXT_BYTES,
            max_part_bytes: MAX_PART_SIZE as u64,
            max_ttl_secs: MAX_LIFETIME_SECS,
            upload_timeout_secs: 3600,
            claim_window_secs: 600,
            pow: Arc::new(PowManager::new(difficulty)),
            rate_limiter: Arc::new(RateLimiter::new(rate_limit_config)),
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
