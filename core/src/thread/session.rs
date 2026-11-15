//! Thread session management and message envelope processing.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use super::handshake::{HandshakeSessionKeys, MailboxCapability};
use super::ratchet::{EncryptedMessagePayload, ReceivingRatchet, SendingRatchet};
use crate::error::EmberError;

/// Decrypted message received or sent inside an active thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadMessage {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// Unix timestamp in seconds when the message was encrypted.
    pub timestamp: u64,
    /// Sender's local handle (e.g. `amber-fox-4821`).
    pub sender_handle: String,
    /// Message plaintext content.
    pub text: String,
}

/// Active bidirectional encrypted thread session.
pub struct ThreadSession {
    /// Local user handle for this thread.
    pub handle: String,
    /// Outgoing mailbox capability.
    pub mailbox_out: MailboxCapability,
    /// Incoming mailbox capability.
    pub mailbox_in: MailboxCapability,
    send_ratchet: SendingRatchet,
    recv_ratchet: ReceivingRatchet,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl ThreadSession {
    /// Creates a thread session from established handshake keys.
    pub fn new(handle: String, keys: HandshakeSessionKeys) -> Self {
        Self {
            handle,
            mailbox_out: keys.mailbox_out.clone(),
            mailbox_in: keys.mailbox_in.clone(),
            send_ratchet: SendingRatchet::new(keys.send_chain_key),
            recv_ratchet: ReceivingRatchet::new(keys.recv_chain_key),
        }
    }

    /// Next sequence number for outgoing messages.
    pub fn next_send_seq(&self) -> u64 {
        self.send_ratchet.seq()
    }

    /// Next expected sequence number for incoming messages.
    pub fn expected_recv_seq(&self) -> u64 {
        self.recv_ratchet.expected_seq()
    }

    /// Encrypts a message using the forward-secret sending ratchet.
    pub fn encrypt_text(&mut self, text: &str) -> Result<EncryptedMessagePayload, EmberError> {
        let msg = ThreadMessage {
            seq: self.send_ratchet.seq(),
            timestamp: now_secs(),
            sender_handle: self.handle.clone(),
            text: text.to_string(),
        };

        let json_bytes = serde_json::to_vec(&msg)
            .map_err(|e| EmberError::InvalidMetadata(format!("JSON serialization failed: {e}")))?;

        self.send_ratchet.encrypt(&json_bytes)
    }

    /// Decrypts a received message payload using the forward-secret receiving ratchet.
    pub fn decrypt_message(
        &mut self,
        payload: &EncryptedMessagePayload,
    ) -> Result<ThreadMessage, EmberError> {
        let plaintext_bytes = self.recv_ratchet.decrypt(payload)?;

        let msg: ThreadMessage = serde_json::from_slice(&plaintext_bytes)
            .map_err(|e| EmberError::InvalidMetadata(format!("Invalid message JSON: {e}")))?;

        Ok(msg)
    }
}
