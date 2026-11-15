//! Coarse, privacy-preserving in-memory IP rate limiter.
//!
//! Stores short-lived request counters in memory only (never written to disk or logs).
//! IPv6 addresses are truncated to `/64` prefixes to prevent address-rotation abuse.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::error::ServerError;

/// Truncated IP key for coarse rate limiting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IpKey {
    V4([u8; 4]),
    V6Prefix([u8; 8]),
}

impl From<IpAddr> for IpKey {
    fn from(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(v4) => IpKey::V4(v4.octets()),
            IpAddr::V6(v6) => {
                let octets = v6.octets();
                let mut prefix = [0u8; 8];
                prefix.copy_from_slice(&octets[0..8]);
                IpKey::V6Prefix(prefix)
            }
        }
    }
}

/// Short-lived counter bucket for an IP key.
#[derive(Clone, Debug)]
struct IpBucket {
    count: u32,
    window_start: u64,
}

/// Rate limit configuration.
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    /// Maximum allowed requests per window.
    pub max_requests: u32,
    /// Window duration in seconds (target 15 minutes = 900s).
    pub window_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 60,
            window_secs: 900, // 15 minutes
        }
    }
}

/// Thread-safe in-memory rate limiter.
pub struct RateLimiter {
    max_requests: AtomicU32,
    window_secs: AtomicU64,
    entries: Mutex<HashMap<IpKey, IpBucket>>,
}

impl RateLimiter {
    /// Creates a new `RateLimiter` with the given configuration.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            max_requests: AtomicU32::new(config.max_requests),
            window_secs: AtomicU64::new(config.window_secs),
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Sets limit parameters dynamically (useful for test cases).
    pub fn set_limits(&self, max_requests: u32, window_secs: u64) {
        self.max_requests.store(max_requests, Ordering::SeqCst);
        self.window_secs.store(window_secs, Ordering::SeqCst);
    }

    /// Checks if a request from `ip` is allowed, and increments its counter.
    ///
    /// If the counter exceeds the configured limit within the window,
    /// returns `ServerError::RateLimited { retry_after_secs }`.
    pub async fn check_and_record(&self, ip: IpAddr) -> Result<(), ServerError> {
        let key = IpKey::from(ip);
        let max_reqs = self.max_requests.load(Ordering::SeqCst);
        let window_secs = self.window_secs.load(Ordering::SeqCst);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();

        let mut map = self.entries.lock().await;

        // Periodic eviction of expired windows
        map.retain(|_, bucket| now < bucket.window_start + window_secs);

        if let Some(bucket) = map.get_mut(&key) {
            if now < bucket.window_start + window_secs {
                if bucket.count >= max_reqs {
                    let retry_after = (bucket.window_start + window_secs)
                        .saturating_sub(now)
                        .max(1);
                    return Err(ServerError::RateLimited {
                        retry_after_secs: retry_after,
                    });
                }
                bucket.count += 1;
                return Ok(());
            }
        }

        // New window for this IP
        map.insert(
            key,
            IpBucket {
                count: 1,
                window_start: now,
            },
        );

        Ok(())
    }

    /// Sweeps expired rate limit entries from memory.
    pub async fn sweep_expired(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();
        let window_secs = self.window_secs.load(Ordering::SeqCst);

        let mut map = self.entries.lock().await;
        map.retain(|_, bucket| now < bucket.window_start + window_secs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_rate_limiter_allows_up_to_max_and_blocks() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 3,
            window_secs: 60,
        });

        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        // 3 requests allowed
        assert!(limiter.check_and_record(ip).await.is_ok());
        assert!(limiter.check_and_record(ip).await.is_ok());
        assert!(limiter.check_and_record(ip).await.is_ok());

        // 4th request blocked
        let err = limiter.check_and_record(ip).await.unwrap_err();
        match err {
            ServerError::RateLimited { retry_after_secs } => {
                assert!(retry_after_secs <= 60 && retry_after_secs > 0);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }

        // A different IP is not affected
        let other_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 101));
        assert!(limiter.check_and_record(other_ip).await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_ipv6_prefix_grouping() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 2,
            window_secs: 60,
        });

        // Two IPv6 addresses with identical /64 prefix
        let ip1: IpAddr = "2001:db8:abcd:0012:0000:0000:0000:0001".parse().unwrap();
        let ip2: IpAddr = "2001:db8:abcd:0012:ffff:ffff:ffff:ffff".parse().unwrap();

        assert!(limiter.check_and_record(ip1).await.is_ok());
        assert!(limiter.check_and_record(ip2).await.is_ok());

        // 3rd request from same /64 is blocked
        assert!(limiter.check_and_record(ip1).await.is_err());

        // A different /64 prefix is not blocked
        let ip3: IpAddr = "2001:db8:abcd:0013:0000:0000:0000:0001".parse().unwrap();
        assert!(limiter.check_and_record(ip3).await.is_ok());
    }
}
