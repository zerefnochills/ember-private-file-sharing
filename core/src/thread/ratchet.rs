//! Forward-secret transport ratcheting backed by Noise protocol stateless transport mode.
//!
//! Provides per-message forward secrecy and authentication:
//! - Each message is encrypted with ChaCha20-Poly1305 using the message sequence counter.
//! - Nonces are derived deterministically from the 64-bit sequence number per Noise specification.
//! - Monotonically enforced sequence numbers prevent replay attacks.
//! - Envelope versioning: payloads include format version byte `v = 2`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::EmberError;
use ember_protocol::{MAX_THREAD_MSG_BYTES, THREAD_MSG_VERSION};

/// Encrypted message payload transmitted over the thread mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedMessagePayload {
    /// Format version byte (version 2 for Noise transport).
    #[serde(default)]
    pub v: u8,
    /// Monotonically increasing sequence number (0, 1, 2, ...).
    pub seq: u64,
    /// Ciphertext with Poly1305 authentication tag appended.
    pub ciphertext: Vec<u8>,
}

/// Sending ratchet maintaining message encryption for outgoing thread messages.
pub struct SendingRatchet {
    transport: Arc<snow::StatelessTransportState>,
    seq: u64,
}

impl SendingRatchet {
    /// Initializes a sending ratchet from an active Noise stateless transport state.
    pub fn new(transport: Arc<snow::StatelessTransportState>) -> Self {
        Self { transport, seq: 0 }
    }

    /// Current sequence number for outgoing messages.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Encrypts a message payload using the next sequence number.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<EncryptedMessagePayload, EmberError> {
        if plaintext.len() > MAX_THREAD_MSG_BYTES {
            return Err(EmberError::InvalidMetadata(format!(
                "Message payload {} exceeds maximum {} bytes",
                plaintext.len(),
                MAX_THREAD_MSG_BYTES
            )));
        }

        // Noise ChaCha20-Poly1305 adds 16 bytes overhead
        let mut ciphertext = vec![0u8; plaintext.len() + 16];
        let len = self
            .transport
            .write_message(self.seq, plaintext, &mut ciphertext)
            .map_err(|e| EmberError::Crypto(format!("Thread message encryption failed: {e:?}")))?;
        ciphertext.truncate(len);

        let payload = EncryptedMessagePayload {
            v: THREAD_MSG_VERSION,
            seq: self.seq,
            ciphertext,
        };

        self.seq += 1;
        Ok(payload)
    }
}

/// Receiving ratchet maintaining message decryption and replay protection for incoming thread messages.
pub struct ReceivingRatchet {
    transport: Arc<snow::StatelessTransportState>,
    expected_seq: u64,
}

impl ReceivingRatchet {
    /// Initializes a receiving ratchet from an active Noise stateless transport state.
    pub fn new(transport: Arc<snow::StatelessTransportState>) -> Self {
        Self {
            transport,
            expected_seq: 0,
        }
    }

    /// Next expected message sequence number.
    pub fn expected_seq(&self) -> u64 {
        self.expected_seq
    }

    /// Decrypts a received message, advances the expected sequence counter, and rejects replays.
    pub fn decrypt(&mut self, payload: &EncryptedMessagePayload) -> Result<Vec<u8>, EmberError> {
        // Enforce envelope version
        if payload.v != THREAD_MSG_VERSION {
            return Err(EmberError::UnsupportedVersion(payload.v));
        }

        // Replay check
        if payload.seq < self.expected_seq {
            return Err(EmberError::ReplayDetected);
        }

        // Sequential order check
        if payload.seq != self.expected_seq {
            return Err(EmberError::OutOfOrderMessage(payload.seq));
        }

        if payload.ciphertext.len() < 16 {
            return Err(EmberError::Auth);
        }

        let mut plaintext = vec![0u8; payload.ciphertext.len()];
        let len = self
            .transport
            .read_message(payload.seq, &payload.ciphertext, &mut plaintext)
            .map_err(|_| EmberError::Auth)?;

        plaintext.truncate(len);
        self.expected_seq += 1;
        Ok(plaintext)
    }
}
