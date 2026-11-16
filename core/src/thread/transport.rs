//! Session-level forward-secret transport backed by Noise protocol stateless transport mode.
//!
//! Provides session-level forward secrecy and authentication:
//! - Each message is encrypted with ChaCha20-Poly1305 using the message sequence counter.
//! - Nonces are derived deterministically from the 64-bit sequence number per Noise specification.
//! - Monotonically enforced sequence numbers prevent replay attacks.
//! - Envelope versioning: payloads include format version byte `v = 3` in a binary prefix `[v: u8][seq: u64 be]`.

use std::sync::Arc;

use crate::error::EmberError;
use ember_protocol::{
    MAX_THREAD_CIPHERTEXT_BYTES, MAX_THREAD_MSG_BYTES, THREAD_MSG_HEADER_LEN, THREAD_MSG_VERSION,
};

/// Encrypted message payload transmitted over the thread mailbox.
///
/// Binary wire encoding: `v(1) | seq(8, big-endian) | ciphertext`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedMessagePayload {
    /// Format version byte (version 3 for binary envelope).
    pub v: u8,
    /// Monotonically increasing sequence number (0, 1, 2, ...).
    pub seq: u64,
    /// Ciphertext with Poly1305 authentication tag appended.
    pub ciphertext: Vec<u8>,
}

impl EncryptedMessagePayload {
    /// Serializes the envelope into binary wire format `v(1) | seq(8, big-endian) | ciphertext`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(THREAD_MSG_HEADER_LEN + self.ciphertext.len());
        out.push(self.v);
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.extend_from_slice(&self.ciphertext);
        out
    }

    /// Deserializes a binary wire payload, validating version, header length, and ciphertext bounds before allocating.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EmberError> {
        if bytes.len() < THREAD_MSG_HEADER_LEN {
            return Err(EmberError::InvalidMetadata(
                "Thread message envelope truncated: missing header".into(),
            ));
        }

        let v = bytes[0];
        if v != THREAD_MSG_VERSION {
            return Err(EmberError::UnsupportedVersion(v));
        }

        let seq =
            u64::from_be_bytes(bytes[1..9].try_into().map_err(|_| {
                EmberError::InvalidMetadata("Invalid sequence number encoding".into())
            })?);

        let ct = &bytes[9..];
        if ct.len() > MAX_THREAD_CIPHERTEXT_BYTES {
            return Err(EmberError::InvalidMetadata(format!(
                "Ciphertext length {} exceeds maximum allowed {} bytes",
                ct.len(),
                MAX_THREAD_CIPHERTEXT_BYTES
            )));
        }

        if ct.len() < 16 {
            return Err(EmberError::Auth);
        }

        Ok(Self {
            v,
            seq,
            ciphertext: ct.to_vec(),
        })
    }
}

/// Sending transport maintaining message encryption for outgoing thread messages.
pub struct ThreadSender {
    transport: Arc<snow::StatelessTransportState>,
    seq: u64,
}

impl ThreadSender {
    /// Initializes a thread sender from an active Noise stateless transport state.
    pub fn new(transport: Arc<snow::StatelessTransportState>) -> Self {
        Self { transport, seq: 0 }
    }

    /// Current sequence number for outgoing messages.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Sets sequence number (for test-hooks testing only).
    #[cfg(feature = "test-hooks")]
    pub fn set_seq_for_testing(&mut self, seq: u64) {
        self.seq = seq;
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

/// Receiving transport maintaining message decryption and replay protection for incoming thread messages.
pub struct ThreadReceiver {
    transport: Arc<snow::StatelessTransportState>,
    expected_seq: u64,
}

impl ThreadReceiver {
    /// Initializes a thread receiver from an active Noise stateless transport state.
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
    ///
    /// Sequence numbers `seq >= expected_seq` are accepted and advance state to `seq + 1`.
    /// Sequence numbers `seq < expected_seq` are strictly rejected as replays.
    /// Tampered messages with invalid authentication tags do not advance the sequence counter.
    pub fn decrypt(&mut self, payload: &EncryptedMessagePayload) -> Result<Vec<u8>, EmberError> {
        // Enforce envelope version
        if payload.v != THREAD_MSG_VERSION {
            return Err(EmberError::UnsupportedVersion(payload.v));
        }

        // Replay check
        if payload.seq < self.expected_seq {
            return Err(EmberError::ReplayDetected);
        }

        if payload.ciphertext.len() < 16 {
            return Err(EmberError::Auth);
        }

        if payload.ciphertext.len() > MAX_THREAD_CIPHERTEXT_BYTES {
            return Err(EmberError::InvalidMetadata(format!(
                "Ciphertext length {} exceeds maximum {} bytes",
                payload.ciphertext.len(),
                MAX_THREAD_CIPHERTEXT_BYTES
            )));
        }

        let mut plaintext = vec![0u8; payload.ciphertext.len()];
        let len = self
            .transport
            .read_message(payload.seq, &payload.ciphertext, &mut plaintext)
            .map_err(|_| EmberError::Auth)?;

        plaintext.truncate(len);
        // Advance expected sequence to seq + 1 only on successful decryption
        self.expected_seq = payload.seq + 1;
        Ok(plaintext)
    }

    /// Decrypts a raw binary wire payload (`v | seq | ciphertext`).
    pub fn decrypt_bytes(&mut self, bytes: &[u8]) -> Result<Vec<u8>, EmberError> {
        let payload = EncryptedMessagePayload::from_bytes(bytes)?;
        self.decrypt(&payload)
    }
}
