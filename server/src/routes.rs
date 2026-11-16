//! HTTP route handlers for Ember relay server.

use axum::body::Bytes;
use axum::extract::{ConnectInfo, Extension, Path as AxumPath, Query, State};
use axum::http::header::{HeaderMap, AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, RANGE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use dryoc::rng::copy_randombytes;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use ember_core::{generate_blob_id, validate_blob_id};
use ember_protocol::{ABYTES, MAX_LIFETIME_SECS, MAX_PART_SIZE, MAX_THREAD_MSG_BYTES, PART0_SIZE};

use crate::error::ServerError;
use crate::model::{
    AdminTakedownRequest, BlobMetadataResponse, BlobRecord, BlobState, ClaimResponse,
    CreateBlobRequest, CreateBlobResponse, CreateMailboxRequest, GetMessagesResponse,
    MailboxMessageItem, MailboxRecord, PushMessageRequest, PushMessageResponse, ReportRecord,
    SubmitReportRequest, SubmitReportResponse, TakedownRecord,
};
use crate::state::{now_secs, resolve_client_ip, AppState};

type Peer = Option<Extension<ConnectInfo<std::net::SocketAddr>>>;

const INDEX_HTML: &str = include_str!("../../web-receiver/dist/index.html");
const STYLE_CSS: &str = include_str!("../../web-receiver/dist/style.css");
const APP_JS: &str = include_str!("../../web-receiver/dist/app.js");

const CSP_HEADER: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data: blob:; base-uri 'none'; form-action 'none'; frame-ancestors 'none';";

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/d/{id}", get(serve_drop_page))
        .route("/style.css", get(serve_css))
        .route("/app.js", get(serve_js))
        .route("/v1/pow/challenge", get(get_pow_challenge))
        .route("/v1/blobs", post(create_blob))
        .route("/v1/blobs/{id}", get(get_metadata).delete(delete_blob))
        .route(
            "/v1/blobs/{id}/parts/{n}",
            put(upload_part).get(download_part),
        )
        .route("/v1/blobs/{id}/complete", post(complete_blob))
        .route("/v1/blobs/{id}/claim", post(claim_blob))
        .route("/v1/blobs/{id}/finish", post(finish_blob))
        .route("/v1/mailboxes", post(create_mailbox))
        .route("/v1/mailboxes/{id}", delete(delete_mailbox))
        .route(
            "/v1/mailboxes/{id}/messages",
            post(push_message).get(get_messages),
        )
        .route("/v1/reports", post(submit_report))
        .route("/v1/transparency", get(get_transparency_stats))
        .route("/v1/admin/takedown", post(admin_takedown))
        .route("/v1/admin/reports", get(admin_list_reports))
        .with_state(state)
}

async fn serve_index() -> Response {
    let mut resp = (StatusCode::OK, INDEX_HTML).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(CSP_HEADER),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp
}

async fn serve_drop_page(AxumPath(_id): AxumPath<String>) -> Response {
    serve_index().await
}

async fn serve_css() -> Response {
    let mut resp = (StatusCode::OK, STYLE_CSS).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    resp
}

async fn serve_js() -> Response {
    let mut resp = (StatusCode::OK, APP_JS).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/javascript; charset=utf-8"),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    resp
}

fn client_ip(state: &AppState, peer: &Peer, headers: &HeaderMap) -> std::net::IpAddr {
    resolve_client_ip(
        &state.trusted_proxies,
        peer.as_ref().map(|Extension(ConnectInfo(addr))| *addr),
        headers,
    )
}

