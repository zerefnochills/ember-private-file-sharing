//! Ephemeral Diffie-Hellman handshake using the audited Noise Protocol Framework (`snow`).
//!
//! Follows ADR 0008 (superseding ADR 0006):
//! - Handshake pattern: `Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s`.
//! - Ephemeral-to-ephemeral Diffie-Hellman authenticated with drop pre-shared key (PSK).
//! - Thread PSK derived via `crypto_kdf_derive_from_key(32, SUBKEY_ID_THREAD_PSK = 3, "EMBERv1_", drop_key)`.
//! - Handshake hash `h` (32 bytes) is identical on initiator and responder, binding the full exchange.
//! - Directional store-and-forward mailbox capabilities are derived deterministically from `h`.
//! - Envelope versioning: handshake messages use a leading 1-byte version prefix (`0x02`).

use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use dryoc::classic::crypto_kdf::crypto_kdf_derive_from_key;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::error::EmberError;
use crate::key::SecretKey;
use ember_protocol::{
    KDF_CONTEXT, NOISE_PATTERN, SUBKEY_ID_THREAD_CHAIN_INIT, SUBKEY_ID_THREAD_CHAIN_RESP,
    SUBKEY_ID_THREAD_MAILBOX, SUBKEY_ID_THREAD_PSK, SUBKEY_ID_THREAD_RENDEZVOUS,
    THREAD_HANDSHAKE_VERSION, THREAD_INITIATOR_MSG_LEN, THREAD_RESPONDER_MSG_LEN,
};

/// Mailbox capability authorization for one directional store-and-forward queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxCapability {
    /// Random URL-safe 22-character mailbox identifier.
    pub id: String,
    /// Bearer token authorizing push operations (posting messages).
    pub push_token: String,
    /// Bearer token authorizing pull operations (retrieving/deleting messages).
    pub pull_token: String,
}

impl MailboxCapability {
    /// SHA-256 hash of push_token (matching server representation).
    pub fn push_token_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.push_token.as_bytes());
        hasher.finalize().into()
    }

    /// SHA-256 hash of pull_token (matching server representation).
    pub fn pull_token_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.pull_token.as_bytes());
        hasher.finalize().into()
    }
}

/// Handshake result establishing a bidirectional encrypted thread session.
pub struct HandshakeSessionKeys {
    /// 32-byte initial sending chain key (derived from handshake hash for compatibility).
    pub send_chain_key: [u8; 32],
    /// 32-byte initial receiving chain key (derived from handshake hash for compatibility).
    pub recv_chain_key: [u8; 32],
    /// Mailbox used for sending messages.
    pub mailbox_out: MailboxCapability,
    /// Mailbox used for receiving messages.
    pub mailbox_in: MailboxCapability,
    /// 32-byte transcript handshake hash `h` binding the exchange.
    pub handshake_hash: [u8; 32],
    /// Underlying Noise stateless transport engine.
    pub transport: Arc<snow::StatelessTransportState>,
}

impl Drop for HandshakeSessionKeys {
    fn drop(&mut self) {
        self.send_chain_key.zeroize();
        self.recv_chain_key.zeroize();
        self.handshake_hash.zeroize();
    }
}

impl std::fmt::Debug for HandshakeSessionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandshakeSessionKeys")
            .field("mailbox_out", &self.mailbox_out)
            .field("mailbox_in", &self.mailbox_in)
            .field("send_chain_key", &"[REDACTED]")
            .field("recv_chain_key", &"[REDACTED]")
            .field("handshake_hash", &"[REDACTED]")
            .finish()
    }
}

impl HandshakeSessionKeys {
    /// Underlying Noise transport state reference.
    pub fn transport(&self) -> Arc<snow::StatelessTransportState> {
        self.transport.clone()
    }
}

/// Derives the 32-byte thread pre-shared key (PSK) from the drop master key.
///
/// Uses spec-reserved sub-key ID 3 and context `"EMBERv1_"`.
pub fn derive_thread_psk(drop_key: &SecretKey) -> Result<[u8; 32], EmberError> {
    let mut psk = [0u8; 32];
    crypto_kdf_derive_from_key(
        &mut psk,
        SUBKEY_ID_THREAD_PSK,
        &KDF_CONTEXT,
        drop_key.as_bytes(),
    )
    .map_err(|e| EmberError::Crypto(format!("Thread PSK derivation failed: {e:?}")))?;
    Ok(psk)
}

