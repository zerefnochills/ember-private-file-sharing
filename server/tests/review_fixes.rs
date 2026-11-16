//! Regression tests for issues found in the M0 to M10 security review.
//!
//! Each test names the weakness it guards against.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

use ember_server::{
    app, AppState, CreateBlobRequest, FilesystemStorage, MemoryStorage, RateLimitConfig, Storage,
};

async fn post_create(
    app: &axum::Router,
    body: &CreateBlobRequest,
    xff: Option<&str>,
) -> StatusCode {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/pow/challenge")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let pow: ember_server::PowChallengeResponse = serde_json::from_slice(&bytes).unwrap();
    let nonce =
        ember_core::solve_pow(pow.challenge.as_bytes(), pow.difficulty, Some(5_000_000)).unwrap();
    let mut b = Request::builder()
        .method("POST")
        .uri("/v1/blobs")
        .header("Content-Type", "application/json")
        .header("X-Pow-Challenge", &pow.challenge)
        .header("X-Pow-Nonce", nonce.to_string());
    if let Some(ip) = xff {
        b = b.header("X-Forwarded-For", ip);
    }
    app.clone()
        .oneshot(
            b.body(Body::from(serde_json::to_vec(body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

fn req(size: u64, parts: u32) -> CreateBlobRequest {
    CreateBlobRequest {
        size,
        parts,
        burn: false,
        ttl_secs: Some(3600),
        pow: None,
    }
}

/// A client must not be able to dodge the per-address limit by forging X-Forwarded-For.
#[tokio::test]
async fn forged_forwarded_for_does_not_bypass_rate_limit() {
    let state = AppState::with_pow_and_rate_limit(
        Arc::new(MemoryStorage::new()),
        4,
        RateLimitConfig {
            max_requests: 2,
            window_secs: 60,
        },
    );
    let app = app(state);
    let body = req(8250, 2);
    assert_eq!(
        post_create(&app, &body, Some("1.1.1.1")).await,
        StatusCode::CREATED
    );
    assert_eq!(
        post_create(&app, &body, Some("2.2.2.2")).await,
        StatusCode::CREATED
    );
    // Third request with yet another forged address must still be throttled.
    assert_eq!(
        post_create(&app, &body, Some("3.3.3.3")).await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

/// Declared size and part count must agree, so nobody can reserve millions of parts.
#[tokio::test]
async fn inconsistent_size_and_part_count_is_rejected() {
    let state = AppState::with_pow_and_rate_limit(
        Arc::new(MemoryStorage::new()),
        4,
        RateLimitConfig {
            max_requests: 100,
            window_secs: 60,
        },
    );
    let app = app(state);
    // Four billion parts for a tiny blob.
    assert_eq!(
        post_create(&app, &req(8250, u32::MAX), None).await,
        StatusCode::BAD_REQUEST
    );
    // Only part 0 and no content.
    assert_eq!(
        post_create(&app, &req(4137, 1), None).await,
        StatusCode::BAD_REQUEST
    );
    // Size too small for the number of parts.
    assert_eq!(
        post_create(&app, &req(5000, 3), None).await,
        StatusCode::BAD_REQUEST
    );
    // Size too large for the number of parts.
    assert_eq!(
        post_create(&app, &req(10_000_000, 2), None).await,
        StatusCode::BAD_REQUEST
    );
    // A realistic single-chunk drop is accepted.
    assert_eq!(
        post_create(&app, &req(8250, 2), None).await,
        StatusCode::CREATED
    );
}

/// Identifiers become path components; `..` or absolute paths must never reach the filesystem.
#[tokio::test]
async fn filesystem_storage_ignores_path_traversal_ids() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("blobs");
    let storage = FilesystemStorage::new(&root).await.unwrap();
    let sentinel = dir.path().join("sentinel.txt");
    std::fs::write(&sentinel, b"keep me").unwrap();

    for evil in ["..", ".", "../..", "/", "/etc", "a/b", ".tombstones"] {
        storage.delete_blob(evil).await.ok();
        storage.delete_mailbox(evil).await.ok();
        storage.record_tombstone(evil, 0).await.ok();
    }

    assert!(root.exists(), "storage root must survive");
    assert!(sentinel.exists(), "files outside the root must survive");
}

/// The admin takedown endpoint must reject malformed identifiers.
#[tokio::test]
async fn admin_takedown_rejects_path_like_target_ids() {
    let state =
        AppState::new(Arc::new(MemoryStorage::new())).with_admin_token("review-admin-token");
    let app = app(state);
    for evil in ["..", "../x", "/etc/passwd", ""] {
        let body = serde_json::json!({
            "target_id": evil, "target_type": "blob",
            "request_ref": "t", "operator": "op", "reason": "test"
        });
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/takedown")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer review-admin-token")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "target_id {evil:?}");
    }
}

/// Oversized free-text report fields must be refused.
#[tokio::test]
async fn oversized_report_is_rejected() {
    let state = AppState::new(Arc::new(MemoryStorage::new()));
    let app = app(state);
    let body = serde_json::json!({
        "target_id": "x", "target_type": "blob", "reason": "r".repeat(5000)
    });
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/reports")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