/// GET /v1/pow/challenge
async fn get_pow_challenge(State(state): State<AppState>) -> Result<Response, ServerError> {
    let (challenge, difficulty, expires_at) = state.pow.issue_challenge();
    let resp = crate::model::PowChallengeResponse {
        challenge,
        difficulty,
        expires_at,
    };
    let mut response = Json(resp).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn hash_token(token: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.finalize().into()
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<String, ServerError> {
    let auth_header = headers
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(ServerError::Unauthorized)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
        .ok_or(ServerError::Unauthorized)?
        .trim();

    if token.is_empty() {
        return Err(ServerError::Unauthorized);
    }

    Ok(token.to_string())
}

fn extract_claim_token(headers: &HeaderMap) -> Result<String, ServerError> {
    let claim_header = headers
        .get("X-Claim")
        .or_else(|| headers.get("x-claim"))
        .and_then(|h| h.to_str().ok())
        .ok_or(ServerError::Unauthorized)?;

    let token = claim_header.trim();
    if token.is_empty() {
        return Err(ServerError::Unauthorized);
    }

    Ok(token.to_string())
}

async fn check_lazy_expiry(state: &AppState, record: &BlobRecord) -> Result<(), ServerError> {
    let now = now_secs();

    // Check hard expiry
    if now >= record.expires_at {
        let _ = state
            .storage
            .record_tombstone(&record.id, record.expires_at + 3600)
            .await;
        let _ = state.storage.delete_blob(&record.id).await;
        return Err(ServerError::Gone);
    }

    // Check burn claim window expiry
    if record.burn {
        if let Some(claim_exp) = record.claim_expires_at {
            if now >= claim_exp {
                let _ = state.storage.record_tombstone(&record.id, now + 3600).await;
                let _ = state.storage.delete_blob(&record.id).await;
                return Err(ServerError::Gone);
            }
        }
    }

    // Check uncompleted upload timeout (1 hour)
    if record.state == BlobState::Uploading && now >= record.created_at + state.upload_timeout_secs
    {
        let _ = state.storage.delete_blob(&record.id).await;
        return Err(ServerError::Gone);
    }

    Ok(())
}

async fn lookup_blob(state: &AppState, id: &str) -> Result<BlobRecord, ServerError> {
    match state.storage.get_blob(id).await? {
        Some(record) => {
            check_lazy_expiry(state, &record).await?;
            Ok(record)
        }
        None => {
            if state.storage.is_tombstone(id).await? {
                Err(ServerError::Gone)
            } else {
                Err(ServerError::NotFound)
            }
        }
    }
}

/// POST /v1/blobs
async fn create_blob(
    State(state): State<AppState>,
    peer: Peer,
    headers: HeaderMap,
    Json(req): Json<CreateBlobRequest>,
) -> Result<Response, ServerError> {
    // 1. Check coarse IP rate limiter
    let client_ip = client_ip(&state, &peer, &headers);
    state.rate_limiter.check_and_record(client_ip).await?;

    // 2. Extract and verify Proof of Work
    let (challenge, nonce) = if let Some(ref pow) = req.pow {
        (pow.challenge.clone(), pow.nonce)
    } else {
        let ch = headers
            .get("X-Pow-Challenge")
            .or_else(|| headers.get("x-pow-challenge"))
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| {
                ServerError::PowRequired(
                    "Proof of work required: missing X-Pow-Challenge header or body.pow".into(),
                )
            })?;
        let n_str = headers
            .get("X-Pow-Nonce")
            .or_else(|| headers.get("x-pow-nonce"))
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| {
                ServerError::PowRequired(
                    "Proof of work required: missing X-Pow-Nonce header or body.pow".into(),
                )
            })?;
        let n = n_str.trim().parse::<u64>().map_err(|_| {
            ServerError::PowInvalid(
                "X-Pow-Nonce header must be a valid 64-bit unsigned integer".into(),
            )
        })?;
        (ch.trim().to_string(), n)
    };

    state.pow.verify_solution(&challenge, nonce).await?;

    if req.size == 0 {
        return Err(ServerError::BadRequest(
            "Size must be greater than 0".to_string(),
        ));
    }

    if req.size > state.max_total_bytes {
        return Err(ServerError::PayloadTooLarge(format!(
            "Size {} exceeds cap {}",
            req.size, state.max_total_bytes
        )));
    }

    if req.parts == 0 {
        return Err(ServerError::BadRequest(
            "Parts count must be at least 1".to_string(),
        ));
    }

    // The declared part count must be consistent with the declared total size, otherwise a
    // client could reserve huge numbers of tiny parts (and make `complete` loop for ages).
    // Part 0 is exactly PART0_SIZE; every further part is one encrypted chunk of between
    // 4096 and 1 MiB of plaintext plus the 17-byte authentication overhead.
    let extra_parts = u64::from(req.parts) - 1;
    let min_size = PART0_SIZE as u64 + extra_parts * (4096 + ABYTES as u64);
    let max_size = PART0_SIZE as u64 + extra_parts * MAX_PART_SIZE as u64;
    if extra_parts == 0 || req.size < min_size || req.size > max_size {
        return Err(ServerError::BadRequest(
            "Declared size and part count are inconsistent".to_string(),
        ));
    }

    let now = now_secs();
    let ttl = req
        .ttl_secs
        .unwrap_or(state.max_ttl_secs)
        .clamp(60, state.max_ttl_secs);
    let expires_at = now + ttl;

    let id = generate_blob_id();

    // Generate 32-byte random delete token
    let mut raw_token = [0u8; 32];
    copy_randombytes(&mut raw_token);
    let delete_token = URL_SAFE_NO_PAD.encode(raw_token);
    let delete_token_hash = hash_token(&delete_token);

    let record = BlobRecord {
        id: id.clone(),
        delete_token_hash,
        size: req.size,
        parts: req.parts,
        burn: req.burn,
        created_at: now,
        expires_at,
        state: BlobState::Uploading,
        claim_token_hash: None,
        claim_expires_at: None,
    };

    state.storage.create_blob(&record).await?;

    let res_body = CreateBlobResponse {
        id,
        delete_token,
        expires_at,
        max_part_bytes: state.max_part_bytes,
    };

    let mut response = (StatusCode::CREATED, Json(res_body)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// PUT /v1/blobs/{id}/parts/{n}
async fn upload_part(
    State(state): State<AppState>,
    AxumPath((id, part_index)): AxumPath<(String, u32)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ServerError> {
    validate_blob_id(&id)?;
    let record = lookup_blob(&state, &id).await?;

    let token = extract_bearer_token(&headers)?;
    let token_hash = hash_token(&token);
    if !bool::from(token_hash.ct_eq(&record.delete_token_hash)) {
        return Err(ServerError::Unauthorized);
    }

    if record.state != BlobState::Uploading {
        return Err(ServerError::BadRequest(
            "Blob upload is already completed".to_string(),
        ));
    }

    if part_index >= record.parts {
        return Err(ServerError::BadRequest(format!(
            "Part index {part_index} out of range (total parts: {})",
            record.parts
        )));
    }

    if body.len() as u64 > state.max_part_bytes {
        return Err(ServerError::PayloadTooLarge(format!(
            "Part size {} exceeds cap {}",
            body.len(),
            state.max_part_bytes
        )));
    }

    if part_index == 0 && body.len() != PART0_SIZE {
        return Err(ServerError::BadRequest(format!(
            "Part 0 must be exactly {PART0_SIZE} bytes"
        )));
    }

    state.storage.write_part(&id, part_index, &body).await?;

    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// POST /v1/blobs/{id}/complete
async fn complete_blob(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Response, ServerError> {
    validate_blob_id(&id)?;
    let mut record = lookup_blob(&state, &id).await?;

    let token = extract_bearer_token(&headers)?;
    let token_hash = hash_token(&token);
    if !bool::from(token_hash.ct_eq(&record.delete_token_hash)) {
        return Err(ServerError::Unauthorized);
    }

    if record.state == BlobState::Available {
        return Ok(StatusCode::OK.into_response());
    }

    // Verify all parts exist and total size matches declared size
    let mut total_size = 0u64;
    for i in 0..record.parts {
        let size = state
            .storage
            .get_part_size(&id, i)
            .await?
            .ok_or_else(|| ServerError::BadRequest(format!("Missing part {i}")))?;
        total_size += size;
    }

    if total_size != record.size {
        return Err(ServerError::BadRequest(format!(
            "Total parts size {} does not match declared size {}",
            total_size, record.size
        )));
    }

    record.state = BlobState::Available;
    state.storage.update_blob(&record).await?;

    let mut response = StatusCode::OK.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// GET /v1/blobs/{id}
async fn get_metadata(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Response, ServerError> {
    validate_blob_id(&id)?;
    let record = lookup_blob(&state, &id).await?;

    let res = BlobMetadataResponse {
        size: record.size,
        parts: record.parts,
        burn: record.burn,
        expires_at: record.expires_at,
        state: record.state,
    };

    let mut response = Json(res).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// POST /v1/blobs/{id}/claim
async fn claim_blob(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Response, ServerError> {
    validate_blob_id(&id)?;
    // Serialise claims: lookup, check and update must be one atomic step, otherwise two
    // concurrent readers could both be handed a claim token for a one-view blob.
    let _guard = state.claim_lock.lock().await;
    let mut record = lookup_blob(&state, &id).await?;

    if record.state != BlobState::Available {
        return Err(ServerError::BadRequest(
            "Blob upload not completed".to_string(),
        ));
    }

    if !record.burn {
        let res = ClaimResponse {
            claim: None,
            claim_expires_at: None,
        };
        let mut response = Json(res).into_response();
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return Ok(response);
    }

    let now = now_secs();
    if let Some(exp) = record.claim_expires_at {
        if now < exp {
            return Err(ServerError::Conflict(
                "Blob already claimed by an active reader".to_string(),
            ));
        } else {
            // Claim expired without finish -> delete blob
            state
                .storage
                .record_tombstone(&id, record.expires_at)
                .await?;
            state.storage.delete_blob(&id).await?;
            return Err(ServerError::Gone);
        }
    }

    // Generate 16-byte random claim token
    let mut raw_claim = [0u8; 16];
    copy_randombytes(&mut raw_claim);
    let claim_str = URL_SAFE_NO_PAD.encode(raw_claim);
    let claim_hash = hash_token(&claim_str);
    let claim_expires_at = now + state.claim_window_secs;

    record.claim_token_hash = Some(claim_hash);
    record.claim_expires_at = Some(claim_expires_at);
    state.storage.update_blob(&record).await?;

    let res = ClaimResponse {
        claim: Some(claim_str),
        claim_expires_at: Some(claim_expires_at),
    };

    let mut response = Json(res).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// GET /v1/blobs/{id}/parts/{n}
async fn download_part(
    State(state): State<AppState>,
    AxumPath((id, part_index)): AxumPath<(String, u32)>,
    headers: HeaderMap,
) -> Result<Response, ServerError> {
    validate_blob_id(&id)?;
    let record = lookup_blob(&state, &id).await?;

    if record.state != BlobState::Available {
        return Err(ServerError::BadRequest(
            "Blob not available for download".to_string(),
        ));
    }

    if record.burn {
        let claim_str = extract_claim_token(&headers)?;
        let claim_hash = hash_token(&claim_str);
        match record.claim_token_hash {
            Some(stored_hash) if bool::from(claim_hash.ct_eq(&stored_hash)) => (),
            _ => return Err(ServerError::Unauthorized),
        }
    }

    let part_bytes = state
        .storage
        .read_part(&id, part_index)
        .await?
        .ok_or(ServerError::NotFound)?;

    // Handle optional Range header (e.g. bytes=0-1023)
    let total_len = part_bytes.len();
    let mut status = StatusCode::OK;
    let mut body_slice = &part_bytes[..];
    let mut content_range = None;

    if let Some(range_val) = headers.get(RANGE).and_then(|v| v.to_str().ok()) {
        if let Some(range_spec) = range_val.strip_prefix("bytes=") {
            let parts: Vec<&str> = range_spec.split('-').collect();
            if parts.len() == 2 {
                let start: usize = parts[0].parse().unwrap_or(0);
                let end: usize = if parts[1].is_empty() {
                    total_len.saturating_sub(1)
                } else {
                    parts[1].parse().unwrap_or(total_len.saturating_sub(1))
                };

                if start <= end && start < total_len {
                    let end_bounded = end.min(total_len - 1);
                    body_slice = &part_bytes[start..=end_bounded];
                    status = StatusCode::PARTIAL_CONTENT;
                    content_range = Some(format!("bytes {start}-{end_bounded}/{total_len}"));
                }
            }
        }
    }

    let mut response = (status, Bytes::copy_from_slice(body_slice)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Some(cr) = content_range {
        if let Ok(val) = HeaderValue::from_str(&cr) {
            response.headers_mut().insert("Content-Range", val);
        }
    }

    Ok(response)
}

/// POST /v1/blobs/{id}/finish
async fn finish_blob(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Response, ServerError> {
    validate_blob_id(&id)?;
    let _guard = state.claim_lock.lock().await;
    let record = lookup_blob(&state, &id).await?;

    if record.burn {
        let claim_str = extract_claim_token(&headers)?;
        let claim_hash = hash_token(&claim_str);
        match record.claim_token_hash {
            Some(stored_hash) if bool::from(claim_hash.ct_eq(&stored_hash)) => (),
            _ => return Err(ServerError::Unauthorized),
        }

        // Delete burned blob immediately upon verified finish, and record tombstone!
        state
            .storage
            .record_tombstone(&id, record.expires_at)
            .await?;
        state.storage.delete_blob(&id).await?;
    }

    let mut response = StatusCode::OK.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// DELETE /v1/blobs/{id}
async fn delete_blob(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Response, ServerError> {
    validate_blob_id(&id)?;
    let record = lookup_blob(&state, &id).await?;

    let token = extract_bearer_token(&headers)?;
    let token_hash = hash_token(&token);
    if !bool::from(token_hash.ct_eq(&record.delete_token_hash)) {
        return Err(ServerError::Unauthorized);
    }

    state
        .storage
        .record_tombstone(&id, record.expires_at)
        .await?;
    state.storage.delete_blob(&id).await?;

    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

#[derive(Debug, Deserialize)]
pub struct GetMessagesQuery {
    pub poll_timeout_secs: Option<u64>,
    pub after_id: Option<u64>,
    pub drain: Option<bool>,
}

async fn lookup_mailbox(state: &AppState, id: &str) -> Result<MailboxRecord, ServerError> {
    validate_blob_id(id)?;
    let record = match state.storage.get_mailbox(id).await? {
        Some(r) => r,
        None => return Err(ServerError::NotFound),
    };

    let now = now_secs();
    if now >= record.expires_at {
        let _ = state.storage.delete_mailbox(id).await;
        return Err(ServerError::NotFound);
    }

    Ok(record)
}

/// POST /v1/mailboxes
async fn create_mailbox(
    State(state): State<AppState>,
    peer: Peer,
    headers: HeaderMap,
    Json(req): Json<CreateMailboxRequest>,
) -> Result<Response, ServerError> {
    // Mailbox creation had no throttle at all: anyone could create unlimited mailboxes and
    // fill the disk. Apply the same per-address limit as blob creation.
    let ip = client_ip(&state, &peer, &headers);
    state.rate_limiter.check_and_record(ip).await?;

    validate_blob_id(&req.id)?;

    let push_hash_bytes = URL_SAFE_NO_PAD
        .decode(&req.push_token_hash)
        .map_err(|_| ServerError::BadRequest("Invalid push_token_hash base64url".into()))?;
    if push_hash_bytes.len() != 32 {
        return Err(ServerError::BadRequest(
            "push_token_hash must be 32 bytes".into(),
        ));
    }
    let mut push_token_hash = [0u8; 32];
    push_token_hash.copy_from_slice(&push_hash_bytes);

    let pull_hash_bytes = URL_SAFE_NO_PAD
        .decode(&req.pull_token_hash)
        .map_err(|_| ServerError::BadRequest("Invalid pull_token_hash base64url".into()))?;
    if pull_hash_bytes.len() != 32 {
        return Err(ServerError::BadRequest(
            "pull_token_hash must be 32 bytes".into(),
        ));
    }
    let mut pull_token_hash = [0u8; 32];
    pull_token_hash.copy_from_slice(&pull_hash_bytes);

    let now = now_secs();
    let ttl = req
        .ttl_secs
        .unwrap_or(MAX_LIFETIME_SECS)
        .min(MAX_LIFETIME_SECS);
    let expires_at = now + ttl;

    // Check if already exists (idempotency)
    if let Some(existing) = state.storage.get_mailbox(&req.id).await? {
        if bool::from(existing.push_token_hash.ct_eq(&push_token_hash))
            && bool::from(existing.pull_token_hash.ct_eq(&pull_token_hash))
        {
            let mut response = StatusCode::OK.into_response();
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            return Ok(response);
        } else {
            return Err(ServerError::Conflict(
                "Mailbox already exists with different tokens".into(),
            ));
        }
    }

    let record = MailboxRecord {
        id: req.id,
        push_token_hash,
        pull_token_hash,
        created_at: now,
        expires_at,
    };

    state.storage.create_mailbox(&record).await?;

    let mut response = StatusCode::CREATED.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// POST /v1/mailboxes/{id}/messages
async fn push_message(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
    Json(req): Json<PushMessageRequest>,
) -> Result<Response, ServerError> {
    let record = lookup_mailbox(&state, &id).await?;

    let token = extract_bearer_token(&headers)?;
    let token_hash = hash_token(&token);
    if !bool::from(token_hash.ct_eq(&record.push_token_hash)) {
        return Err(ServerError::Unauthorized);
    }

    let payload = URL_SAFE_NO_PAD
        .decode(&req.payload)
        .map_err(|_| ServerError::BadRequest("Invalid message payload base64url".into()))?;

    if payload.len() > MAX_THREAD_MSG_BYTES {
        return Err(ServerError::PayloadTooLarge(
            "Message payload exceeds 64 KiB".into(),
        ));
    }

    let now = now_secs();
    let msg_id = state
        .storage
        .push_mailbox_message(&id, payload, now)
        .await?;

    let _ = state.mailbox_notifier.send(id);

    let mut response = (
        StatusCode::CREATED,
        Json(PushMessageResponse { id: msg_id }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// GET /v1/mailboxes/{id}/messages
async fn get_messages(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<GetMessagesQuery>,
    headers: HeaderMap,
) -> Result<Response, ServerError> {
    let record = lookup_mailbox(&state, &id).await?;

    let token = extract_bearer_token(&headers)?;
    let token_hash = hash_token(&token);
    if !bool::from(token_hash.ct_eq(&record.pull_token_hash)) {
        return Err(ServerError::Unauthorized);
    }

    let drain = query.drain.unwrap_or(true);
    let mut messages = state
        .storage
        .get_mailbox_messages(&id, query.after_id, drain)
        .await?;

    if messages.is_empty() {
        if let Some(poll_secs) = query.poll_timeout_secs {
            let timeout_duration = std::time::Duration::from_secs(poll_secs.min(30));
            if timeout_duration > std::time::Duration::ZERO {
                let mut rx = state.mailbox_notifier.subscribe();
                let deadline = tokio::time::Instant::now() + timeout_duration;

                while tokio::time::Instant::now() < deadline {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => break,
                        recv_res = rx.recv() => {
                            if let Ok(notif_id) = recv_res {
                                if notif_id == id {
                                    messages = state.storage.get_mailbox_messages(&id, query.after_id, drain).await?;
                                    if !messages.is_empty() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let items: Vec<MailboxMessageItem> = messages
        .into_iter()
        .map(|m| MailboxMessageItem {
            id: m.id,
            payload: URL_SAFE_NO_PAD.encode(m.payload),
            created_at: m.created_at,
        })
        .collect();

    let mut response = (
        StatusCode::OK,
        Json(GetMessagesResponse { messages: items }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// DELETE /v1/mailboxes/{id}
async fn delete_mailbox(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Response, ServerError> {
    let record = lookup_mailbox(&state, &id).await?;

    let token = extract_bearer_token(&headers)?;
    let token_hash = hash_token(&token);
    let is_push = bool::from(token_hash.ct_eq(&record.push_token_hash));
    let is_pull = bool::from(token_hash.ct_eq(&record.pull_token_hash));

    if !is_push && !is_pull {
        return Err(ServerError::Unauthorized);
    }

    state.storage.delete_mailbox(&id).await?;

    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn authenticate_admin(headers: &HeaderMap, state: &AppState) -> Result<(), ServerError> {
    let expected = match &state.admin_token {
        Some(t) if !t.is_empty() => t.as_bytes(),
        _ => return Err(ServerError::Unauthorized),
    };
    let auth_header = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(ServerError::Unauthorized)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(ServerError::Unauthorized)?;

    let token_bytes = token.as_bytes();
    if token_bytes.len() != expected.len() {
        return Err(ServerError::Unauthorized);
    }

    if token_bytes.ct_eq(expected).into() {
        Ok(())
    } else {
        Err(ServerError::Unauthorized)
    }
}

/// POST /v1/reports
async fn submit_report(
    State(state): State<AppState>,
    peer: Peer,
    headers: HeaderMap,
    Json(req): Json<SubmitReportRequest>,
) -> Result<Response, ServerError> {
    let client_ip = client_ip(&state, &peer, &headers);
    state.rate_limiter.check_and_record(client_ip).await?;

    // Bound every free-text field: reports are stored for 30 days and the endpoint has no
    // proof-of-work, so unbounded fields would let anyone fill the disk.
    const MAX_FIELD_BYTES: usize = 2_000;
    const MAX_CONTENT_BYTES: usize = 256 * 1024;
    let too_long = req.target_id.len() > 64
        || req.reason.len() > MAX_FIELD_BYTES
        || req
            .notes
            .as_ref()
            .is_some_and(|n| n.len() > MAX_FIELD_BYTES)
        || req.link_key.as_ref().is_some_and(|k| k.len() > 512)
        || req
            .content
            .as_ref()
            .is_some_and(|c| c.len() > MAX_CONTENT_BYTES);
    if too_long {
        return Err(ServerError::PayloadTooLarge(
            "A report field exceeds its maximum length".into(),
        ));
    }

    if req.target_id.trim().is_empty() {
        return Err(ServerError::BadRequest("target_id cannot be empty".into()));
    }
    if req.reason.trim().is_empty() {
        return Err(ServerError::BadRequest("reason cannot be empty".into()));
    }

    let report_id = generate_blob_id();
    let created_at = now_secs();
    // 30 days retention per privacy-and-logging.md
    let expires_at = created_at + 30 * 86400;

    let (content, link_key, encrypted) = if let Some(ref pk) = state.reviewer_public_key {
        let enc_content = if let Some(ref c) = req.content {
            Some(
                ember_core::seal_report_field(c.as_bytes(), pk).map_err(|e| {
                    ServerError::Storage(format!("Failed to seal report content: {e}"))
                })?,
            )
        } else {
            None
        };
        let enc_link_key = if let Some(ref k) = req.link_key {
            Some(
                ember_core::seal_report_field(k.as_bytes(), pk).map_err(|e| {
                    ServerError::Storage(format!("Failed to seal report link key: {e}"))
                })?,
            )
        } else {
            None
        };
        (enc_content, enc_link_key, true)
    } else {
        (req.content, req.link_key, false)
    };

    let record = ReportRecord {
        id: report_id.clone(),
        created_at,
        expires_at,
        target_id: req.target_id,
        target_type: req.target_type,
        reason: req.reason,
        notes: req.notes,
        content,
        link_key,
        status: "pending".to_string(),
        encrypted,
    };

    state.storage.save_report(record).await?;

    let mut response = (
        StatusCode::CREATED,
        Json(SubmitReportResponse {
            id: report_id,
            status: "submitted".to_string(),
        }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// GET /v1/transparency
async fn get_transparency_stats(State(state): State<AppState>) -> Result<Response, ServerError> {
    let stats = state.storage.get_transparency_stats().await?;
    let mut response = (StatusCode::OK, Json(stats)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// POST /v1/admin/takedown
async fn admin_takedown(
    State(state): State<AppState>,
    peer: Peer,
    headers: HeaderMap,
    Json(req): Json<AdminTakedownRequest>,
) -> Result<Response, ServerError> {
    // Throttle guessing of the admin token (every attempt counts against the caller's address).
    let ip = client_ip(&state, &peer, &headers);
    state.rate_limiter.check_and_record(ip).await?;
    authenticate_admin(&headers, &state)?;

    // Identifiers become file-system paths: accept only well-formed IDs.
    validate_blob_id(&req.target_id)
        .map_err(|_| ServerError::BadRequest("target_id is not a valid identifier".into()))?;

    if req.target_id.trim().is_empty() {
        return Err(ServerError::BadRequest("target_id cannot be empty".into()));
    }
    if req.operator.trim().is_empty() {
        return Err(ServerError::BadRequest("operator cannot be empty".into()));
    }
    if req.reason.trim().is_empty() {
        return Err(ServerError::BadRequest("reason cannot be empty".into()));
    }

    match req.target_type.as_str() {
        "blob" => {
            let _ = state.storage.delete_blob(&req.target_id).await;
            let tombstone_exp = now_secs() + state.max_ttl_secs;
            let _ = state
                .storage
                .record_tombstone(&req.target_id, tombstone_exp)
                .await;
        }
        "mailbox" => {
            let _ = state.storage.delete_mailbox(&req.target_id).await;
        }
        _ => {
            return Err(ServerError::BadRequest(
                "Invalid target_type; must be 'blob' or 'mailbox'".into(),
            ));
        }
    }

    let takedown_id = generate_blob_id();
    let record = TakedownRecord {
        id: takedown_id,
        timestamp: now_secs(),
        target_id: req.target_id,
        target_type: req.target_type,
        request_ref: req.request_ref,
        operator: req.operator,
        reason: req.reason,
        action_taken: "deleted".to_string(),
    };

    state.storage.record_takedown(record.clone()).await?;

    let mut response = (StatusCode::OK, Json(record)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// GET /v1/admin/reports
async fn admin_list_reports(
    State(state): State<AppState>,
    peer: Peer,
    headers: HeaderMap,
) -> Result<Response, ServerError> {
    let ip = client_ip(&state, &peer, &headers);
    state.rate_limiter.check_and_record(ip).await?;
    authenticate_admin(&headers, &state)?;

    let reports = state.storage.list_reports().await?;
    let mut response = (StatusCode::OK, Json(reports)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}
