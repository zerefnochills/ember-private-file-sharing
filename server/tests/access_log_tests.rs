//! Tests for the privacy boundary of access-log route and size fields.

use ember_server::{sanitize_route_template, size_bucket, AccessLogEntry, AccessLogRecorder};

#[test]
fn route_templates_never_include_resource_ids() {
    let blob_id = "AbCdEf0123456789_-Abcd";
    let mailbox_id = "QwErTy0123456789_-Qwer";

    let cases = [
        (format!("/d/{blob_id}"), "/d/{id}"),
        (
            format!("/v1/blobs/{blob_id}/parts/12"),
            "/v1/blobs/{id}/parts/{n}",
        ),
        (
            format!("/v1/mailboxes/{mailbox_id}/messages"),
            "/v1/mailboxes/{id}/messages",
        ),
        ("/v1/admin/takedown".to_string(), "/v1/admin/takedown"),
    ];

    for (path, expected) in cases {
        let route = sanitize_route_template(&path);
        assert_eq!(route, expected);
        assert!(!route.contains(blob_id));
        assert!(!route.contains(mailbox_id));
    }
}

#[test]
fn access_log_size_buckets_are_coarse_and_cover_boundaries() {
    assert_eq!(size_bucket(0), "<1KB");
    assert_eq!(size_bucket(1023), "<1KB");
    assert_eq!(size_bucket(1024), "1KB-64KB");
    assert_eq!(size_bucket(65_536), "1KB-64KB");
    assert_eq!(size_bucket(65_537), "64KB-1MB");
    assert_eq!(size_bucket(1_048_576), "64KB-1MB");
    assert_eq!(size_bucket(1_048_577), "1MB-50MB");
    assert_eq!(size_bucket(52_428_800), "1MB-50MB");
    assert_eq!(size_bucket(52_428_801), ">50MB");
}

#[test]
fn recorder_keeps_only_the_defined_access_fields() {
    let recorder = AccessLogRecorder::new();
    recorder.record(AccessLogEntry {
        timestamp: 1_800_000_000,
        ip: "192.0.2.1".into(),
        method: "GET".into(),
        route_template: "/v1/blobs/{id}",
        status: 200,
        size_bucket: "1KB-64KB",
        duration_ms: 7,
    });

    let entries = recorder.entries();
    assert_eq!(entries.len(), 1);
    let line = entries[0].format_line();
    assert!(line.contains("route=\"/v1/blobs/{id}\""));
    assert!(!line.contains("AbCdEf0123456789_-Abcd"));
    assert!(!line.contains("link_key"));
}

#[tokio::test]
async fn test_access_log_disk_persistence_and_retention_pruning() {
    let temp_dir = tempfile::tempdir().unwrap();
    let log_file = temp_dir.path().join("access.log");

    // 30 days retention = 30 * 86,400 = 2,592,000 seconds
    let recorder = AccessLogRecorder::with_file(&log_file, 30);
    assert_eq!(recorder.retention_secs(), 30 * 86_400);

    let now = 2_000_000_000u64;
    let expired_ts = now - (31 * 86_400); // 31 days old
    let active_ts = now - (5 * 86_400); // 5 days old

    // Record an expired entry
    recorder.record(AccessLogEntry {
        timestamp: expired_ts,
        ip: "192.0.2.1".into(),
        method: "GET".into(),
        route_template: "/v1/pow/challenge",
        status: 200,
        size_bucket: "<1KB",
        duration_ms: 3,
    });

    // Record an active entry
    recorder.record(AccessLogEntry {
        timestamp: active_ts,
        ip: "192.0.2.2".into(),
        method: "POST".into(),
        route_template: "/v1/blobs",
        status: 201,
        size_bucket: "<1KB",
        duration_ms: 10,
    });

    // Verify both exist on disk
    let raw = std::fs::read_to_string(&log_file).unwrap();
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 2);
    assert!(raw.contains("192.0.2.1"));
    assert!(raw.contains("192.0.2.2"));

    // Prune entries older than 30 days relative to `now`
    let pruned = recorder.prune_expired(now);
    assert_eq!(pruned, 1);

    // Verify disk now only has the active entry
    let raw_after = std::fs::read_to_string(&log_file).unwrap();
    let lines_after: Vec<&str> = raw_after.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines_after.len(), 1);
    assert!(!raw_after.contains("192.0.2.1"));
    assert!(raw_after.contains("192.0.2.2"));

    // Verify in-memory buffer was also pruned
    let mem_entries = recorder.entries();
    assert_eq!(mem_entries.len(), 1);
    assert_eq!(mem_entries[0].ip, "192.0.2.2");
}

#[tokio::test]
async fn test_access_log_sweeper_integration() {
    let temp_dir = tempfile::tempdir().unwrap();
    let log_file = temp_dir.path().join("swept_access.log");

    let recorder = AccessLogRecorder::with_file(&log_file, 30);
    let storage = std::sync::Arc::new(ember_server::MemoryStorage::new());
    let state = ember_server::AppState::new(storage).with_access_logger(recorder.clone());

    // Insert old log entry
    let now = ember_server::now_secs();
    recorder.record(AccessLogEntry {
        timestamp: now - (40 * 86_400), // 40 days old
        ip: "203.0.113.10".into(),
        method: "GET".into(),
        route_template: "/v1/transparency",
        status: 200,
        size_bucket: "<1KB",
        duration_ms: 2,
    });

    assert_eq!(recorder.entries().len(), 1);

    // Run sweeper iteration
    ember_server::sweeper::sweep_once(&state).await;

    // The expired access log entry should have been pruned by the sweeper
    assert_eq!(recorder.entries().len(), 0);
    let disk_content = std::fs::read_to_string(&log_file).unwrap();
    assert!(disk_content.trim().is_empty());
}