/// State of an in-progress thread handshake for the initiator.
pub struct HandshakeInitiator {
    psk: [u8; 32],
    handshake: Option<snow::HandshakeState>,
    msg: Vec<u8>,
}

impl Drop for HandshakeInitiator {
    fn drop(&mut self) {
        self.psk.zeroize();
        self.msg.zeroize();
    }
}

impl HandshakeInitiator {
    /// Begins a thread handshake as the initiator.
    pub fn new(drop_key: &SecretKey) -> Result<Self, EmberError> {
        let psk = derive_thread_psk(drop_key)?;
        let builder = snow::Builder::new(
            NOISE_PATTERN
                .parse()
                .map_err(|e| EmberError::Crypto(format!("Noise pattern parse error: {e:?}")))?,
        );
        let mut handshake = builder
            .psk(2, &psk)
            .map_err(|e| EmberError::Crypto(format!("Noise psk configuration failed: {e:?}")))?
            .build_initiator()
            .map_err(|e| EmberError::Crypto(format!("Noise initiator build failed: {e:?}")))?;

        let mut noise_buf = [0u8; 128];
        let len = handshake
            .write_message(&[], &mut noise_buf)
            .map_err(|e| EmberError::Crypto(format!("Noise message 1 generation failed: {e:?}")))?;

        let mut msg = Vec::with_capacity(1 + len);
        msg.push(THREAD_HANDSHAKE_VERSION);
        msg.extend_from_slice(&noise_buf[..len]);

        Ok(Self {
            psk,
            handshake: Some(handshake),
            msg,
        })
    }

    /// Begins a thread handshake with an explicit ephemeral secret key (for test vectors / deterministic testing).
    pub fn with_ephemeral_key(
        drop_key: &SecretKey,
        ephemeral_sk: &[u8; 32],
    ) -> Result<Self, EmberError> {
        let psk = derive_thread_psk(drop_key)?;
        let builder = snow::Builder::new(
            NOISE_PATTERN
                .parse()
                .map_err(|e| EmberError::Crypto(format!("Noise pattern parse error: {e:?}")))?,
        );
        let mut handshake = builder
            .psk(2, &psk)
            .map_err(|e| EmberError::Crypto(format!("Noise psk configuration failed: {e:?}")))?
            .fixed_ephemeral_key_for_testing_only(ephemeral_sk)
            .build_initiator()
            .map_err(|e| EmberError::Crypto(format!("Noise initiator build failed: {e:?}")))?;

        let mut noise_buf = [0u8; 128];
        let len = handshake
            .write_message(&[], &mut noise_buf)
            .map_err(|e| EmberError::Crypto(format!("Noise message 1 generation failed: {e:?}")))?;

        let mut msg = Vec::with_capacity(1 + len);
        msg.push(THREAD_HANDSHAKE_VERSION);
        msg.extend_from_slice(&noise_buf[..len]);

        Ok(Self {
            psk,
            handshake: Some(handshake),
            msg,
        })
    }

    /// Initiator's 49-byte versioned handshake message to post to the peer.
    pub fn handshake_message(&self) -> &[u8] {
        &self.msg
    }

    /// Public alias for the initiator's handshake message payload (backward compatibility).
    pub fn ephemeral_public_key(&self) -> Vec<u8> {
        self.msg.clone()
    }

