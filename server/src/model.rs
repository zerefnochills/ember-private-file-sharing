//! Wire models and internal records for Ember relay server.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Current lifecycle state of a blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlobState {
    Uploading,
    Available,
}

/// Request body for POST /v1/blobs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBlobRequest {
    /// Total ciphertext size in bytes across all parts.
    pub size: u64,
    /// Number of transport parts.
    pub parts: u32,
    /// Whether this blob should be deleted immediately after completed view.
    #[serde(default)]
    pub burn: bool,
    /// Requested time-to-live in seconds (max 43,200).
    pub ttl_secs: Option<u64>,
    /// Optional PoW solution in body (can also be supplied in HTTP headers).
    #[serde(default)]
    pub pow: Option<PowSolutionPayload>,
}

/// Optional PoW solution payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowSolutionPayload {
    pub challenge: String,
    pub nonce: u64,
}

/// Response body for GET /v1/pow/challenge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowChallengeResponse {
    pub challenge: String,
    pub difficulty: u32,
    pub expires_at: u64,
}

/// Response body for POST /v1/blobs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBlobResponse {
    /// Server-generated 22-character base64url blob identifier.
    pub id: String,
    /// 32-byte random bearer token to authorize uploads, completion, and deletion.
    pub delete_token: String,
    /// Unix timestamp when the blob will expire if not already burned.
    pub expires_at: u64,
    /// Maximum allowed bytes per transport part.
    pub max_part_bytes: u64,
}

/// Response body for GET /v1/blobs/{id}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobMetadataResponse {
    pub size: u64,
    pub parts: u32,
    pub burn: bool,
    pub expires_at: u64,
    pub state: BlobState,
}

/// Response body for POST /v1/blobs/{id}/claim
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimResponse {
    /// Claim token required for downloading parts of a burn-after-view blob.
    pub claim: Option<String>,
    /// Unix timestamp when the 10-minute claim window expires.
    pub claim_expires_at: Option<u64>,
}

/// Internal metadata record stored by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobRecord {
    pub id: String,
    /// SHA-256 hash of the delete token; plaintext token is never stored.
    pub delete_token_hash: [u8; 32],
    pub size: u64,
    pub parts: u32,
    pub burn: bool,
    pub created_at: u64,
    pub expires_at: u64,
    pub state: BlobState,
    /// SHA-256 hash of the active claim token if claimed.
    pub claim_token_hash: Option<[u8; 32]>,
    pub claim_expires_at: Option<u64>,
}

/// Request body for POST /v1/mailboxes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMailboxRequest {
    /// 22-character base64url mailbox ID
    pub id: String,
    /// Base64url-encoded SHA-256 hash of push token
    pub push_token_hash: String,
    /// Base64url-encoded SHA-256 hash of pull token
    pub pull_token_hash: String,
    /// Requested TTL in seconds (max 43,200)
    pub ttl_secs: Option<u64>,
}

/// Request body for POST /v1/mailboxes/{id}/messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushMessageRequest {
    /// Base64url-encoded ciphertext bytes of the message payload
    pub payload: String,
}

/// Response body for POST /v1/mailboxes/{id}/messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushMessageResponse {
    pub id: u64,
}

/// Stored message record in a mailbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxMessageRecord {
    pub id: u64,
    pub payload: Vec<u8>,
    pub created_at: u64,
}

/// Item in response for GET /v1/mailboxes/{id}/messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxMessageItem {
    pub id: u64,
    pub payload: String,
    pub created_at: u64,
}

/// Response body for GET /v1/mailboxes/{id}/messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMessagesResponse {
    pub messages: Vec<MailboxMessageItem>,
}

/// Internal mailbox metadata record stored by the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxRecord {
    pub id: String,
    /// SHA-256 hash of push bearer token
    pub push_token_hash: [u8; 32],
    /// SHA-256 hash of pull bearer token
    pub pull_token_hash: [u8; 32],
    pub created_at: u64,
    pub expires_at: u64,
}
