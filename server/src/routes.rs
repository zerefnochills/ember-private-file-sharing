//! HTTP route handlers for Ember relay server.

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::header::{HeaderMap, AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, RANGE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use dryoc::rng::copy_randombytes;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use ember_core::{generate_blob_id, validate_blob_id};
use ember_protocol::PART0_SIZE;

use crate::error::ServerError;
use crate::model::{
    BlobMetadataResponse, BlobRecord, BlobState, ClaimResponse, CreateBlobRequest,
    CreateBlobResponse,
};
use crate::state::{now_secs, AppState};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/blobs", post(create_blob))
        .route("/v1/blobs/{id}", get(get_metadata).delete(delete_blob))
        .route(
            "/v1/blobs/{id}/parts/{n}",
            put(upload_part).get(download_part),
        )
        .route("/v1/blobs/{id}/complete", post(complete_blob))
        .route("/v1/blobs/{id}/claim", post(claim_blob))
        .route("/v1/blobs/{id}/finish", post(finish_blob))
        .with_state(state)
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
    Json(req): Json<CreateBlobRequest>,
) -> Result<Response, ServerError> {
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
