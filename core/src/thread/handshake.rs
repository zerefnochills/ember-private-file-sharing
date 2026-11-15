//! Ephemeral Diffie-Hellman handshake with Pre-Shared Key (PSK) authentication.
//!
//! Follows ADR 0006:
//! - Uses ephemeral X25519 key exchange authenticated with drop key subkey `K_thread`.
//! - Derives forward-secret sending and receiving chain keys.
//! - Derives separate store-and-forward mailbox capability tokens for each direction:
//!   - `mailbox_out`: where this client posts outgoing messages.
//!   - `mailbox_in`: where this client polls/listens for incoming messages.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use dryoc::classic::crypto_generichash::crypto_generichash;
use dryoc::classic::crypto_kdf::crypto_kdf_derive_from_key;
use dryoc::classic::crypto_kx::{
    crypto_kx_client_session_keys, crypto_kx_keypair, crypto_kx_server_session_keys,
};
use zeroize::Zeroize;

use crate::error::EmberError;
use crate::key::SecretKey;
use ember_protocol::{
    KDF_CONTEXT, SUBKEY_ID_THREAD_CHAIN_INIT, SUBKEY_ID_THREAD_CHAIN_RESP,
    SUBKEY_ID_THREAD_MAILBOX, SUBKEY_ID_THREAD_PSK, SUBKEY_ID_THREAD_RENDEZVOUS,
};
use sha2::{Digest, Sha256};

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
    /// 32-byte initial sending chain key.
    pub send_chain_key: [u8; 32],
    /// 32-byte initial receiving chain key.
    pub recv_chain_key: [u8; 32],
    /// Mailbox used for sending messages.
    pub mailbox_out: MailboxCapability,
    /// Mailbox used for receiving messages.
    pub mailbox_in: MailboxCapability,
}

impl Drop for HandshakeSessionKeys {
    fn drop(&mut self) {
        self.send_chain_key.zeroize();
        self.recv_chain_key.zeroize();
    }
}

/// Derives the 32-byte thread pre-shared key (PSK) from the drop master key.
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

/// State of an in-progress thread handshake.
pub struct HandshakeInitiator {
    psk: [u8; 32],
    client_pk: [u8; 32],
    client_sk: [u8; 32],
}

impl Drop for HandshakeInitiator {
    fn drop(&mut self) {
        self.psk.zeroize();
        self.client_sk.zeroize();
    }
}

impl HandshakeInitiator {
    /// Begins a thread handshake as the initiator.
    pub fn new(drop_key: &SecretKey) -> Result<Self, EmberError> {
        let psk = derive_thread_psk(drop_key)?;
        let (client_pk, client_sk) = crypto_kx_keypair();
        Ok(Self {
            psk,
            client_pk,
            client_sk,
        })
    }

    /// Begins a thread handshake with an explicit ephemeral keypair (for test vectors / deterministic testing).
    pub fn with_ephemeral_keypair(
        drop_key: &SecretKey,
        client_pk: [u8; 32],
        client_sk: [u8; 32],
    ) -> Result<Self, EmberError> {
        let psk = derive_thread_psk(drop_key)?;
        Ok(Self {
            psk,
            client_pk,
            client_sk,
        })
    }

    /// The initiator's 32-byte ephemeral public key to send to the responder.
    pub fn ephemeral_public_key(&self) -> [u8; 32] {
        self.client_pk
    }

    /// Completes the handshake upon receiving the responder's ephemeral public key.
    pub fn complete(self, responder_pk: &[u8; 32]) -> Result<HandshakeSessionKeys, EmberError> {
        let mut crx = [0u8; 32];
        let mut ctx = [0u8; 32];

        crypto_kx_client_session_keys(
            &mut crx,
            &mut ctx,
            &self.client_pk,
            &self.client_sk,
            responder_pk,
        )
        .map_err(|_| EmberError::Auth)?;

        // Authenticate with thread PSK via keyed BLAKE2b to produce directional chain keys
        let mut auth_tx = [0u8; 32];
        let mut auth_rx = [0u8; 32];
        crypto_generichash(&mut auth_tx, &ctx, Some(&self.psk))
            .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
        crypto_generichash(&mut auth_rx, &crx, Some(&self.psk))
            .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;

        let mut send_chain_key = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut send_chain_key,
            SUBKEY_ID_THREAD_CHAIN_INIT,
            &KDF_CONTEXT,
            &auth_tx,
        )
        .map_err(|e| EmberError::Crypto(format!("Failed to derive initiator send chain: {e:?}")))?;

        let mut recv_chain_key = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut recv_chain_key,
            SUBKEY_ID_THREAD_CHAIN_RESP,
            &KDF_CONTEXT,
            &auth_rx,
        )
        .map_err(|e| EmberError::Crypto(format!("Failed to derive initiator recv chain: {e:?}")))?;

        let (mb_init_to_resp, mb_resp_to_init) =
            derive_mailbox_capabilities(&self.psk, &self.client_pk, responder_pk)?;

        crx.zeroize();
        ctx.zeroize();
        auth_tx.zeroize();
        auth_rx.zeroize();

        Ok(HandshakeSessionKeys {
            send_chain_key,
            recv_chain_key,
            mailbox_out: mb_init_to_resp,
            mailbox_in: mb_resp_to_init,
        })
    }
}

