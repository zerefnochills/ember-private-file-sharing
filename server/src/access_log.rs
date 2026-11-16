//! Privacy-preserving access logging for Ember relay server.
//!
//! Follows `docs/privacy-and-logging.md` Section 2:
//! - Strictly limited log fields: timestamp, client IP, method, route template, status code, size bucket, duration.
//! - Route templates only (e.g. `/v1/blobs/{id}/parts/{n}`); actual blob IDs, mailbox IDs, fragments, and keys are NEVER logged.
//! - Response size is rounded into coarse buckets to prevent traffic analysis.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Default retention for persistent access logs: 30 days.
pub const DEFAULT_RETENTION_DAYS: u32 = 30;
/// Seconds in one calendar day.
pub const SECS_PER_DAY: u64 = 86_400;

/// Maximum number of access-log entries kept in memory.
const MAX_BUFFERED_ENTRIES: usize = 10_000;

/// Coarse size bucket to prevent precise size leakage.
pub fn size_bucket(bytes: usize) -> &'static str {
    if bytes < 1024 {
        "<1KB"
    } else if bytes <= 65536 {
        "1KB-64KB"
    } else if bytes <= 1048576 {
        "64KB-1MB"
    } else if bytes <= 52428800 {
        "1MB-50MB"
    } else {
        ">50MB"
    }
}

/// Converts a concrete request path into a static route template.
///
/// Guaranteed NEVER to leak blob IDs, mailbox IDs, or link fragments into logs.
pub fn sanitize_route_template(path: &str) -> &'static str {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if parts.is_empty() {
        return "/";
    }

    match parts.as_slice() {
        ["style.css"] => "/style.css",
        ["app.js"] => "/app.js",
        ["d", _] => "/d/{id}",
        ["v1", "pow", "challenge"] => "/v1/pow/challenge",
        ["v1", "blobs"] => "/v1/blobs",
        ["v1", "blobs", _] => "/v1/blobs/{id}",
        ["v1", "blobs", _, "complete"] => "/v1/blobs/{id}/complete",
        ["v1", "blobs", _, "claim"] => "/v1/blobs/{id}/claim",
        ["v1", "blobs", _, "finish"] => "/v1/blobs/{id}/finish",
        ["v1", "blobs", _, "parts", _] => "/v1/blobs/{id}/parts/{n}",
        ["v1", "mailboxes"] => "/v1/mailboxes",
        ["v1", "mailboxes", _] => "/v1/mailboxes/{id}",
        ["v1", "mailboxes", _, "messages"] => "/v1/mailboxes/{id}/messages",
        ["v1", "reports"] => "/v1/reports",
        ["v1", "admin", "takedown"] => "/v1/admin/takedown",
        ["v1", "admin", "reports"] => "/v1/admin/reports",
        ["v1", "transparency"] => "/v1/transparency",
        _ => "/unknown",
    }
}

/// A captured log line for privacy inspection during testing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccessLogEntry {
    pub timestamp: u64,
    pub ip: String,
    pub method: String,
    pub route_template: &'static str,
    pub status: u16,
    pub size_bucket: &'static str,
    pub duration_ms: u64,
}

impl AccessLogEntry {
    pub fn format_line(&self) -> String {
        format!(
            "[{}] ip=\"{}\" method=\"{}\" route=\"{}\" status={} size=\"{}\" duration_ms={}",
            self.timestamp,
            self.ip,
            self.method,
            self.route_template,
            self.status,
            self.size_bucket,
            self.duration_ms
        )
    }
}

/// Collector for recording log entries with in-memory buffer and optional persistent disk storage.
#[derive(Clone)]
pub struct AccessLogRecorder {
    entries: Arc<Mutex<Vec<AccessLogEntry>>>,
    file_path: Option<PathBuf>,
    retention_secs: u64,
}

impl Default for AccessLogRecorder {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
            file_path: None,
            retention_secs: (DEFAULT_RETENTION_DAYS as u64) * SECS_PER_DAY,
        }
    }
}

