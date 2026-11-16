//! Application state for Ember relay server.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ember_protocol::{DEFAULT_MAX_CIPHERTEXT_BYTES, MAX_LIFETIME_SECS, MAX_PART_SIZE};

use crate::access_log::AccessLogRecorder;
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
    pub mailbox_notifier: tokio::sync::broadcast::Sender<String>,
    pub access_logger: AccessLogRecorder,
    pub admin_token: Option<String>,
    /// Reverse proxies whose `X-Forwarded-For` / `X-Real-IP` headers may be believed.
    /// Empty by default: the TCP peer address is then always used as the client address.
    pub trusted_proxies: Arc<Vec<IpAddr>>,
    /// Serialises claim and finish operations so a burn-after-view blob can only be
    /// claimed by one reader (the storage trait has no atomic compare-and-set).
    pub claim_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        let (mailbox_notifier, _) = tokio::sync::broadcast::channel(256);
        let admin_token = std::env::var("EMBER_ADMIN_TOKEN").ok();
        Self {
            storage,
            max_total_bytes: DEFAULT_MAX_CIPHERTEXT_BYTES,
            max_part_bytes: MAX_PART_SIZE as u64,
            max_ttl_secs: MAX_LIFETIME_SECS,
            upload_timeout_secs: 3600, // 1 hour for uncompleted uploads
            claim_window_secs: 600,    // 10 minutes claim window
            pow: Arc::new(PowManager::new(DEFAULT_POW_DIFFICULTY)),
            rate_limiter: Arc::new(RateLimiter::new(RateLimitConfig::default())),
            mailbox_notifier,
            access_logger: AccessLogRecorder::new(),
            admin_token,
            trusted_proxies: Arc::new(Vec::new()),
            claim_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn with_pow_and_rate_limit(
        storage: Arc<dyn Storage>,
        difficulty: u32,
        rate_limit_config: RateLimitConfig,
    ) -> Self {
        let (mailbox_notifier, _) = tokio::sync::broadcast::channel(256);
        let admin_token = std::env::var("EMBER_ADMIN_TOKEN").ok();
        Self {
            storage,
            max_total_bytes: DEFAULT_MAX_CIPHERTEXT_BYTES,
            max_part_bytes: MAX_PART_SIZE as u64,
            max_ttl_secs: MAX_LIFETIME_SECS,
            upload_timeout_secs: 3600,
            claim_window_secs: 600,
            pow: Arc::new(PowManager::new(difficulty)),
            rate_limiter: Arc::new(RateLimiter::new(rate_limit_config)),
            mailbox_notifier,
            access_logger: AccessLogRecorder::new(),
            admin_token,
            trusted_proxies: Arc::new(Vec::new()),
            claim_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn with_admin_token(mut self, token: impl Into<String>) -> Self {
        self.admin_token = Some(token.into());
        self
    }

    /// Declares reverse proxies (by IP) whose forwarding headers are trusted.
    pub fn with_trusted_proxies(mut self, proxies: Vec<IpAddr>) -> Self {
        self.trusted_proxies = Arc::new(proxies);
        self
    }

    pub fn with_access_logger(mut self, logger: AccessLogRecorder) -> Self {
        self.access_logger = logger;
        self
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Resolves the client address used for rate limiting and access logs.
///
/// The TCP peer address is authoritative. `X-Forwarded-For` / `X-Real-IP` are honoured only when
/// the peer is one of the configured trusted proxies; the right-most address that is not itself
/// a trusted proxy is used (the left-most entries are client-controlled and can be forged).
/// When no peer address is available (in-process tests) the loopback address is assumed.
pub fn resolve_client_ip(
    trusted_proxies: &[IpAddr],
    peer: Option<std::net::SocketAddr>,
    headers: &axum::http::HeaderMap,
) -> IpAddr {
    let peer_ip = peer
        .map(|p| p.ip())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

    if !trusted_proxies.contains(&peer_ip) {
        return peer_ip;
    }

    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        for entry in forwarded.rsplit(',') {
            if let Ok(ip) = entry.trim().parse::<IpAddr>() {
                if !trusted_proxies.contains(&ip) {
                    return ip;
                }
            }
        }
    }

    if let Some(real) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
    {
        return real;
    }

    peer_ip
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    fn hm(xff: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", xff.parse().unwrap());
        h
    }

    #[test]
    fn untrusted_peer_cannot_spoof_forwarded_header() {
        let peer: std::net::SocketAddr = "203.0.113.9:1234".parse().unwrap();
        let ip = resolve_client_ip(&[], Some(peer), &hm("1.2.3.4"));
        assert_eq!(ip, "203.0.113.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn trusted_proxy_uses_rightmost_untrusted_entry() {
        let proxy: IpAddr = "10.0.0.1".parse().unwrap();
        let peer: std::net::SocketAddr = "10.0.0.1:443".parse().unwrap();
        // The client forged "9.9.9.9"; the proxy appended the real address after it.
        let ip = resolve_client_ip(&[proxy], Some(peer), &hm("9.9.9.9, 198.51.100.7"));
        assert_eq!(ip, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn missing_peer_falls_back_to_loopback() {
        let ip = resolve_client_ip(&[], None, &HeaderMap::new());
        assert_eq!(ip, IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    }
}
