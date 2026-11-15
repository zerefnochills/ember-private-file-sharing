//! Forward-secret symmetric ratchet for Ember thread messages.
//!
//! Provides per-message forward secrecy:
//! - Each message derives an ephemeral message key `MK` using BLAKE2b KDF.
//! - The chain key advances monotonically and the message key is immediately zeroized.
//! - Compromise of state at step N cannot decrypt past messages 0..N-1.
//! - Monotonically checked sequence numbers prevent replay attacks.

use dryoc::classic::crypto_aead_xchacha20poly1305_ietf::{
    crypto_aead_xchacha20poly1305_ietf_decrypt, crypto_aead_xchacha20poly1305_ietf_encrypt,
};
use dryoc::classic::crypto_kdf::crypto_kdf_derive_from_key;
use dryoc::rng::copy_randombytes;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::error::EmberError;
use ember_protocol::{
    AD_THREAD, KDF_CONTEXT, MAX_THREAD_MSG_BYTES, SUBKEY_ID_THREAD_CHAIN_INIT,
    SUBKEY_ID_THREAD_MSG,
};

/// Encrypted message payload transmitted over the thread mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedMessagePayload {
    /// Monotonically increasing sequence number (0, 1, 2, ...).
    pub seq: u64,
    /// 24-byte nonce (8 bytes big-endian seq || 16 random bytes).
    pub nonce: [u8; 24],
    /// Ciphertext with 16-byte Poly1305 authentication tag appended.
    pub ciphertext: Vec<u8>,
}

/// Sending ratchet maintaining forward secrecy for outgoing thread messages.
pub struct SendingRatchet {
    chain_key: [u8; 32],
    seq: u64,
}

impl Drop for SendingRatchet {
    fn drop(&mut self) {
        self.chain_key.zeroize();
    }
}

impl SendingRatchet {
    /// Initializes a sending ratchet from a 32-byte initial chain key.
    pub fn new(initial_chain_key: [u8; 32]) -> Self {
        Self {
            chain_key: initial_chain_key,
            seq: 0,
        }
    }

    /// Current sequence number.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Encrypts a message payload, advances the chain key, and zeroizes the ephemeral message key.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<EncryptedMessagePayload, EmberError> {
        if plaintext.len() > MAX_THREAD_MSG_BYTES {
            return Err(EmberError::InvalidMetadata(format!(
                "Message payload {} exceeds maximum {} bytes",
                plaintext.len(),
                MAX_THREAD_MSG_BYTES
            )));
        }

        // 1. Derive ephemeral message key MK
        let mut mk = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut mk,
            SUBKEY_ID_THREAD_MSG,
            &KDF_CONTEXT,
            &self.chain_key,
        )
        .map_err(|e| EmberError::Crypto(format!("Failed to derive message key: {e:?}")))?;

        // 2. Advance chain key immediately
        let mut next_chain_key = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut next_chain_key,
            SUBKEY_ID_THREAD_CHAIN_INIT,
            &KDF_CONTEXT,
            &self.chain_key,
        )
        .map_err(|e| EmberError::Crypto(format!("Failed to advance chain key: {e:?}")))?;

        self.chain_key.zeroize();
        self.chain_key = next_chain_key;

        // 3. Construct 24-byte nonce: 8-byte BE seq + 16 random bytes
        let mut nonce = [0u8; 24];
        nonce[0..8].copy_from_slice(&self.seq.to_be_bytes());
        copy_randombytes(&mut nonce[8..24]);

        // 4. Encrypt with XChaCha20-Poly1305
        let mut ciphertext = vec![0u8; plaintext.len() + 16];
        let encrypt_res = crypto_aead_xchacha20poly1305_ietf_encrypt(
            &mut ciphertext,
            plaintext,
            Some(AD_THREAD),
            &nonce,
            &mk,
        );

        // Zeroize ephemeral message key immediately
        mk.zeroize();

        encrypt_res
            .map_err(|e| EmberError::Crypto(format!("Thread message encryption failed: {e:?}")))?;

        let payload = EncryptedMessagePayload {
            seq: self.seq,
            nonce,
            ciphertext,
        };

        self.seq += 1;
        Ok(payload)
    }
}

/// Receiving ratchet maintaining forward secrecy for incoming thread messages.
pub struct ReceivingRatchet {
    chain_key: [u8; 32],
    expected_seq: u64,
}

impl Drop for ReceivingRatchet {
    fn drop(&mut self) {
        self.chain_key.zeroize();
    }
}

impl ReceivingRatchet {
    /// Initializes a receiving ratchet from a 32-byte initial chain key.
    pub fn new(initial_chain_key: [u8; 32]) -> Self {
        Self {
            chain_key: initial_chain_key,
            expected_seq: 0,
        }
    }

    /// Next expected message sequence number.
    pub fn expected_seq(&self) -> u64 {
        self.expected_seq
    }

    /// Decrypts a received message, advances the chain key, and rejects replays.
    pub fn decrypt(&mut self, payload: &EncryptedMessagePayload) -> Result<Vec<u8>, EmberError> {
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

        // 1. Derive message key MK
        let mut mk = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut mk,
            SUBKEY_ID_THREAD_MSG,
            &KDF_CONTEXT,
            &self.chain_key,
        )
        .map_err(|e| EmberError::Crypto(format!("Failed to derive message key: {e:?}")))?;

        // 2. Advance chain key immediately
        let mut next_chain_key = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut next_chain_key,
            SUBKEY_ID_THREAD_CHAIN_INIT,
            &KDF_CONTEXT,
            &self.chain_key,
        )
        .map_err(|e| EmberError::Crypto(format!("Failed to advance chain key: {e:?}")))?;

        self.chain_key.zeroize();
        self.chain_key = next_chain_key;

        // 3. Decrypt with XChaCha20-Poly1305
        let mut plaintext = vec![0u8; payload.ciphertext.len() - 16];
        let decrypt_res = crypto_aead_xchacha20poly1305_ietf_decrypt(
            &mut plaintext,
            &payload.ciphertext,
            Some(AD_THREAD),
            &payload.nonce,
            &mk,
        );

        // Zeroize ephemeral message key immediately
        mk.zeroize();

        decrypt_res.map_err(|_| EmberError::Auth)?;

        self.expected_seq += 1;
        Ok(plaintext)
    }
}
