//! Thread session management and message envelope processing.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use super::handle::is_valid_handle;
use super::handshake::{HandshakeSessionKeys, MailboxCapability};
use super::transport::{EncryptedMessagePayload, ThreadReceiver, ThreadSender};
use crate::error::EmberError;
use ember_protocol::{MAX_THREAD_MSG_BYTES, MAX_THREAD_TEXT_BYTES};

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
    sender: ThreadSender,
    receiver: ThreadReceiver,
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
            sender: ThreadSender::new(keys.transport.clone()),
            receiver: ThreadReceiver::new(keys.transport.clone()),
        }
    }

    /// Next sequence number for outgoing messages.
    pub fn next_send_seq(&self) -> u64 {
        self.sender.seq()
    }

    /// Sets sequence number (for test-hooks testing only).
    #[cfg(feature = "test-hooks")]
    pub fn set_send_seq_for_testing(&mut self, seq: u64) {
        self.sender.set_seq_for_testing(seq);
    }

    /// Next expected sequence number for incoming messages.
    pub fn expected_recv_seq(&self) -> u64 {
        self.receiver.expected_seq()
    }

    /// Encrypts a message using the session-level forward-secret transport.
    pub fn encrypt_text(&mut self, text: &str) -> Result<EncryptedMessagePayload, EmberError> {
        self.encrypt_text_with_timestamp_internal(text, now_secs())
    }

    /// Encrypts a message with an explicit timestamp (for testing and reproducible test vectors).
    #[cfg(feature = "test-hooks")]
    pub fn encrypt_text_with_timestamp(
        &mut self,
        text: &str,
        timestamp: u64,
    ) -> Result<EncryptedMessagePayload, EmberError> {
        self.encrypt_text_with_timestamp_internal(text, timestamp)
    }

    fn encrypt_text_with_timestamp_internal(
        &mut self,
        text: &str,
        timestamp: u64,
    ) -> Result<EncryptedMessagePayload, EmberError> {
        if text.len() > MAX_THREAD_TEXT_BYTES {
            return Err(EmberError::InvalidMetadata(format!(
                "Message text length {} exceeds maximum allowed {} bytes",
                text.len(),
                MAX_THREAD_TEXT_BYTES
            )));
        }

        let current_seq = self.sender.seq();
        if current_seq == u64::MAX {
            return Err(EmberError::SeqOverflow);
        }

        let msg = ThreadMessage {
            seq: current_seq,
            timestamp,
            sender_handle: self.handle.clone(),
            text: text.to_string(),
        };

        let json_bytes = serde_json::to_vec(&msg)
            .map_err(|e| EmberError::InvalidMetadata(format!("JSON serialization failed: {e}")))?;

        if json_bytes.len() > MAX_THREAD_MSG_BYTES {
            return Err(EmberError::InvalidMetadata(format!(
                "Serialized message length {} exceeds maximum allowed {} bytes",
                json_bytes.len(),
                MAX_THREAD_MSG_BYTES
            )));
        }

        self.sender.encrypt(&json_bytes)
    }

    /// Decrypts a received message payload using the session-level forward-secret transport.
    pub fn decrypt_message(
        &mut self,
        payload: &EncryptedMessagePayload,
    ) -> Result<ThreadMessage, EmberError> {
        let plaintext_bytes = self.receiver.decrypt(payload)?;

        let msg: ThreadMessage = serde_json::from_slice(&plaintext_bytes)
            .map_err(|e| EmberError::InvalidMetadata(format!("Invalid message JSON: {e}")))?;

        if !is_valid_handle(&msg.sender_handle) {
            return Err(EmberError::InvalidSenderHandle(msg.sender_handle));
        }

        if msg.seq != payload.seq {
            return Err(EmberError::SeqMismatch(payload.seq, msg.seq));
        }

        Ok(msg)
    }

    /// Decrypts a received raw binary wire payload (`v | seq | ciphertext`).
    pub fn decrypt_message_bytes(&mut self, bytes: &[u8]) -> Result<ThreadMessage, EmberError> {
        let payload = EncryptedMessagePayload::from_bytes(bytes)?;
        self.decrypt_message(&payload)
    }
}
