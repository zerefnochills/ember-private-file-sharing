//! Integration tests for Ember relay server (Milestone M3).
//!
//! Verifies:
//! - Full upload and download roundtrip
//! - Burn-after-view survives plain GET and deletes on finish
//! - Concurrent claim rejection (409 Conflict)
//! - Hard expiry and sweeper deletion
//! - Interrupted upload resumption
//! - Range request inside parts
//! - Key and link fragment are never sent to or stored by server
//! - End-to-end CLI send and receive with live HTTP server

use axum::body::{to_bytes, Body};
use axum::http::header::{AUTHORIZATION, RANGE};
use axum::http::{Request, StatusCode};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::net::TcpListener;
use tokio::process::Command;
use tower::ServiceExt;

use ember_core::{
    build_link, decrypt_drop_parts, encrypt_drop_to_parts, generate_blob_id,
    thread::{derive_rendezvous_mailbox, HandshakeInitiator, HandshakeResponder, ThreadSession},
    DropMetadata, LinkPayload, SecretKey,
};
use ember_server::{
    app, sweeper::sweep_once, AccessLogRecorder, AdminTakedownRequest, AppState, CreateBlobRequest,
    CreateBlobResponse, CreateMailboxRequest, FilesystemStorage, GetMessagesResponse,
    MemoryStorage, PushMessageRequest, PushMessageResponse, ReportRecord, Storage,
    SubmitReportRequest, SubmitReportResponse, TakedownRecord, TransparencyStats,
};

async fn response_json<T: serde::de::DeserializeOwned>(res: axum::response::Response) -> T {
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn response_bytes(res: axum::response::Response) -> Vec<u8> {
    to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

/// Builds the CLI binaries once if they are missing. `cargo test -p ember-server` does not
/// build other packages' binaries, so on a clean checkout (or in CI) these tests used to fail
/// unless someone had run `cargo build --workspace` first.
fn ensure_cli_binaries_built() {
    static BUILD: std::sync::Once = std::sync::Once::new();
    BUILD.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = std::process::Command::new(cargo)
            .args([
                "build",
                "--quiet",
                "-p",
                "ember-cli",
                "-p",
                "ember-admin",
                "-p",
                "ember-server",
            ])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .status()
            .expect("failed to run cargo to build the CLI binaries");
        assert!(status.success(), "building the CLI binaries failed");
    });
}

fn find_ember_bin() -> PathBuf {
    ensure_cli_binaries_built();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = if cfg!(windows) { "ember.exe" } else { "ember" };
    let path = manifest_dir.join("../target/debug").join(exe);
    if path.exists() {
        return path;
    }
    manifest_dir.join("../../target/debug").join(exe)
}

fn find_ember_admin_bin() -> PathBuf {
    ensure_cli_binaries_built();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = if cfg!(windows) {
        "ember-admin.exe"
    } else {
        "ember-admin"
    };
    let path = manifest_dir.join("../target/debug").join(exe);
    if path.exists() {
        return path;
    }
    manifest_dir.join("../../target/debug").join(exe)
}

fn find_ember_server_bin() -> PathBuf {
    ensure_cli_binaries_built();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = if cfg!(windows) {
        "ember-server.exe"
    } else {
        "ember-server"
    };
    let path = manifest_dir.join("../target/debug").join(exe);
    if path.exists() {
        return path;
    }
    manifest_dir.join("../../target/debug").join(exe)
}

async fn create_blob_with_pow(
    app: &axum::Router,
    body: &CreateBlobRequest,
) -> axum::response::Response {
    let req = Request::builder()
        .method("GET")
        .uri("/v1/pow/challenge")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let pow_resp: ember_server::PowChallengeResponse = response_json(res).await;

    let nonce = ember_core::solve_pow(
        pow_resp.challenge.as_bytes(),
        pow_resp.difficulty,
        Some(5_000_000),
    )
    .unwrap();

    let req = Request::builder()
        .method("POST")
        .uri("/v1/blobs")
        .header("Content-Type", "application/json")
        .header("X-Pow-Challenge", &pow_resp.challenge)
        .header("X-Pow-Nonce", nonce.to_string())
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap();
    app.clone().oneshot(req).await.unwrap()
}

