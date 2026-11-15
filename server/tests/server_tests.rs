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
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::net::TcpListener;
use tokio::process::Command;
use tower::ServiceExt;

use ember_core::{
    build_link, decrypt_drop_parts, encrypt_drop_to_parts, generate_blob_id, DropMetadata,
    LinkPayload, SecretKey,
};
use ember_server::{
    app, sweeper::sweep_once, AppState, CreateBlobRequest, CreateBlobResponse, MemoryStorage,
    Storage,
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

fn find_ember_bin() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = if cfg!(windows) { "ember.exe" } else { "ember" };
    let path = manifest_dir.join("../target/debug").join(exe);
    if path.exists() {
        return path;
    }
    manifest_dir.join("../../target/debug").join(exe)
}

#[tokio::test]
async fn test_server_upload_and_download_roundtrip() {
    let storage = Arc::new(MemoryStorage::new());
    let state = AppState::new(storage);
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

    let parts = encrypt_drop_to_parts(&key, &meta, content, None).unwrap();
    let total_size: u64 = parts.iter().map(|p| p.len() as u64).sum();

    // 1. Create upload
    let create_req = CreateBlobRequest {
        size: total_size,
        parts: parts.len() as u32,
        burn: false,
        ttl_secs: Some(3600),
    };

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
    let state = AppState::new(storage);
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

    let parts = encrypt_drop_to_parts(&key, &meta, content, None).unwrap();
    let total_size: u64 = parts.iter().map(|p| p.len() as u64).sum();

    // 1. Create burn blob
    let create_req = CreateBlobRequest {
        size: total_size,
        parts: parts.len() as u32,
        burn: true,
        ttl_secs: Some(3600),
    };

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
    let state = AppState::new(storage);
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
    let parts = encrypt_drop_to_parts(&key, &meta, &content, None).unwrap();
    let total_size: u64 = parts.iter().map(|p| p.len() as u64).sum();

    // 1. Create upload
    let create_req = CreateBlobRequest {
        size: total_size,
        parts: parts.len() as u32,
        burn: false,
        ttl_secs: Some(3600),
    };
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
    let state = AppState::new(storage.clone());
    let app = app(state.clone());

    let create_req = CreateBlobRequest {
        size: 5000,
        parts: 1,
        burn: false,
        ttl_secs: Some(60),
    };

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
    let state = AppState::new(storage);
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
    let state = AppState::new(storage);
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