    /// Completes the handshake upon receiving the responder's 49-byte handshake message.
    pub fn complete(mut self, responder_msg: &[u8]) -> Result<HandshakeSessionKeys, EmberError> {
        if responder_msg.is_empty() {
            return Err(EmberError::Auth);
        }

        if responder_msg[0] != THREAD_HANDSHAKE_VERSION {
            return Err(EmberError::UnsupportedVersion(responder_msg[0]));
        }

        if responder_msg.len() != THREAD_RESPONDER_MSG_LEN {
            return Err(EmberError::Auth);
        }

        let mut handshake = self.handshake.take().ok_or(EmberError::Auth)?;

        let mut payload_buf = [0u8; 64];
        if handshake
            .read_message(&responder_msg[1..], &mut payload_buf)
            .is_err()
        {
            return Err(EmberError::Auth);
        }

        if !handshake.is_handshake_finished() {
            return Err(EmberError::Auth);
        }

        let mut handshake_hash = [0u8; 32];
        let hash_slice = handshake.get_handshake_hash();
        if hash_slice.len() != 32 {
            return Err(EmberError::Crypto("Handshake hash length mismatch".into()));
        }
        handshake_hash.copy_from_slice(hash_slice);

        let stateless = handshake
            .into_stateless_transport_mode()
            .map_err(|e| EmberError::Crypto(format!("Stateless transition failed: {e:?}")))?;

        let (mb_init_to_resp, mb_resp_to_init) =
            derive_mailbox_capabilities_from_hash(&handshake_hash)?;

        let mut send_chain_key = [0u8; 32];
        let mut recv_chain_key = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut send_chain_key,
            SUBKEY_ID_THREAD_CHAIN_INIT,
            &KDF_CONTEXT,
            &handshake_hash,
        )
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
        crypto_kdf_derive_from_key(
            &mut recv_chain_key,
            SUBKEY_ID_THREAD_CHAIN_RESP,
            &KDF_CONTEXT,
            &handshake_hash,
        )
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;

        Ok(HandshakeSessionKeys {
            send_chain_key,
            recv_chain_key,
            mailbox_out: mb_init_to_resp,
            mailbox_in: mb_resp_to_init,
            handshake_hash,
            transport: Arc::new(stateless),
        })
    }
}

/// Responder for the ephemeral thread handshake.
pub struct HandshakeResponder;

impl HandshakeResponder {
    /// Responds to the initiator's handshake message with an explicit ephemeral key (for tests/vectors).
    pub fn respond_with_ephemeral_key(
        drop_key: &SecretKey,
        initiator_msg: &[u8],
        ephemeral_sk: &[u8; 32],
    ) -> Result<(Vec<u8>, HandshakeSessionKeys), EmberError> {
        if initiator_msg.is_empty() {
            return Err(EmberError::Auth);
        }

        if initiator_msg[0] != THREAD_HANDSHAKE_VERSION {
            return Err(EmberError::UnsupportedVersion(initiator_msg[0]));
        }

        if initiator_msg.len() != THREAD_INITIATOR_MSG_LEN {
            return Err(EmberError::Auth);
        }

        let mut psk = derive_thread_psk(drop_key)?;
        let builder = snow::Builder::new(
            NOISE_PATTERN
                .parse()
                .map_err(|e| EmberError::Crypto(format!("Noise pattern parse error: {e:?}")))?,
        );

        let mut responder = builder
            .psk(2, &psk)
            .map_err(|e| EmberError::Crypto(format!("Noise psk configuration failed: {e:?}")))?
            .fixed_ephemeral_key_for_testing_only(ephemeral_sk)
            .build_responder()
            .map_err(|e| EmberError::Crypto(format!("Noise responder build failed: {e:?}")))?;

        let mut payload_buf = [0u8; 64];
        responder
            .read_message(&initiator_msg[1..], &mut payload_buf)
            .map_err(|_| EmberError::Auth)?;

        let mut noise_msg2 = [0u8; 128];
        let len = responder
            .write_message(&[], &mut noise_msg2)
            .map_err(|e| EmberError::Crypto(format!("Noise message 2 generation failed: {e:?}")))?;

        let mut resp_msg = Vec::with_capacity(1 + len);
        resp_msg.push(THREAD_HANDSHAKE_VERSION);
        resp_msg.extend_from_slice(&noise_msg2[..len]);

        if !responder.is_handshake_finished() {
            psk.zeroize();
            return Err(EmberError::Auth);
        }

        let mut handshake_hash = [0u8; 32];
        let hash_slice = responder.get_handshake_hash();
        if hash_slice.len() != 32 {
            psk.zeroize();
            return Err(EmberError::Crypto("Handshake hash length mismatch".into()));
        }
        handshake_hash.copy_from_slice(hash_slice);

        let stateless = responder
            .into_stateless_transport_mode()
            .map_err(|e| EmberError::Crypto(format!("Stateless transition failed: {e:?}")))?;

        let (mb_init_to_resp, mb_resp_to_init) =
            derive_mailbox_capabilities_from_hash(&handshake_hash)?;

        let mut send_chain_key = [0u8; 32];
        let mut recv_chain_key = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut send_chain_key,
            SUBKEY_ID_THREAD_CHAIN_RESP,
            &KDF_CONTEXT,
            &handshake_hash,
        )
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
        crypto_kdf_derive_from_key(
            &mut recv_chain_key,
            SUBKEY_ID_THREAD_CHAIN_INIT,
            &KDF_CONTEXT,
            &handshake_hash,
        )
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;