async fn create_mailbox_with_pow(
    app: &axum::Router,
    body: &CreateMailboxRequest,
) -> axum::response::Response {
    let req = Request::builder()
        .method("GET")
        .uri("/v1/pow/challenge")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let pow_resp: ember_server::PowChallengeResponse = response_json(res).await;

    let nonce = ember_core::solve_pow(
        pow_resp.challenge.as_bytes(),
        pow_resp.difficulty,
        Some(5_000_000),
    )
    .unwrap();

    let mut body_with_pow = body.clone();
    body_with_pow.pow = Some(ember_server::PowSolutionPayload {
        challenge: pow_resp.challenge,
        nonce,
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/mailboxes")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body_with_pow).unwrap()))
        .unwrap();
    app.clone().oneshot(req).await.unwrap()
}

#[tokio::test]
async fn test_server_upload_and_download_roundtrip() {
    let storage = Arc::new(MemoryStorage::new());
    let state =
        AppState::with_pow_and_rate_limit(storage, 8, ember_server::RateLimitConfig::default());
    let app = app(state);

    let key = SecretKey::generate();
    let content = b"Relay server upload and download roundtrip test content.";
    let meta = DropMetadata::new_file(
        "roundtrip.txt",
        "text/plain",
        content.len() as u64,
        false,
        1_700_000_000,
        1_700_043_200,
    );

    let parts = encrypt_drop_to_parts(&key, &meta, content).unwrap();
    let total_size: u64 = parts.iter().map(|p| p.len() as u64).sum();

    // 1. Create upload with PoW
    let create_req = CreateBlobRequest {
        size: total_size,
        parts: parts.len() as u32,
        burn: false,
        ttl_secs: Some(3600),
        pow: None,
    };

    let res = create_blob_with_pow(&app, &create_req).await;

    assert_eq!(res.status(), StatusCode::CREATED);
    let create_res: CreateBlobResponse = response_json(res).await;
    let blob_id = create_res.id;
    let token = create_res.delete_token;

    // 2. Upload parts
    for (i, part) in parts.iter().enumerate() {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/v1/blobs/{blob_id}/parts/{i}"))
                    .header(AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::from(part.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    // 3. Complete upload
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/blobs/{blob_id}/complete"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // 4. Download parts and decrypt
    let mut downloaded_parts = Vec::new();
    for (i, expected_part) in parts.iter().enumerate() {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/blobs/{blob_id}/parts/{i}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        let part_bytes = response_bytes(res).await;
        assert_eq!(part_bytes, *expected_part);
        downloaded_parts.push(part_bytes);
    }

    let (dec_meta, dec_content) = decrypt_drop_parts(&key, &downloaded_parts).unwrap();
    assert_eq!(dec_meta, meta);
    assert_eq!(dec_content, content);
}

#[tokio::test]
async fn test_burn_after_view_survives_plain_get_and_burns_on_finish() {
    let storage = Arc::new(MemoryStorage::new());
    let state =
        AppState::with_pow_and_rate_limit(storage, 8, ember_server::RateLimitConfig::default());
    let app = app(state);

    let key = SecretKey::generate();
    let content = b"Sensitive burn-after-view document.";
    let meta = DropMetadata::new_file(
        "secret.txt",
        "text/plain",
        content.len() as u64,
        true,
        1_700_000_000,
        1_700_043_200,
    );

    let parts = encrypt_drop_to_parts(&key, &meta, content).unwrap();
    let total_size: u64 = parts.iter().map(|p| p.len() as u64).sum();

    // 1. Create burn blob with PoW
    let create_req = CreateBlobRequest {
        size: total_size,
        parts: parts.len() as u32,
        burn: true,
        ttl_secs: Some(3600),
        pow: None,
    };

    let res = create_blob_with_pow(&app, &create_req).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let create_res: CreateBlobResponse = response_json(res).await;
    let blob_id = create_res.id;
    let token = create_res.delete_token;

    // Upload parts and complete
    for (i, part) in parts.iter().enumerate() {
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/v1/blobs/{blob_id}/parts/{i}"))
                    .header(AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::from(part.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/blobs/{blob_id}/complete"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // 2. Link preview bot sends plain GET /v1/blobs/{id} (multiple times)
    for _ in 0..3 {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/blobs/{blob_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            res.status(),
            StatusCode::OK,
            "Plain GET MUST NOT burn the blob"
        );
    }

    // 3. Legitimate user claims download
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/blobs/{blob_id}/claim"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let claim_res: ember_server::ClaimResponse = response_json(res).await;
    let claim_token = claim_res.claim.expect("Burn blob must return claim token");

    // 4. Concurrent claim while active MUST return 409 Conflict
    let conflict_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/blobs/{blob_id}/claim"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(conflict_res.status(), StatusCode::CONFLICT);

    // 5. Download parts with X-Claim header
    for (i, expected_part) in parts.iter().enumerate() {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/blobs/{blob_id}/parts/{i}"))
                    .header("X-Claim", &claim_token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(response_bytes(res).await, *expected_part);
    }

    // 6. Finish burning
    let finish_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/blobs/{blob_id}/finish"))
                .header("X-Claim", &claim_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(finish_res.status(), StatusCode::OK);

    // 7. Verify blob is GONE (410)
    let gone_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/blobs/{blob_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(gone_res.status(), StatusCode::GONE);
}

#[tokio::test]
async fn test_resumable_upload_and_range_download() {
    let storage = Arc::new(MemoryStorage::new());
    let state =
        AppState::with_pow_and_rate_limit(storage, 8, ember_server::RateLimitConfig::default());
    let app = app(state);

    let key = SecretKey::generate();
    let content = vec![0xEE; 1000];
    let meta = DropMetadata::new_file(
        "test.bin",
        "application/octet-stream",
        1000,
        false,
        1000,
        2000,
    );
    let parts = encrypt_drop_to_parts(&key, &meta, &content).unwrap();
    let total_size: u64 = parts.iter().map(|p| p.len() as u64).sum();

    // 1. Create upload with PoW
    let create_req = CreateBlobRequest {
        size: total_size,
        parts: parts.len() as u32,
        burn: false,
        ttl_secs: Some(3600),
        pow: None,
    };
    let res = create_blob_with_pow(&app, &create_req).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let create_res: CreateBlobResponse = response_json(res).await;
    let blob_id = create_res.id;
    let token = create_res.delete_token;

    // 2. Upload part 0
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/blobs/{blob_id}/parts/0"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(parts[0].clone()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Interruption simulated! Re-uploading part 0 (idempotent)
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/blobs/{blob_id}/parts/0"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(parts[0].clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Upload remaining part 1
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/blobs/{blob_id}/parts/1"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(parts[1].clone()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Complete
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/blobs/{blob_id}/complete"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. Test HTTP Range request on Part 0
    let range_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/blobs/{blob_id}/parts/0"))
                .header(RANGE, "bytes=0-23") // Stream header (first 24 bytes)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(range_res.status(), StatusCode::PARTIAL_CONTENT);
    let range_bytes = response_bytes(range_res).await;
    assert_eq!(range_bytes.len(), 24);
    assert_eq!(range_bytes, parts[0][0..24]);
}

#[tokio::test]
async fn test_expiry_and_sweeper() {
    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::with_pow_and_rate_limit(
        storage.clone(),
        8,
        ember_server::RateLimitConfig::default(),
    );
    let app = app(state.clone());

    let create_req = CreateBlobRequest {
        size: 8250,
        parts: 2,
        burn: false,
        ttl_secs: Some(60),
        pow: None,
    };

    let res = create_blob_with_pow(&app, &create_req).await;
    let create_res: CreateBlobResponse = response_json(res).await;
    let blob_id = create_res.id;

    // Mutate the record expiry into the past to simulate time travel
    let mut record = storage.get_blob(&blob_id).await.unwrap().unwrap();
    record.expires_at = 100; // Past
    storage.update_blob(&record).await.unwrap();

    // Sweeper cleans it up
    let swept = sweep_once(&state).await;
    assert_eq!(swept, 1);

    // Requesting expired blob returns 410 Gone
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/blobs/{blob_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::GONE);
}

#[tokio::test]
async fn test_server_never_receives_link_fragment_or_key() {
    let key = SecretKey::generate();
    let payload = LinkPayload::Unprotected(key.clone());
    let blob_id = generate_blob_id();
    let share_link = build_link("ember.example.com", &blob_id, &payload).unwrap();

    // Link format is: https://ember.example.com/d/<blobId>#<payload>
    let fragment_index = share_link.find('#').expect("Link must contain fragment");
    let url_without_frag = &share_link[..fragment_index];
    let fragment = &share_link[fragment_index + 1..];

    assert!(!fragment.is_empty());
    // The fragment must never be part of HTTP request path or body
    assert!(!url_without_frag.contains(fragment));

    // Inspect server request URIs: fragment character '#' is forbidden in HTTP request-line
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/blobs/{blob_id}"))
        .body(Body::empty())
        .unwrap();

    let path_str = req.uri().to_string();
    assert!(!path_str.contains('#'));
    assert!(!path_str.contains(fragment));
}

#[tokio::test]
async fn test_cli_send_and_receive_end_to_end() {
    let tmp = tempdir().unwrap();
    let storage = Arc::new(MemoryStorage::new());
    let state =
        AppState::with_pow_and_rate_limit(storage, 8, ember_server::RateLimitConfig::default());
    let app = app(state);

    // Bind server to an ephemeral local port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let relay_url = format!("http://127.0.0.1:{}", addr.port());

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let ember_exe = find_ember_bin();
    assert!(
        ember_exe.exists(),
        "ember binary must be compiled at {}",
        ember_exe.display()
    );

    // 1. Prepare sample file
    let input_path = tmp.path().join("secret_document.txt");
    let original_bytes = b"Top secret end-to-end encrypted Ember CLI transmission.";
    std::fs::write(&input_path, original_bytes).unwrap();

    // 2. Run: ember send <file> --relay <url>
    let send_output = Command::new(&ember_exe)
        .arg("send")
        .arg(&input_path)
        .arg("--relay")
        .arg(&relay_url)
        .output()
        .await
        .expect("Failed to execute ember send");

    assert!(
        send_output.status.success(),
        "ember send failed: {}",
        String::from_utf8_lossy(&send_output.stderr)
    );
    let stdout = String::from_utf8_lossy(&send_output.stdout);

    let link_prefix = "Share link: ";
    let link_line = stdout
        .lines()
        .find(|l| l.contains(link_prefix))
        .expect("Must print share link");
    let link = link_line.split(link_prefix).nth(1).unwrap().trim();

    // 3. Run: ember receive <link> --relay <url> -o <out_file>
    let received_path = tmp.path().join("downloaded.txt");
    let recv_output = Command::new(&ember_exe)
        .arg("receive")
        .arg(link)
        .arg("--relay")
        .arg(&relay_url)
        .arg("-o")
        .arg(&received_path)
        .output()
        .await
        .expect("Failed to execute ember receive");

    assert!(
        recv_output.status.success(),
        "ember receive failed: {}",
        String::from_utf8_lossy(&recv_output.stderr)
    );

    // 4. Verify received content matches original exactly
    let received_bytes = std::fs::read(&received_path).unwrap();
    assert_eq!(received_bytes, original_bytes);
}

#[tokio::test]
async fn test_cli_send_and_receive_burn_after_view() {
    let tmp = tempdir().unwrap();
    let storage = Arc::new(MemoryStorage::new());
    let state =
        AppState::with_pow_and_rate_limit(storage, 8, ember_server::RateLimitConfig::default());
    let app = app(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let relay_url = format!("http://127.0.0.1:{}", addr.port());

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let ember_exe = find_ember_bin();

    // 1. Prepare file and send with --burn
    let input_path = tmp.path().join("burn_document.txt");
    let original_bytes = b"Self-destructing burn document.";
    std::fs::write(&input_path, original_bytes).unwrap();

    let send_output = Command::new(&ember_exe)
        .arg("send")
        .arg(&input_path)
        .arg("--relay")
        .arg(&relay_url)
        .arg("--burn")
        .output()
        .await
        .unwrap();

    let stdout = String::from_utf8_lossy(&send_output.stdout);
    let link = stdout
        .lines()
        .find(|l| l.contains("Share link: "))
        .unwrap()
        .split("Share link: ")
        .nth(1)
        .unwrap()
        .trim();

    // 2. Receive first time -> succeeds
    let received_path = tmp.path().join("burn_recv.txt");
    let recv_output = Command::new(&ember_exe)
        .arg("receive")
        .arg(link)
        .arg("--relay")
        .arg(&relay_url)
        .arg("-o")
        .arg(&received_path)
        .output()
        .await
        .unwrap();

    assert!(recv_output.status.success());
    assert_eq!(std::fs::read(&received_path).unwrap(), original_bytes);

    // 3. Second receive attempt MUST fail (burned)
    let second_path = tmp.path().join("burn_recv_2.txt");
    let second_recv = Command::new(&ember_exe)
        .arg("receive")
        .arg(link)
        .arg("--relay")
        .arg(&relay_url)
        .arg("-o")
        .arg(&second_path)
        .output()
        .await
        .unwrap();

    assert!(
        !second_recv.status.success(),
        "Second receive on burn drop MUST fail"
    );
    let stderr = String::from_utf8_lossy(&second_recv.stderr);
    assert!(stderr.contains("expired or was already burned"));
}

#[tokio::test]
async fn test_pow_missing_is_rejected() {
    let storage = Arc::new(MemoryStorage::new());
    let state =
        AppState::with_pow_and_rate_limit(storage, 8, ember_server::RateLimitConfig::default());
    let app = app(state);

    let create_req = CreateBlobRequest {
        size: 8250,
        parts: 2,
        burn: false,
        ttl_secs: Some(3600),
        pow: None,
    };

    // Attempt POST /v1/blobs without X-Pow-Challenge / X-Pow-Nonce headers
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/blobs")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response_bytes(res).await;
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"], "pow_required");
}

#[tokio::test]
async fn test_pow_invalid_nonce_is_rejected() {
    let storage = Arc::new(MemoryStorage::new());
    let state =
        AppState::with_pow_and_rate_limit(storage, 16, ember_server::RateLimitConfig::default());
    let app = app(state);

    // Get challenge
    let req = Request::builder()
        .method("GET")
        .uri("/v1/pow/challenge")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let pow_resp: ember_server::PowChallengeResponse = response_json(res).await;

    let create_req = CreateBlobRequest {
        size: 8250,
        parts: 2,
        burn: false,
        ttl_secs: Some(3600),
        pow: None,
    };

    // Submit with intentionally invalid nonce (e.g. 0 when difficulty is 16)
    // Find an invalid nonce that does not solve the 16-bit challenge
    let mut invalid_nonce = 0u64;
    while ember_core::pow::verify_pow(pow_resp.challenge.as_bytes(), invalid_nonce, 16) {
        invalid_nonce += 1;
    }

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/blobs")
                .header("Content-Type", "application/json")
                .header("X-Pow-Challenge", &pow_resp.challenge)
                .header("X-Pow-Nonce", invalid_nonce.to_string())
                .body(Body::from(serde_json::to_vec(&create_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response_bytes(res).await;
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"], "invalid_pow");
}

#[tokio::test]
async fn test_pow_replay_is_rejected() {
    let storage = Arc::new(MemoryStorage::new());
    let state =
        AppState::with_pow_and_rate_limit(storage, 8, ember_server::RateLimitConfig::default());
    let app = app(state);

    // Get challenge
    let req = Request::builder()
        .method("GET")
        .uri("/v1/pow/challenge")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let pow_resp: ember_server::PowChallengeResponse = response_json(res).await;

    let nonce = ember_core::solve_pow(
        pow_resp.challenge.as_bytes(),
        pow_resp.difficulty,
        Some(5_000_000),
    )
    .unwrap();

    let create_req = CreateBlobRequest {
        size: 8250,
        parts: 2,
        burn: false,
        ttl_secs: Some(3600),
        pow: None,
    };

    // First submission -> 201 Created
    let res1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/blobs")
                .header("Content-Type", "application/json")
                .header("X-Pow-Challenge", &pow_resp.challenge)
                .header("X-Pow-Nonce", nonce.to_string())
                .body(Body::from(serde_json::to_vec(&create_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res1.status(), StatusCode::CREATED);

    // Second submission with identical challenge and nonce -> 400 Bad Request (pow_replayed)
    let res2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/blobs")
                .header("Content-Type", "application/json")
                .header("X-Pow-Challenge", &pow_resp.challenge)
                .header("X-Pow-Nonce", nonce.to_string())
                .body(Body::from(serde_json::to_vec(&create_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res2.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response_bytes(res2).await;
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"], "pow_replayed");
}

#[tokio::test]
async fn test_rate_limiter_triggers_and_recovers() {
    let storage = Arc::new(MemoryStorage::new());
    let rate_config = ember_server::RateLimitConfig {
        max_requests: 2,
        window_secs: 2, // 2-second short window for test recovery
    };
    // In-process requests carry no TCP peer, so they appear to come from loopback. Trust it as
    // a reverse proxy so the test can emulate distinct client addresses via X-Forwarded-For.
    let state = AppState::with_pow_and_rate_limit(storage, 4, rate_config)
        .with_trusted_proxies(vec!["127.0.0.1".parse().unwrap()]);
    let app = app(state.clone());

    let create_req = CreateBlobRequest {
        size: 8250,
        parts: 2,
        burn: false,
        ttl_secs: Some(3600),
        pow: None,
    };

    let client_ip = "192.168.1.50";

    // Request 1 from client_ip -> 201 Created
    let res1 = {
        let req_pow = Request::builder()
            .method("GET")
            .uri("/v1/pow/challenge")
            .body(Body::empty())
            .unwrap();
        let pow_resp: ember_server::PowChallengeResponse =
            response_json(app.clone().oneshot(req_pow).await.unwrap()).await;
        let nonce = ember_core::solve_pow(
            pow_resp.challenge.as_bytes(),
            pow_resp.difficulty,
            Some(5_000_000),
        )
        .unwrap();

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/blobs")
                    .header("Content-Type", "application/json")
                    .header("X-Forwarded-For", client_ip)
                    .header("X-Pow-Challenge", &pow_resp.challenge)
                    .header("X-Pow-Nonce", nonce.to_string())
                    .body(Body::from(serde_json::to_vec(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
    };
    assert_eq!(res1.status(), StatusCode::CREATED);

    // Request 2 from client_ip -> 201 Created
    let res2 = {
        let req_pow = Request::builder()
            .method("GET")
            .uri("/v1/pow/challenge")
            .body(Body::empty())
            .unwrap();
        let pow_resp: ember_server::PowChallengeResponse =
            response_json(app.clone().oneshot(req_pow).await.unwrap()).await;
        let nonce = ember_core::solve_pow(
            pow_resp.challenge.as_bytes(),
            pow_resp.difficulty,
            Some(5_000_000),
        )
        .unwrap();

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/blobs")
                    .header("Content-Type", "application/json")
                    .header("X-Forwarded-For", client_ip)
                    .header("X-Pow-Challenge", &pow_resp.challenge)
                    .header("X-Pow-Nonce", nonce.to_string())
                    .body(Body::from(serde_json::to_vec(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
    };
    assert_eq!(res2.status(), StatusCode::CREATED);

    // Request 3 from client_ip exceeds limit -> 429 Too Many Requests
    let res3 = {
        let req_pow = Request::builder()
            .method("GET")
            .uri("/v1/pow/challenge")
            .body(Body::empty())
            .unwrap();
        let pow_resp: ember_server::PowChallengeResponse =
            response_json(app.clone().oneshot(req_pow).await.unwrap()).await;
        let nonce = ember_core::solve_pow(
            pow_resp.challenge.as_bytes(),
            pow_resp.difficulty,
            Some(5_000_000),
        )
        .unwrap();

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/blobs")
                    .header("Content-Type", "application/json")
                    .header("X-Forwarded-For", client_ip)
                    .header("X-Pow-Challenge", &pow_resp.challenge)
                    .header("X-Pow-Nonce", nonce.to_string())
                    .body(Body::from(serde_json::to_vec(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
    };
    assert_eq!(res3.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(res3.headers().contains_key("retry-after"));
    let body_bytes = response_bytes(res3).await;
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"], "rate_limited");

    // Request from a different IP is unaffected -> 201 Created
    let res_other = {
        let req_pow = Request::builder()
            .method("GET")
            .uri("/v1/pow/challenge")
            .body(Body::empty())
            .unwrap();
        let pow_resp: ember_server::PowChallengeResponse =
            response_json(app.clone().oneshot(req_pow).await.unwrap()).await;
        let nonce = ember_core::solve_pow(
            pow_resp.challenge.as_bytes(),
            pow_resp.difficulty,
            Some(5_000_000),
        )
        .unwrap();

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/blobs")
                    .header("Content-Type", "application/json")
                    .header("X-Forwarded-For", "192.168.1.99")
                    .header("X-Pow-Challenge", &pow_resp.challenge)
                    .header("X-Pow-Nonce", nonce.to_string())
                    .body(Body::from(serde_json::to_vec(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
    };
    assert_eq!(res_other.status(), StatusCode::CREATED);

    // Wait 2.1 seconds for window to expire -> client_ip recovers and request succeeds
    tokio::time::sleep(std::time::Duration::from_millis(2100)).await;

    let res_recovered = {
        let req_pow = Request::builder()
            .method("GET")
            .uri("/v1/pow/challenge")
            .body(Body::empty())
            .unwrap();
        let pow_resp: ember_server::PowChallengeResponse =
            response_json(app.clone().oneshot(req_pow).await.unwrap()).await;
        let nonce = ember_core::solve_pow(
            pow_resp.challenge.as_bytes(),
            pow_resp.difficulty,
            Some(5_000_000),
        )
        .unwrap();

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/blobs")
                    .header("Content-Type", "application/json")
                    .header("X-Forwarded-For", client_ip)
                    .header("X-Pow-Challenge", &pow_resp.challenge)
                    .header("X-Pow-Nonce", nonce.to_string())
                    .body(Body::from(serde_json::to_vec(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
    };
    assert_eq!(res_recovered.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_web_receiver_static_assets_serving_and_security_headers() {
    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::new(storage);
    let app = app(state);

    // 1. GET / serves index.html with strict CSP and security headers
    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert_eq!(
        res.headers().get("content-security-policy").unwrap(),
        "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data: blob:; base-uri 'none'; form-action 'none'; frame-ancestors 'none';"
    );
    assert_eq!(
        res.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(res.headers().get("x-frame-options").unwrap(), "DENY");
    assert_eq!(res.headers().get("referrer-policy").unwrap(), "no-referrer");
    let body = String::from_utf8(response_bytes(res).await).unwrap();
    assert!(body.contains("Ember — Drop Receiver"));

    // 2. GET /d/{id} serves the same index.html with identical security headers
    let req_drop = Request::builder()
        .method("GET")
        .uri("/d/test-blob-1234")
        .body(Body::empty())
        .unwrap();
    let res_drop = app.clone().oneshot(req_drop).await.unwrap();
    assert_eq!(res_drop.status(), StatusCode::OK);
    assert_eq!(
        res_drop.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    let body_drop = String::from_utf8(response_bytes(res_drop).await).unwrap();
    assert!(body_drop.contains("Ember — Drop Receiver"));

    // 3. GET /style.css serves CSS
    let req_css = Request::builder()
        .method("GET")
        .uri("/style.css")
        .body(Body::empty())
        .unwrap();
    let res_css = app.clone().oneshot(req_css).await.unwrap();
    assert_eq!(res_css.status(), StatusCode::OK);
    assert_eq!(
        res_css.headers().get("content-type").unwrap(),
        "text/css; charset=utf-8"
    );
    let body_css = String::from_utf8(response_bytes(res_css).await).unwrap();
    assert!(body_css.contains(":root"));

    // 4. GET /app.js serves JavaScript bundle
    let req_js = Request::builder()
        .method("GET")
        .uri("/app.js")
        .body(Body::empty())
        .unwrap();
    let res_js = app.clone().oneshot(req_js).await.unwrap();
    assert_eq!(res_js.status(), StatusCode::OK);
    assert_eq!(
        res_js.headers().get("content-type").unwrap(),
        "application/javascript; charset=utf-8"
    );
    let bytes_js = response_bytes(res_js).await;
    assert!(!bytes_js.is_empty());
}

#[tokio::test]
async fn test_web_receiver_bot_preview_does_not_burn_blob() {
    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::new(storage);
    let app = app(state);

    // Create burn-after-view blob
    let create_req = CreateBlobRequest {
        size: 8250,
        parts: 2,
        burn: true,
        ttl_secs: Some(3600),
        pow: None,
    };
    let res = create_blob_with_pow(&app, &create_req).await;
    let create_resp: CreateBlobResponse = response_json(res).await;
    let blob_id = create_resp.id;

    // Upload part 0
    let part0 = vec![42u8; 4137];
    let req_part = Request::builder()
        .method("PUT")
        .uri(format!("/v1/blobs/{blob_id}/parts/0"))
        .header(
            AUTHORIZATION,
            format!("Bearer {}", create_resp.delete_token),
        )
        .body(Body::from(part0))
        .unwrap();
    let res_part = app.clone().oneshot(req_part).await.unwrap();
    assert_eq!(res_part.status(), StatusCode::NO_CONTENT);

    // Upload part 1 (a minimal 4096-byte content chunk plus 17 bytes of overhead)
    let req_part1 = Request::builder()
        .method("PUT")
        .uri(format!("/v1/blobs/{blob_id}/parts/1"))
        .header(
            AUTHORIZATION,
            format!("Bearer {}", create_resp.delete_token),
        )
        .body(Body::from(vec![43u8; 4113]))
        .unwrap();
    let res_part1 = app.clone().oneshot(req_part1).await.unwrap();
    assert_eq!(res_part1.status(), StatusCode::NO_CONTENT);

    // Complete upload
    let req_comp = Request::builder()
        .method("POST")
        .uri(format!("/v1/blobs/{blob_id}/complete"))
        .header(
            AUTHORIZATION,
            format!("Bearer {}", create_resp.delete_token),
        )
        .body(Body::empty())
        .unwrap();
    let res_comp = app.clone().oneshot(req_comp).await.unwrap();
    assert_eq!(res_comp.status(), StatusCode::OK);

    // Bot opens web receiver page: GET /d/{blob_id}
    let req_bot = Request::builder()
        .method("GET")
        .uri(format!("/d/{blob_id}"))
        .body(Body::empty())
        .unwrap();
    let res_bot = app.clone().oneshot(req_bot).await.unwrap();
    assert_eq!(res_bot.status(), StatusCode::OK);

    // Bot / client checks metadata: GET /v1/blobs/{blob_id}
    let req_meta = Request::builder()
        .method("GET")
        .uri(format!("/v1/blobs/{blob_id}"))
        .body(Body::empty())
        .unwrap();
    let res_meta = app.clone().oneshot(req_meta).await.unwrap();
    assert_eq!(res_meta.status(), StatusCode::OK);

    // Ensure blob is STILL available and not burned
    let meta_resp: ember_server::BlobMetadataResponse = response_json(res_meta).await;
    assert!(meta_resp.burn);

    // Now receiver claims drop: POST /v1/blobs/{blob_id}/claim
    let req_claim = Request::builder()
        .method("POST")
        .uri(format!("/v1/blobs/{blob_id}/claim"))
        .body(Body::empty())
        .unwrap();
    let res_claim = app.clone().oneshot(req_claim).await.unwrap();
    assert_eq!(res_claim.status(), StatusCode::OK);
    let claim_resp: ember_server::ClaimResponse = response_json(res_claim).await;

    // Receiver downloads part 0: GET /v1/blobs/{blob_id}/parts/0
    let req_down = Request::builder()
        .method("GET")
        .uri(format!("/v1/blobs/{blob_id}/parts/0"))
        .header("X-Claim", claim_resp.claim.as_deref().unwrap())
        .body(Body::empty())
        .unwrap();
    let res_down = app.clone().oneshot(req_down).await.unwrap();
    assert_eq!(res_down.status(), StatusCode::OK);

    // Receiver finishes burn: POST /v1/blobs/{blob_id}/finish
    let req_finish = Request::builder()
        .method("POST")
        .uri(format!("/v1/blobs/{blob_id}/finish"))
        .header("X-Claim", claim_resp.claim.as_deref().unwrap())
        .body(Body::empty())
        .unwrap();
    let res_finish = app.clone().oneshot(req_finish).await.unwrap();
    assert_eq!(res_finish.status(), StatusCode::OK);

    // Blob is now burned and returns 410 Gone
    let req_check = Request::builder()
        .method("GET")
        .uri(format!("/v1/blobs/{blob_id}"))
        .body(Body::empty())
        .unwrap();
    let res_check = app.clone().oneshot(req_check).await.unwrap();
    assert_eq!(res_check.status(), StatusCode::GONE);
}

#[tokio::test]
async fn test_mailbox_lifecycle_and_messaging() {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use sha2::{Digest, Sha256};

    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::new(storage.clone());
    let app = app(state.clone());

    let mb_id = generate_blob_id();
    let push_token = "push-secret-token-1234567890ab";
    let pull_token = "pull-secret-token-1234567890ab";

    let mut hasher = Sha256::new();
    hasher.update(push_token.as_bytes());
    let push_hash: [u8; 32] = hasher.finalize().into();

    let mut hasher = Sha256::new();
    hasher.update(pull_token.as_bytes());
    let pull_hash: [u8; 32] = hasher.finalize().into();

    // 1. Create mailbox: POST /v1/mailboxes
    let create_req = CreateMailboxRequest {
        id: mb_id.to_string(),
        push_token_hash: URL_SAFE_NO_PAD.encode(push_hash),
        pull_token_hash: URL_SAFE_NO_PAD.encode(pull_hash),
        ttl_secs: Some(3600),
        pow: None,
    };

    let res = create_mailbox_with_pow(&app, &create_req).await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // Idempotent creation with same tokens and fresh PoW returns 200 OK
    let res = create_mailbox_with_pow(&app, &create_req).await;
    assert_eq!(res.status(), StatusCode::OK);

    // 2. Push message with wrong token -> 401
    let msg_payload = URL_SAFE_NO_PAD.encode(b"hello world ciphertext");
    let push_body = serde_json::to_vec(&PushMessageRequest {
        payload: msg_payload.clone(),
    })
    .unwrap();

    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/mailboxes/{mb_id}/messages"))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer wrong-token")
        .body(Body::from(push_body.clone()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Push message with valid push_token -> 201
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/mailboxes/{mb_id}/messages"))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {push_token}"))
        .body(Body::from(push_body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let push_res: PushMessageResponse = response_json(res).await;
    assert_eq!(push_res.id, 1);

    // 3. Pull messages with wrong token -> 401
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/mailboxes/{mb_id}/messages"))
        .header("Authorization", "Bearer wrong-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Pull messages with valid pull_token -> 200 with 1 message
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/mailboxes/{mb_id}/messages"))
        .header("Authorization", format!("Bearer {pull_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let msgs_res: GetMessagesResponse = response_json(res).await;
    assert_eq!(msgs_res.messages.len(), 1);
    assert_eq!(msgs_res.messages[0].payload, msg_payload);

    // Pull again: queue was drained, so returns 0 messages
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/mailboxes/{mb_id}/messages?poll_timeout_secs=0"
        ))
        .header("Authorization", format!("Bearer {pull_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let msgs_res: GetMessagesResponse = response_json(res).await;
    assert_eq!(msgs_res.messages.len(), 0);

    // 4. Delete mailbox: DELETE /v1/mailboxes/{mb_id} with pull_token -> 204
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/mailboxes/{mb_id}"))
        .header("Authorization", format!("Bearer {pull_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Subsequent access returns 404
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/mailboxes/{mb_id}/messages"))
        .header("Authorization", format!("Bearer {pull_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_mailbox_push_5kb_payload_is_rejected() {
    use sha2::{Digest, Sha256};

    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::new(storage);
    let app = app(state);

    let mb_id = generate_blob_id();
    let push_tok = "push-token-test-5kb-secret";
    let push_hash = URL_SAFE_NO_PAD.encode(Sha256::digest(push_tok.as_bytes()));
    let pull_hash = URL_SAFE_NO_PAD.encode(Sha256::digest(b"pull-token-test-secret"));
    let reg_req = CreateMailboxRequest {
        id: mb_id.to_string(),
        push_token_hash: push_hash,
        pull_token_hash: pull_hash,
        ttl_secs: Some(3600),
        pow: None,
    };
    let res = create_mailbox_with_pow(&app, &reg_req).await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // 5 KB payload (5120 bytes) exceeds MAX_THREAD_ENVELOPE_BYTES (4633 bytes)
    let payload_5kb = vec![0x42u8; 5120];
    let b64_5kb = URL_SAFE_NO_PAD.encode(&payload_5kb);
    let push_body = PushMessageRequest { payload: b64_5kb };

    let push_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/mailboxes/{mb_id}/messages"))
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {push_tok}"))
                .body(Body::from(serde_json::to_vec(&push_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(push_res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn test_mailbox_end_to_end_thread_handshake_and_exchange() {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::new(storage.clone());
    let app = app(state.clone());

    // Both parties share the drop master secret key from the link fragment
    let drop_key = SecretKey::generate();

    // 1. Both derive the deterministic rendezvous mailbox
    let mb_rendezvous = derive_rendezvous_mailbox(&drop_key).unwrap();

    // Initiator registers rendezvous mailbox
    let reg_req = CreateMailboxRequest {
        id: mb_rendezvous.id.clone(),
        push_token_hash: URL_SAFE_NO_PAD.encode(mb_rendezvous.push_token_hash()),
        pull_token_hash: URL_SAFE_NO_PAD.encode(mb_rendezvous.pull_token_hash()),
        ttl_secs: Some(3600),
        pow: None,
    };
    let res = create_mailbox_with_pow(&app, &reg_req).await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // Initiator starts handshake, posts handshake message to rendezvous mailbox
    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_msg = init.handshake_message();

    let post_pk_body = serde_json::to_vec(&PushMessageRequest {
        payload: URL_SAFE_NO_PAD.encode(init_msg),
    })
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/mailboxes/{}/messages", mb_rendezvous.id))
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", mb_rendezvous.push_token),
        )
        .body(Body::from(post_pk_body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Responder pulls init_msg from rendezvous mailbox
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/mailboxes/{}/messages", mb_rendezvous.id))
        .header(
            "Authorization",
            format!("Bearer {}", mb_rendezvous.pull_token),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let msgs: GetMessagesResponse = response_json(res).await;
    assert_eq!(msgs.messages.len(), 1);
    let fetched_init_msg_bytes = URL_SAFE_NO_PAD.decode(&msgs.messages[0].payload).unwrap();

    // Responder responds to handshake
    let (resp_msg, resp_keys) =
        HandshakeResponder::respond(&drop_key, &fetched_init_msg_bytes).unwrap();

    // Responder posts its handshake message to rendezvous mailbox
    let post_resp_body = serde_json::to_vec(&PushMessageRequest {
        payload: URL_SAFE_NO_PAD.encode(&resp_msg),
    })
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/mailboxes/{}/messages", mb_rendezvous.id))
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", mb_rendezvous.push_token),
        )
        .body(Body::from(post_resp_body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Responder registers directional mailboxes on the server
    for mb in [&resp_keys.mailbox_out, &resp_keys.mailbox_in] {
        let reg = CreateMailboxRequest {
            id: mb.id.clone(),
            push_token_hash: URL_SAFE_NO_PAD.encode(mb.push_token_hash()),
            pull_token_hash: URL_SAFE_NO_PAD.encode(mb.pull_token_hash()),
            ttl_secs: Some(43200),
            pow: None,
        };
        let res = create_mailbox_with_pow(&app, &reg).await;
        assert!(res.status() == StatusCode::CREATED || res.status() == StatusCode::OK);
    }

    // Initiator pulls resp_msg from rendezvous mailbox
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/mailboxes/{}/messages", mb_rendezvous.id))
        .header(
            "Authorization",
            format!("Bearer {}", mb_rendezvous.pull_token),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let msgs: GetMessagesResponse = response_json(res).await;
    assert_eq!(msgs.messages.len(), 1);
    let fetched_resp_msg_bytes = URL_SAFE_NO_PAD.decode(&msgs.messages[0].payload).unwrap();

    // Initiator completes handshake
    let init_keys = init.complete(&fetched_resp_msg_bytes).unwrap();

    // Initiator cleans up rendezvous mailbox: DELETE /v1/mailboxes/{rendezvous_id}
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/mailboxes/{}", mb_rendezvous.id))
        .header(
            "Authorization",
            format!("Bearer {}", mb_rendezvous.pull_token),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Verify derived directional mailboxes match across initiator and responder
    assert_eq!(init_keys.mailbox_out.id, resp_keys.mailbox_in.id);
    assert_eq!(init_keys.mailbox_in.id, resp_keys.mailbox_out.id);

    // Initialize sessions for both parties
    let mut init_session = ThreadSession::new("amber-fox-4821".to_string(), init_keys);
    let mut resp_session = ThreadSession::new("silver-owl-1934".to_string(), resp_keys);

    // 2. Initiator sends message 1 to responder
    let ct1 = init_session.encrypt_text("Hello from initiator!").unwrap();
    let ct1_bytes = ct1.to_bytes();

    let post_msg_body = serde_json::to_vec(&PushMessageRequest {
        payload: URL_SAFE_NO_PAD.encode(&ct1_bytes),
    })
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri(format!(
            "/v1/mailboxes/{}/messages",
            init_session.mailbox_out.id
        ))
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", init_session.mailbox_out.push_token),
        )
        .body(Body::from(post_msg_body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Responder polls mailbox_in, receives ciphertext, decrypts
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/mailboxes/{}/messages",
            resp_session.mailbox_in.id
        ))
        .header(
            "Authorization",
            format!("Bearer {}", resp_session.mailbox_in.pull_token),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let msgs: GetMessagesResponse = response_json(res).await;
    assert_eq!(msgs.messages.len(), 1);

    let received_payload = URL_SAFE_NO_PAD.decode(&msgs.messages[0].payload).unwrap();
    let received_ct =
        ember_core::thread::EncryptedMessagePayload::from_bytes(&received_payload).unwrap();
    let decrypted_msg1 = resp_session.decrypt_message(&received_ct).unwrap();
    assert_eq!(decrypted_msg1.sender_handle, "amber-fox-4821");
    assert_eq!(decrypted_msg1.text, "Hello from initiator!");

    // 3. Responder replies to initiator
    let ct2 = resp_session
        .encrypt_text("Hello back from responder!")
        .unwrap();
    let ct2_bytes = ct2.to_bytes();

    let post_reply_body = serde_json::to_vec(&PushMessageRequest {
        payload: URL_SAFE_NO_PAD.encode(&ct2_bytes),
    })
    .unwrap();
    let req = Request::builder()
        .method("POST")
        .uri(format!(
            "/v1/mailboxes/{}/messages",
            resp_session.mailbox_out.id
        ))
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", resp_session.mailbox_out.push_token),
        )
        .body(Body::from(post_reply_body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Initiator pulls from mailbox_in, decrypts reply
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/mailboxes/{}/messages",
            init_session.mailbox_in.id
        ))
        .header(
            "Authorization",
            format!("Bearer {}", init_session.mailbox_in.pull_token),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let msgs: GetMessagesResponse = response_json(res).await;
    assert_eq!(msgs.messages.len(), 1);

    let reply_payload = URL_SAFE_NO_PAD.decode(&msgs.messages[0].payload).unwrap();
    let reply_ct = ember_core::thread::EncryptedMessagePayload::from_bytes(&reply_payload).unwrap();
    let decrypted_msg2 = init_session.decrypt_message(&reply_ct).unwrap();
    assert_eq!(decrypted_msg2.sender_handle, "silver-owl-1934");
    assert_eq!(decrypted_msg2.text, "Hello back from responder!");

    // Replay attack: submitting old ct1 again to responder fails with ReplayDetected
    assert!(resp_session.decrypt_message(&received_ct).is_err());

    // 4. Close thread by either side: Initiator deletes mailbox_out
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/mailboxes/{}", init_session.mailbox_out.id))
        .header(
            "Authorization",
            format!("Bearer {}", init_session.mailbox_out.push_token),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Mailbox is deleted and gone
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/mailboxes/{}/messages",
            init_session.mailbox_out.id
        ))
        .header(
            "Authorization",
            format!("Bearer {}", resp_session.mailbox_in.pull_token),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_takedown_removes_blob_and_mailbox() {
    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::new(storage.clone()).with_admin_token("test-admin-token-1234");
    let test_app = app(state);

    // 1. Create a blob
    let blob_req = CreateBlobRequest {
        size: 8250,
        parts: 2,
        burn: false,
        ttl_secs: Some(3600),
        pow: None,
    };
    let res = create_blob_with_pow(&test_app, &blob_req).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let blob: CreateBlobResponse = response_json(res).await;

    // Upload part 0
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/blobs/{}/parts/0", blob.id))
        .header("Authorization", format!("Bearer {}", blob.delete_token))
        .body(Body::from(vec![42u8; 4137]))
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Upload part 1 (a minimal 4096-byte content chunk plus 17 bytes of overhead)
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/blobs/{}/parts/1", blob.id))
        .header("Authorization", format!("Bearer {}", blob.delete_token))
        .body(Body::from(vec![43u8; 4113]))
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Complete blob
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/blobs/{}/complete", blob.id))
        .header("Authorization", format!("Bearer {}", blob.delete_token))
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify blob is accessible
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/blobs/{}", blob.id))
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 2. Create a mailbox
    let push_token = "admin-mb-push-token-1234567890ab";
    let pull_token = "admin-mb-pull-token-1234567890ab";
    let push_hash = Sha256::digest(push_token.as_bytes());
    let pull_hash = Sha256::digest(pull_token.as_bytes());
    let mb_id = generate_blob_id();
    let mb_req = CreateMailboxRequest {
        id: mb_id.clone(),
        push_token_hash: URL_SAFE_NO_PAD.encode(push_hash),
        pull_token_hash: URL_SAFE_NO_PAD.encode(pull_hash),
        ttl_secs: Some(3600),
        pow: None,
    };
    let res = create_mailbox_with_pow(&test_app, &mb_req).await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // 3. Unauthorized takedown attempts
    let takedown_payload = AdminTakedownRequest {
        target_id: blob.id.clone(),
        target_type: "blob".to_string(),
        request_ref: "COURT-ORD-2026-001".to_string(),
        operator: "op-test".to_string(),
        reason: "court_order".to_string(),
    };
    // No auth header -> 401
    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/takedown")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&takedown_payload).unwrap()))
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Wrong auth header -> 401
    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/takedown")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer wrong-token")
        .body(Body::from(serde_json::to_vec(&takedown_payload).unwrap()))
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 4. Authorized takedown of blob
    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/takedown")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test-admin-token-1234")
        .body(Body::from(serde_json::to_vec(&takedown_payload).unwrap()))
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let record: TakedownRecord = response_json(res).await;
    assert_eq!(record.target_id, blob.id);
    assert_eq!(record.action_taken, "deleted");
    assert_eq!(record.operator, "op-test");

    // Assert blob is now GONE (410)
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/blobs/{}", blob.id))
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::GONE);

    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/blobs/{}/parts/0", blob.id))
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::GONE);

    // 5. Authorized takedown of mailbox
    let mb_takedown = AdminTakedownRequest {
        target_id: mb_id.clone(),
        target_type: "mailbox".to_string(),
        request_ref: "NCII-EMERGENCY-002".to_string(),
        operator: "op-test".to_string(),
        reason: "ncii".to_string(),
    };
    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/takedown")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test-admin-token-1234")
        .body(Body::from(serde_json::to_vec(&mb_takedown).unwrap()))
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Assert mailbox is now NOT FOUND (404)
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/mailboxes/{}/messages", mb_id))
        .header("Authorization", format!("Bearer {pull_token}"))
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Assert takedown records in storage
    let takedowns = storage.list_takedowns().await.unwrap();
    assert_eq!(takedowns.len(), 2);
}

#[tokio::test]
async fn test_user_report_endpoint_and_transparency() {
    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::new(storage.clone()).with_admin_token("reviewer-secret-key-999");
    let test_app = app(state);

    // 1. Submit public report
    let report_req = SubmitReportRequest {
        target_id: "test-blob-abuse-id-1".to_string(),
        target_type: "blob".to_string(),
        reason: "malware".to_string(),
        notes: Some("Contains malicious payload".to_string()),
        content: Some("Malicious script snippet...".to_string()),
        link_key: Some("test-voluntarily-shared-key-123".to_string()),
    };
    let req = Request::builder()
        .method("POST")
        .uri("/v1/reports")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&report_req).unwrap()))
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let submit_res: SubmitReportResponse = response_json(res).await;
    assert_eq!(submit_res.status, "submitted");

    // 2. Fetch reports: Unauthenticated is rejected
    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/reports")
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 3. Fetch reports: Reviewer is authorized
    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/reports")
        .header("Authorization", "Bearer reviewer-secret-key-999")
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let reports: Vec<ReportRecord> = response_json(res).await;
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].target_id, "test-blob-abuse-id-1");
    assert_eq!(reports[0].reason, "malware");
    assert_eq!(
        reports[0].link_key.as_deref(),
        Some("test-voluntarily-shared-key-123")
    );

    // 4. Transparency report (public, zero personal data)
    let req = Request::builder()
        .method("GET")
        .uri("/v1/transparency")
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let stats: TransparencyStats = response_json(res).await;
    assert_eq!(stats.total_reports, 1);
    assert_eq!(stats.reports_by_reason.get("malware"), Some(&1));
}

#[tokio::test]
async fn test_reports_encrypted_at_rest_with_reviewer_public_key() {
    let temp_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(FilesystemStorage::new(temp_dir.path()).await.unwrap());

    let (reviewer_pk, reviewer_sk) = ember_core::generate_reviewer_keypair();
    let state = AppState::new(storage)
        .with_admin_token("test-reviewer-admin-token")
        .with_reviewer_public_key(Some(reviewer_pk));
    let test_app = app(state);

    let confidential_content = "CANARY_CONFIDENTIAL_USER_REPORT_EVIDENCE_12345";
    let confidential_key = "CANARY_CONFIDENTIAL_LINK_KEY_67890";

    // 1. Submit report with sensitive fields
    let report_req = SubmitReportRequest {
        target_id: "target-blob-id-99".to_string(),
        target_type: "blob".to_string(),
        reason: "ncii".to_string(),
        notes: Some("Public moderator note".to_string()),
        content: Some(confidential_content.to_string()),
        link_key: Some(confidential_key.to_string()),
    };
    let req = Request::builder()
        .method("POST")
        .uri("/v1/reports")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&report_req).unwrap()))
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let submit_res: SubmitReportResponse = response_json(res).await;

    // 2. Inspect file on disk: Plaintext CANNOT appear on disk
    let report_file = temp_dir
        .path()
        .join(".reports")
        .join(format!("{}.json", submit_res.id));
    assert!(report_file.exists());
    let raw_disk = std::fs::read_to_string(&report_file).unwrap();
    assert!(
        !raw_disk.contains(confidential_content),
        "Plaintext content leaked to disk!"
    );
    assert!(
        !raw_disk.contains(confidential_key),
        "Plaintext link key leaked to disk!"
    );
    assert!(
        raw_disk.contains("sealed:"),
        "Disk file missing sealed ciphertext prefix"
    );
    assert!(raw_disk.contains("\"encrypted\": true"));

    // 3. Fetch from /v1/admin/reports
    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/reports")
        .header("Authorization", "Bearer test-reviewer-admin-token")
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let reports: Vec<ReportRecord> = response_json(res).await;
    let found = reports
        .iter()
        .find(|r| r.id == submit_res.id)
        .expect("Report found");
    assert!(found.encrypted);

    let sealed_content = found.content.as_ref().unwrap();
    let sealed_key = found.link_key.as_ref().unwrap();
    assert!(sealed_content.starts_with("sealed:"));
    assert!(sealed_key.starts_with("sealed:"));

    // 4. Decrypt with correct reviewer key
    let dec_content =
        ember_core::unseal_report_field(sealed_content, &reviewer_pk, &reviewer_sk).unwrap();
    assert_eq!(
        String::from_utf8(dec_content).unwrap(),
        confidential_content
    );

    let dec_key = ember_core::unseal_report_field(sealed_key, &reviewer_pk, &reviewer_sk).unwrap();
    assert_eq!(String::from_utf8(dec_key).unwrap(), confidential_key);

    // 5. Decrypt with wrong key fails
    let (wrong_pk, wrong_sk) = ember_core::generate_reviewer_keypair();
    assert_eq!(
        ember_core::unseal_report_field(sealed_content, &wrong_pk, &wrong_sk).err(),
        Some(ember_core::EmberError::Auth)
    );
}

#[tokio::test]
async fn test_privacy_logs_never_contain_secrets() {
    let storage = Arc::new(MemoryStorage::new());
    let recorder = AccessLogRecorder::new();
    let state = AppState::new(storage.clone())
        .with_admin_token("test-admin-token")
        .with_access_logger(recorder.clone());
    let test_app = app(state);

    let canary_key = "CANARY_KEY_SECRET_9876543210zyxwvutsrqp";
    let canary_fragment = "CANARY_FRAGMENT_ABCDEF0123456789";
    let canary_filename = "CANARY_FILENAME_top_secret_sensitive_plan.pdf";
    let canary_content = "CANARY_PLAINTEXT_CONTENT_super_confidential_message";
    let canary_passphrase = "CANARY_PASSPHRASE_correct-horse-battery-staple";

    // 1. Create a drop containing all canary secrets in payload/metadata
    let key = SecretKey::generate();
    let metadata = DropMetadata::new_file(
        canary_filename,
        "application/pdf",
        canary_content.len() as u64,
        false,
        1_700_000_000,
        1_700_043_200,
    );
    let parts = encrypt_drop_to_parts(&key, &metadata, canary_content.as_bytes()).unwrap();

    let total_size: u64 = parts.iter().map(|p| p.len() as u64).sum();
    let create_req = CreateBlobRequest {
        size: total_size,
        parts: parts.len() as u32,
        burn: false,
        ttl_secs: Some(3600),
        pow: None,
    };
    let res = create_blob_with_pow(&test_app, &create_req).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let blob: CreateBlobResponse = response_json(res).await;

    // Upload all parts
    for (i, part) in parts.iter().enumerate() {
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/v1/blobs/{}/parts/{i}", blob.id))
            .header("Authorization", format!("Bearer {}", blob.delete_token))
            .body(Body::from(part.clone()))
            .unwrap();
        let res = test_app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    // Complete blob
    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/blobs/{}/complete", blob.id))
        .header("Authorization", format!("Bearer {}", blob.delete_token))
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Get metadata
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/blobs/{}", blob.id))
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Download part 0
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/blobs/{}/parts/0", blob.id))
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Create a mailbox
    let mb_id = generate_blob_id();
    let mb_push_hash = Sha256::digest(b"canary-push-token-123456789012");
    let mb_pull_hash = Sha256::digest(b"canary-pull-token-123456789012");
    let mb_req = CreateMailboxRequest {
        id: mb_id.clone(),
        push_token_hash: URL_SAFE_NO_PAD.encode(mb_push_hash),
        pull_token_hash: URL_SAFE_NO_PAD.encode(mb_pull_hash),
        ttl_secs: Some(3600),
        pow: None,
    };
    let res = create_mailbox_with_pow(&test_app, &mb_req).await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // Submit report with canaries
    let report_req = SubmitReportRequest {
        target_id: blob.id.clone(),
        target_type: "blob".to_string(),
        reason: "abuse".to_string(),
        notes: Some(format!("Found canary in drop {}", canary_filename)),
        content: Some(canary_content.to_string()),
        link_key: Some(canary_key.to_string()),
    };
    let req = Request::builder()
        .method("POST")
        .uri("/v1/reports")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&report_req).unwrap()))
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Get transparency stats
    let req = Request::builder()
        .method("GET")
        .uri("/v1/transparency")
        .body(Body::empty())
        .unwrap();
    let res = test_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Inspect all captured access log entries
    let entries = recorder.entries();
    assert!(!entries.is_empty(), "Log recorder should capture requests");

    for entry in &entries {
        let line = entry.format_line();

        // 1. Assert NO secrets leak into any log line
        assert!(!line.contains(canary_key), "Key leaked in log: {line}");
        assert!(
            !line.contains(canary_fragment),
            "Link fragment leaked in log: {line}"
        );
        assert!(
            !line.contains(canary_filename),
            "File name leaked in log: {line}"
        );
        assert!(
            !line.contains(canary_content),
            "Plaintext content leaked in log: {line}"
        );
        assert!(
            !line.contains(canary_passphrase),
            "Passphrase leaked in log: {line}"
        );

        // 2. Assert NO blob IDs leak into the route template
        assert!(
            !entry.route_template.contains(&blob.id),
            "Blob ID leaked in route template: {}",
            entry.route_template
        );
        assert!(
            !entry.route_template.contains(&mb_id),
            "Mailbox ID leaked in route template: {}",
            entry.route_template
        );

        // 3. Assert route template is an approved static route template
        assert!(
            entry.route_template.starts_with('/') && !entry.route_template.contains('?'),
            "Invalid route template: {}",
            entry.route_template
        );

        // 4. Assert size bucket is coarse
        assert!(
            matches!(
                entry.size_bucket,
                "<1KB" | "1KB-64KB" | "64KB-1MB" | "1MB-50MB" | ">50MB"
            ),
            "Unapproved size bucket: {}",
            entry.size_bucket
        );
    }
}

#[tokio::test]
async fn test_admin_cli_binary_execution() {
    let admin_bin = find_ember_admin_bin();
    if !admin_bin.exists() {
        eprintln!(
            "Skipping test_admin_cli_binary_execution: ember-admin binary not found at {:?}",
            admin_bin
        );
        return;
    }

    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::new(storage.clone()).with_admin_token("test-cli-admin-token");
    let router = app(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://{}", addr);

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // 1. Create a dummy blob to takedown
    let blob_id = generate_blob_id();
    storage
        .create_blob(&ember_server::BlobRecord {
            id: blob_id.clone(),
            delete_token_hash: [0u8; 32],
            size: 4096,
            parts: 1,
            burn: false,
            created_at: 1000,
            expires_at: 50000,
            state: ember_server::BlobState::Available,
            claim_token_hash: None,
            claim_expires_at: None,
        })
        .await
        .unwrap();

    // 2. Test takedown via CLI
    let output = Command::new(&admin_bin)
        .args([
            "takedown",
            "--server",
            &server_url,
            "--token",
            "test-cli-admin-token",
            &format!("--target={blob_id}"),
            "--target-type",
            "blob",
            "--order-ref",
            "TEST-ORD-001",
            "--operator",
            "test-operator",
            "--reason",
            "court_order",
        ])
        .output()
        .await
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "admin takedown failed: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Takedown successful"));

    // 3. Test reports via CLI
    let output = Command::new(&admin_bin)
        .args([
            "reports",
            "--server",
            &server_url,
            "--token",
            "test-cli-admin-token",
        ])
        .output()
        .await
        .unwrap();
    assert!(output.status.success());

    // 4. Test transparency via CLI
    let output = Command::new(&admin_bin)
        .args(["transparency", "--server", &server_url, "--json"])
        .output()
        .await
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("total_takedowns"));

    // 5. Test generate-reviewer-keys via CLI
    let output = Command::new(&admin_bin)
        .arg("generate-reviewer-keys")
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Reviewer Keypair Generated"));
    assert!(stdout.contains("Public Key (hex):"));
    assert!(stdout.contains("Secret Key (hex):"));

    // 6. Test decrypt-report via CLI with sealed string
    let (rev_pk, rev_sk) = ember_core::generate_reviewer_keypair();
    let pk_hex = ember_core::key_to_hex(&rev_pk);
    let sk_hex = ember_core::key_to_hex(&rev_sk);
    let sample_text = "Highly confidential evidence string";
    let sealed_str = ember_core::seal_report_field(sample_text.as_bytes(), &rev_pk).unwrap();

    let output = Command::new(&admin_bin)
        .args([
            "decrypt-report",
            "--secret-key",
            &sk_hex,
            "--public-key",
            &pk_hex,
            "--sealed",
            &sealed_str,
        ])
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(sample_text));

    server_handle.abort();
}

#[tokio::test]
async fn test_admin_listen_interface_isolation() {
    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::new(storage).with_admin_token("isolated-admin-secret");

    let public_app = ember_server::public_app(state.clone());
    let admin_app = ember_server::admin_app(state.clone());

    // 1. Public app must serve public routes (e.g. /v1/pow/challenge, /v1/transparency)
    let req = Request::builder()
        .method("GET")
        .uri("/v1/pow/challenge")
        .body(Body::empty())
        .unwrap();
    let res = public_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/transparency")
        .body(Body::empty())
        .unwrap();
    let res = public_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 2. Public app MUST NOT serve admin routes (returns 404 NOT_FOUND)
    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/reports")
        .header("Authorization", "Bearer isolated-admin-secret")
        .body(Body::empty())
        .unwrap();
    let res = public_app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "Admin route should not be exposed on public interface"
    );

    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/takedown")
        .header("Authorization", "Bearer isolated-admin-secret")
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let res = public_app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "Admin takedown should not be exposed on public interface"
    );

    // 3. Admin app MUST serve admin routes
    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/reports")
        .header("Authorization", "Bearer isolated-admin-secret")
        .body(Body::empty())
        .unwrap();
    let res = admin_app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 4. Admin app MUST NOT serve public drop routes
    let req = Request::builder()
        .method("GET")
        .uri("/v1/pow/challenge")
        .body(Body::empty())
        .unwrap();
    let res = admin_app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "Public route should not be exposed on admin interface"
    );
}

#[tokio::test]
async fn test_server_cli_startup_with_explicit_host_port_storage_and_pow() {
    let temp_dir = tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let server_bin = find_ember_server_bin();
    if !server_bin.exists() {
        eprintln!(
            "ember-server binary not found at {:?}, skipping",
            server_bin
        );
        return;
    }

    let access_log_path = temp_dir.path().join("access.log");
    let mut child = Command::new(&server_bin)
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--storage-dir",
            temp_dir.path().to_str().unwrap(),
            "--access-log-file",
            access_log_path.to_str().unwrap(),
        ])
        .spawn()
        .expect("Failed to start ember-server binary");

    let client = reqwest::Client::new();
    let pow_url = format!("http://127.0.0.1:{port}/v1/pow/challenge");

    let mut ready = false;
    for _ in 0..50 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Ok(resp) = client.get(&pow_url).send().await {
            if resp.status().is_success() {
                let body: serde_json::Value = resp.json().await.unwrap();
                assert!(body.get("challenge").is_some());
                assert!(body.get("difficulty").is_some());
                ready = true;
                break;
            }
        }
    }

    let _ = child.kill().await;
    assert!(
        ready,
        "Server failed to start and respond to PoW challenge at {pow_url}"
    );
}

#[tokio::test]
async fn test_mailbox_creation_pow_enforcement_and_recovery() {
    let storage = Arc::new(MemoryStorage::new());
    let state =
        AppState::with_pow_and_rate_limit(storage, 8, ember_server::RateLimitConfig::default());
    let app = app(state);

    let mb_id = generate_blob_id();
    let push_hash = Sha256::digest(b"push-token-test-123456789012");
    let pull_hash = Sha256::digest(b"pull-token-test-123456789012");

    let req_no_pow = CreateMailboxRequest {
        id: mb_id.clone(),
        push_token_hash: URL_SAFE_NO_PAD.encode(push_hash),
        pull_token_hash: URL_SAFE_NO_PAD.encode(pull_hash),
        ttl_secs: Some(3600),
        pow: None,
    };

    // 1. Missing PoW -> 400 Bad Request (pow_required)
    let req = Request::builder()
        .method("POST")
        .uri("/v1/mailboxes")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&req_no_pow).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body_str = String::from_utf8(
        axum::body::to_bytes(res.into_body(), 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        body_str.contains("pow_required"),
        "Response must indicate pow_required: {body_str}"
    );

    // 2. Invalid PoW nonce -> 400 Bad Request (invalid_pow)
    let ch_req = Request::builder()
        .method("GET")
        .uri("/v1/pow/challenge")
        .body(Body::empty())
        .unwrap();
    let ch_res = app.clone().oneshot(ch_req).await.unwrap();
    let pow_resp: ember_server::PowChallengeResponse = response_json(ch_res).await;

    let req_invalid_pow = CreateMailboxRequest {
        id: mb_id.clone(),
        push_token_hash: URL_SAFE_NO_PAD.encode(push_hash),
        pull_token_hash: URL_SAFE_NO_PAD.encode(pull_hash),
        ttl_secs: Some(3600),
        pow: Some(ember_server::PowSolutionPayload {
            challenge: pow_resp.challenge.clone(),
            nonce: 999_999_999,
        }),
    };
    let req = Request::builder()
        .method("POST")
        .uri("/v1/mailboxes")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&req_invalid_pow).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body_str = String::from_utf8(
        axum::body::to_bytes(res.into_body(), 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        body_str.contains("invalid_pow"),
        "Response must indicate invalid_pow: {body_str}"
    );

    // 3. Valid PoW solution -> 201 Created
    let valid_nonce = ember_core::solve_pow(
        pow_resp.challenge.as_bytes(),
        pow_resp.difficulty,
        Some(5_000_000),
    )
    .unwrap();
    let req_valid_pow = CreateMailboxRequest {
        id: mb_id.clone(),
        push_token_hash: URL_SAFE_NO_PAD.encode(push_hash),
        pull_token_hash: URL_SAFE_NO_PAD.encode(pull_hash),
        ttl_secs: Some(3600),
        pow: Some(ember_server::PowSolutionPayload {
            challenge: pow_resp.challenge.clone(),
            nonce: valid_nonce,
        }),
    };
    let req = Request::builder()
        .method("POST")
        .uri("/v1/mailboxes")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&req_valid_pow).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // 4. Replay of same PoW challenge & nonce -> 400 Bad Request (pow_replayed)
    let mb2_id = generate_blob_id();
    let req_replayed = CreateMailboxRequest {
        id: mb2_id.clone(),
        push_token_hash: URL_SAFE_NO_PAD.encode(push_hash),
        pull_token_hash: URL_SAFE_NO_PAD.encode(pull_hash),
        ttl_secs: Some(3600),
        pow: Some(ember_server::PowSolutionPayload {
            challenge: pow_resp.challenge,
            nonce: valid_nonce,
        }),
    };
    let req = Request::builder()
        .method("POST")
        .uri("/v1/mailboxes")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&req_replayed).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body_str = String::from_utf8(
        axum::body::to_bytes(res.into_body(), 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        body_str.contains("pow_replayed"),
        "Response must indicate pow_replayed: {body_str}"
    );

    // 5. Recovery: fresh challenge and solution succeeds -> 201 Created
    let req_recovered = CreateMailboxRequest {
        id: mb2_id,
        push_token_hash: URL_SAFE_NO_PAD.encode(push_hash),
        pull_token_hash: URL_SAFE_NO_PAD.encode(pull_hash),
        ttl_secs: Some(3600),
        pow: None,
    };
    let res = create_mailbox_with_pow(&app, &req_recovered).await;
    assert_eq!(res.status(), StatusCode::CREATED);
}