/// Responds to a thread handshake.
pub struct HandshakeResponder;

impl HandshakeResponder {
    /// Completes a handshake with an explicit ephemeral keypair (for test vectors / deterministic testing).
    pub fn respond_with_ephemeral_keypair(
        drop_key: &SecretKey,
        initiator_pk: &[u8; 32],
        server_pk: [u8; 32],
        mut server_sk: [u8; 32],
    ) -> Result<HandshakeSessionKeys, EmberError> {
        let mut psk = derive_thread_psk(drop_key)?;

        let mut srx = [0u8; 32];
        let mut stx = [0u8; 32];

        crypto_kx_server_session_keys(&mut srx, &mut stx, &server_pk, &server_sk, initiator_pk)
            .map_err(|_| EmberError::Auth)?;

        server_sk.zeroize();

        // Authenticate with thread PSK via keyed BLAKE2b to produce directional chain keys
        let mut auth_rx = [0u8; 32];
        let mut auth_tx = [0u8; 32];
        crypto_generichash(&mut auth_rx, &srx, Some(&psk))
            .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;
        crypto_generichash(&mut auth_tx, &stx, Some(&psk))
            .map_err(|e| EmberError::Crypto(format!("{e:?}")))?;

        // Note: server's send is initiator's receive (SUBKEY_ID_THREAD_CHAIN_RESP)
        let mut send_chain_key = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut send_chain_key,
            SUBKEY_ID_THREAD_CHAIN_RESP,
            &KDF_CONTEXT,
            &auth_tx,
        )
        .map_err(|e| EmberError::Crypto(format!("Failed to derive responder send chain: {e:?}")))?;

        // server's recv is initiator's send (SUBKEY_ID_THREAD_CHAIN_INIT)
        let mut recv_chain_key = [0u8; 32];
        crypto_kdf_derive_from_key(
            &mut recv_chain_key,
            SUBKEY_ID_THREAD_CHAIN_INIT,
            &KDF_CONTEXT,
            &auth_rx,
        )
        .map_err(|e| EmberError::Crypto(format!("Failed to derive responder recv chain: {e:?}")))?;

        let (mb_init_to_resp, mb_resp_to_init) =
            derive_mailbox_capabilities(&psk, initiator_pk, &server_pk)?;

        psk.zeroize();
        srx.zeroize();
        stx.zeroize();
        auth_rx.zeroize();
        auth_tx.zeroize();

        Ok(HandshakeSessionKeys {
            send_chain_key,
            recv_chain_key,
            mailbox_out: mb_resp_to_init,
            mailbox_in: mb_init_to_resp,
        })
    }

    /// Completes a handshake as the responder upon receiving initiator_pk.
    pub fn respond(
        drop_key: &SecretKey,
        initiator_pk: &[u8; 32],
    ) -> Result<([u8; 32], HandshakeSessionKeys), EmberError> {
        let (server_pk, server_sk) = crypto_kx_keypair();
        let keys =
            Self::respond_with_ephemeral_keypair(drop_key, initiator_pk, server_pk, server_sk)?;
        Ok((server_pk, keys))
    }
}

/// Derives deterministic mailbox capability identifiers and tokens for both directions.
fn derive_mailbox_capabilities(
    psk: &[u8; 32],
    init_pk: &[u8; 32],
    resp_pk: &[u8; 32],
) -> Result<(MailboxCapability, MailboxCapability), EmberError> {
    let mut material = Vec::with_capacity(96);
    material.extend_from_slice(psk);
    material.extend_from_slice(init_pk);
    material.extend_from_slice(resp_pk);

    let mut seed = [0u8; 32];
    crypto_kdf_derive_from_key(&mut seed, SUBKEY_ID_THREAD_MAILBOX, &KDF_CONTEXT, psk)
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

    seed.zeroize();

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

    Ok((mb1, mb2))
}

/// Derives deterministic rendezvous mailbox capability from the drop key.
pub fn derive_rendezvous_mailbox(drop_key: &SecretKey) -> Result<MailboxCapability, EmberError> {
    let mut psk = derive_thread_psk(drop_key)?;
    let mut seed = [0u8; 32];
    crypto_kdf_derive_from_key(&mut seed, SUBKEY_ID_THREAD_RENDEZVOUS, &KDF_CONTEXT, &psk)
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

    psk.zeroize();
    seed.zeroize();

    Ok(MailboxCapability {
        id: URL_SAFE_NO_PAD.encode(&id_raw[0..16]),
        push_token: URL_SAFE_NO_PAD.encode(push_raw),
        pull_token: URL_SAFE_NO_PAD.encode(pull_raw),
    })
}