        psk.zeroize();

        let keys = HandshakeSessionKeys {
            send_chain_key,
            recv_chain_key,
            mailbox_out: mb_resp_to_init,
            mailbox_in: mb_init_to_resp,
            handshake_hash,
            transport: Arc::new(stateless),
        };

        Ok((resp_msg, keys))
    }

    /// Responds to the initiator's handshake message with a fresh random ephemeral key.
    pub fn respond(
        drop_key: &SecretKey,
        initiator_msg: &[u8],
    ) -> Result<(Vec<u8>, HandshakeSessionKeys), EmberError> {
        if initiator_msg.is_empty() {
            return Err(EmberError::Auth);
        }

        if initiator_msg[0] != THREAD_HANDSHAKE_VERSION {
            return Err(EmberError::UnsupportedVersion(initiator_msg[0]));
        }

        if initiator_msg.len() != THREAD_INITIATOR_MSG_LEN {
            return Err(EmberError::Auth);
        }

        let mut psk = derive_thread_psk(drop_key)?;
        let builder = snow::Builder::new(
            NOISE_PATTERN
                .parse()
                .map_err(|e| EmberError::Crypto(format!("Noise pattern parse error: {e:?}")))?,
        );

        let mut responder = builder
            .psk(2, &psk)
            .map_err(|e| EmberError::Crypto(format!("Noise psk configuration failed: {e:?}")))?
            .build_responder()
            .map_err(|e| EmberError::Crypto(format!("Noise responder build failed: {e:?}")))?;

        let mut payload_buf = [0u8; 64];
        responder
            .read_message(&initiator_msg[1..], &mut payload_buf)
            .map_err(|_| EmberError::Auth)?;

        let mut noise_msg2 = [0u8; 128];
        let len = responder
            .write_message(&[], &mut noise_msg2)
            .map_err(|e| EmberError::Crypto(format!("Noise message 2 generation failed: {e:?}")))?;

        let mut resp_msg = Vec::with_capacity(1 + len);
        resp_msg.push(THREAD_HANDSHAKE_VERSION);
        resp_msg.extend_from_slice(&noise_msg2[..len]);

        if !responder.is_handshake_finished() {
            psk.zeroize();
            return Err(EmberError::Auth);
        }

        let mut handshake_hash = [0u8; 32];
        let hash_slice = responder.get_handshake_hash();
        if hash_slice.len() != 32 {
            psk.zeroize();
            return Err(EmberError::Crypto("Handshake hash length mismatch".into()));
        }
        handshake_hash.copy_from_slice(hash_slice);

        let stateless = responder
            .into_stateless_transport_mode()
            .map_err(|e| EmberError::Crypto(format!("Stateless transition failed: {e:?}")))?;

        let (mb_init_to_resp, mb_resp_to_init) =
            derive_mailbox_capabilities_from_hash(&handshake_hash)?;

        let mut send_chain_key = [0u8; 32];
        let mut recv_chain_key = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut send_chain_key,
            SUBKEY_ID_THREAD_CHAIN_RESP,
            &KDF_CONTEXT,
            &handshake_hash,
        )
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
        crypto_kdf_derive_from_key(
            &mut recv_chain_key,
            SUBKEY_ID_THREAD_CHAIN_INIT,
            &KDF_CONTEXT,
            &handshake_hash,
        )
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;

        psk.zeroize();

        let keys = HandshakeSessionKeys {
            send_chain_key,
            recv_chain_key,
            mailbox_out: mb_resp_to_init,
            mailbox_in: mb_init_to_resp,
            handshake_hash,
            transport: Arc::new(stateless),
        };

        Ok((resp_msg, keys))
    }
}