impl AccessLogRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an access log recorder that persists JSONL entries to disk with retention in days.
    pub fn with_file(path: impl Into<PathBuf>, retention_days: u32) -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
            file_path: Some(path.into()),
            retention_secs: (retention_days as u64) * SECS_PER_DAY,
        }
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    pub fn retention_secs(&self) -> u64 {
        self.retention_secs
    }

    pub fn record(&self, entry: AccessLogEntry) {
        if let Ok(mut lock) = self.entries.lock() {
            // Bound memory: without a cap this buffer would grow with every request, forever.
            if lock.len() >= MAX_BUFFERED_ENTRIES {
                let drop_n = MAX_BUFFERED_ENTRIES / 10;
                lock.drain(..drop_n);
            }
            lock.push(entry.clone());
        }

        if let Some(path) = &self.file_path {
            if let Ok(json_line) = serde_json::to_string(&entry) {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                    let _ = writeln!(file, "{json_line}");
                }
            }
        }
    }

    pub fn entries(&self) -> Vec<AccessLogEntry> {
        self.entries.lock().map(|l| l.clone()).unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut lock) = self.entries.lock() {
            lock.clear();
        }
    }

    /// Prunes entries older than `now - retention_secs` from both the in-memory buffer
    /// and the persistent file on disk. Returns number of expired entries pruned.
    pub fn prune_expired(&self, now: u64) -> usize {
        let cutoff = now.saturating_sub(self.retention_secs);
        let mut mem_pruned = 0;

        if let Ok(mut lock) = self.entries.lock() {
            let initial_len = lock.len();
            lock.retain(|e| e.timestamp >= cutoff);
            mem_pruned = initial_len - lock.len();
        }

        let mut disk_pruned = 0;
        if let Some(path) = &self.file_path {
            if path.exists() {
                if let Ok(file) = fs::File::open(path) {
                    let reader = BufReader::new(file);
                    let mut retained_lines = Vec::new();

                    #[derive(serde::Deserialize)]
                    struct LogTimestamp {
                        timestamp: u64,
                    }

                    for line in reader.lines().map_while(Result::ok) {
                        if line.trim().is_empty() {
                            continue;
                        }
                        if let Ok(item) = serde_json::from_str::<LogTimestamp>(&line) {
                            if item.timestamp >= cutoff {
                                retained_lines.push(line);
                            } else {
                                disk_pruned += 1;
                            }
                        } else {
                            retained_lines.push(line);
                        }
                    }

                    if disk_pruned > 0 {
                        let parent = path.parent().unwrap_or_else(|| Path::new("."));
                        if let Ok(mut temp_file) = tempfile::NamedTempFile::new_in(parent) {
                            let mut write_ok = true;
                            for line in retained_lines {
                                if writeln!(temp_file, "{line}").is_err() {
                                    write_ok = false;
                                    break;
                                }
                            }
                            if write_ok {
                                let _ = temp_file.persist(path);
                            }
                        }
                    }
                }
            }
        }

        if self.file_path.is_some() {
            disk_pruned
        } else {
            mem_pruned
        }
    }
}

/// Middleware that records privacy-preserving access logs.
pub async fn access_log_middleware(
    recorder: AccessLogRecorder,
    trusted_proxies: std::sync::Arc<Vec<std::net::IpAddr>>,
    req: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let route_template = sanitize_route_template(&path);

    let peer = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0);
    let ip = crate::state::resolve_client_ip(&trusted_proxies, peer, req.headers()).to_string();

    let response = next.run(req).await;

    let duration_ms = start.elapsed().as_millis() as u64;
    let status = response.status().as_u16();

    // Approximate response body size from Content-Length header or status
    let response_bytes = response
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    let entry = AccessLogEntry {
        timestamp: crate::state::now_secs(),
        ip,
        method,
        route_template,
        status,
        size_bucket: size_bucket(response_bytes),
        duration_ms,
    };

    recorder.record(entry);

    response
}
