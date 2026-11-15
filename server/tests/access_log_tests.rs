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