/// Derives deterministic mailbox capability identifiers and tokens from the shared handshake hash.
fn derive_mailbox_capabilities_from_hash(
    h: &[u8; 32],
) -> Result<(MailboxCapability, MailboxCapability), EmberError> {
    let mut seed = [0u8; 32];
    crypto_kdf_derive_from_key(&mut seed, SUBKEY_ID_THREAD_MAILBOX, &KDF_CONTEXT, h)
        .map_err(|e| EmberError::Crypto(format!("Mailbox seed derivation failed: {e:?}")))?;

    // Derive Mailbox 1 (Init -> Resp)
    let mut mb1_id_raw = [0u8; 32];
    let mut mb1_push_raw = [0u8; 32];
    let mut mb1_pull_raw = [0u8; 32];
    crypto_kdf_derive_from_key(&mut mb1_id_raw, 10, &KDF_CONTEXT, &seed)
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
    crypto_kdf_derive_from_key(&mut mb1_push_raw, 11, &KDF_CONTEXT, &seed)
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
    crypto_kdf_derive_from_key(&mut mb1_pull_raw, 12, &KDF_CONTEXT, &seed)
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;

    // Derive Mailbox 2 (Resp -> Init)
    let mut mb2_id_raw = [0u8; 32];
    let mut mb2_push_raw = [0u8; 32];
    let mut mb2_pull_raw = [0u8; 32];
    crypto_kdf_derive_from_key(&mut mb2_id_raw, 20, &KDF_CONTEXT, &seed)
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
    crypto_kdf_derive_from_key(&mut mb2_push_raw, 21, &KDF_CONTEXT, &seed)
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
    crypto_kdf_derive_from_key(&mut mb2_pull_raw, 22, &KDF_CONTEXT, &seed)
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;

    let mb1 = MailboxCapability {
        id: URL_SAFE_NO_PAD.encode(&mb1_id_raw[0..16]),
        push_token: URL_SAFE_NO_PAD.encode(mb1_push_raw),
        pull_token: URL_SAFE_NO_PAD.encode(mb1_pull_raw),
    };

    let mb2 = MailboxCapability {
        id: URL_SAFE_NO_PAD.encode(&mb2_id_raw[0..16]),
        push_token: URL_SAFE_NO_PAD.encode(mb2_push_raw),
        pull_token: URL_SAFE_NO_PAD.encode(mb2_pull_raw),
    };

    seed.zeroize();
    mb1_id_raw.zeroize();
    mb1_push_raw.zeroize();
    mb1_pull_raw.zeroize();
    mb2_id_raw.zeroize();
    mb2_push_raw.zeroize();
    mb2_pull_raw.zeroize();

    Ok((mb1, mb2))
}

/// Derives the rendezvous mailbox capability deterministically from the drop master key.
pub fn derive_rendezvous_mailbox(drop_key: &SecretKey) -> Result<MailboxCapability, EmberError> {
    let mut seed = [0u8; 32];
    crypto_kdf_derive_from_key(
        &mut seed,
        SUBKEY_ID_THREAD_RENDEZVOUS,
        &KDF_CONTEXT,
        drop_key.as_bytes(),
    )
    .map_err(|e| EmberError::Crypto(format!("Rendezvous seed derivation failed: {e:?}")))?;

    let mut id_raw = [0u8; 32];
    let mut push_raw = [0u8; 32];
    let mut pull_raw = [0u8; 32];

    crypto_kdf_derive_from_key(&mut id_raw, 1, &KDF_CONTEXT, &seed)
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
    crypto_kdf_derive_from_key(&mut push_raw, 2, &KDF_CONTEXT, &seed)
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
    crypto_kdf_derive_from_key(&mut pull_raw, 3, &KDF_CONTEXT, &seed)
        .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;

    seed.zeroize();

    let cap = MailboxCapability {
        id: URL_SAFE_NO_PAD.encode(&id_raw[0..16]),
        push_token: URL_SAFE_NO_PAD.encode(push_raw),
        pull_token: URL_SAFE_NO_PAD.encode(pull_raw),
    };
    id_raw.zeroize();
    push_raw.zeroize();
    pull_raw.zeroize();
    Ok(cap)
}
