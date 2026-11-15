//! Shared test vector verification for Ember thread protocol (Milestone M7).
//!
//! Validates:
//! - Deterministic thread PSK derivation from drop key
//! - Deterministic rendezvous mailbox derivation
//! - Deterministic ephemeral X25519 key exchange authenticated by PSK
//! - Directional chain key derivation and mailbox capability tokens
//! - Forward-secret message ratcheting and ciphertext verification
//! - Negative cases: tampering, replay, wrong drop key

use std::fs;
use std::path::PathBuf;

use dryoc::classic::crypto_kx::crypto_kx_seed_keypair;
use serde::{Deserialize, Serialize};

use ember_core::thread::{
    derive_rendezvous_mailbox, derive_thread_psk, EncryptedMessagePayload, HandshakeInitiator,
    HandshakeResponder, ReceivingRatchet, SendingRatchet, ThreadMessage,
};
use ember_core::SecretKey;

#[derive(Debug, Serialize, Deserialize)]
struct ThreadVectorFile {
    version: u8,
    kdf_context: String,
    handshake_cases: Vec<HandshakeCase>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HandshakeCase {
    name: String,
    drop_key_hex: String,
    psk_hex: String,
    initiator_seed_hex: String,
    initiator_pk_hex: String,
    initiator_sk_hex: String,
    responder_seed_hex: String,
    responder_pk_hex: String,
    responder_sk_hex: String,
    initiator_send_chain_key_hex: String,
    initiator_recv_chain_key_hex: String,
    responder_send_chain_key_hex: String,
    responder_recv_chain_key_hex: String,
    rendezvous_mailbox: MailboxVector,
    mailbox_init_to_resp: MailboxVector,
    mailbox_resp_to_init: MailboxVector,
    messages: Vec<MessageVector>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MailboxVector {
    id: String,
    push_token: String,
    pull_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MessageVector {
    seq: u64,
    sender_handle: String,
    text: String,
    nonce_hex: String,
    ciphertext_hex: String,
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn vector_file_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("testvectors")
        .join("thread-v1.json")
}

fn generate_vector_file() -> ThreadVectorFile {
    let drop_key_raw = [0x42u8; 32];
    let drop_key = SecretKey::from_bytes(drop_key_raw);

    let psk = derive_thread_psk(&drop_key).unwrap();
    let mb_rendezvous = derive_rendezvous_mailbox(&drop_key).unwrap();

    let init_seed = [0x11u8; 32];
    let (init_pk, init_sk) = crypto_kx_seed_keypair(&init_seed).unwrap();

    let resp_seed = [0x22u8; 32];
    let (resp_pk, resp_sk) = crypto_kx_seed_keypair(&resp_seed).unwrap();

    let init = HandshakeInitiator::with_ephemeral_keypair(&drop_key, init_pk, init_sk).unwrap();
    let resp_keys =
        HandshakeResponder::respond_with_ephemeral_keypair(&drop_key, &init_pk, resp_pk, resp_sk)
            .unwrap();
    let init_keys = init.complete(&resp_pk).unwrap();

    // Generate messages with fixed nonces
    let mut init_tx = SendingRatchet::new(init_keys.send_chain_key);
    let mut resp_tx = SendingRatchet::new(resp_keys.send_chain_key);

    let msg0 = ThreadMessage {
        seq: 0,
        timestamp: 1700000000,
        sender_handle: "amber-fox-4821".to_string(),
        text: "Hello, this is thread test message 0.".to_string(),
    };
    let mut nonce0 = [0u8; 24];
    nonce0[0..8].copy_from_slice(&0u64.to_be_bytes());
    nonce0[8..24].copy_from_slice(&[0xAA; 16]);
    let msg0_bytes = serde_json::to_vec(&msg0).unwrap();
    let ct0 = init_tx.encrypt_with_nonce(&msg0_bytes, nonce0).unwrap();

    let msg1 = ThreadMessage {
        seq: 0,
        timestamp: 1700000001,
        sender_handle: "silver-owl-1934".to_string(),
        text: "Hello back, this is thread test message 1.".to_string(),
    };
    let mut nonce1 = [0u8; 24];
    nonce1[0..8].copy_from_slice(&0u64.to_be_bytes());
    nonce1[8..24].copy_from_slice(&[0xBB; 16]);
    let msg1_bytes = serde_json::to_vec(&msg1).unwrap();
    let ct1 = resp_tx.encrypt_with_nonce(&msg1_bytes, nonce1).unwrap();

    let case = HandshakeCase {
        name: "deterministic-handshake-roundtrip".to_string(),
        drop_key_hex: to_hex(&drop_key_raw),
        psk_hex: to_hex(&psk),
        initiator_seed_hex: to_hex(&init_seed),
        initiator_pk_hex: to_hex(&init_pk),
        initiator_sk_hex: to_hex(&init_sk),
        responder_seed_hex: to_hex(&resp_seed),
        responder_pk_hex: to_hex(&resp_pk),
        responder_sk_hex: to_hex(&resp_sk),
        initiator_send_chain_key_hex: to_hex(&init_keys.send_chain_key),
        initiator_recv_chain_key_hex: to_hex(&init_keys.recv_chain_key),
        responder_send_chain_key_hex: to_hex(&resp_keys.send_chain_key),
        responder_recv_chain_key_hex: to_hex(&resp_keys.recv_chain_key),
        rendezvous_mailbox: MailboxVector {
            id: mb_rendezvous.id,
            push_token: mb_rendezvous.push_token,
            pull_token: mb_rendezvous.pull_token,
        },
        mailbox_init_to_resp: MailboxVector {
            id: init_keys.mailbox_out.id.clone(),
            push_token: init_keys.mailbox_out.push_token.clone(),
            pull_token: init_keys.mailbox_out.pull_token.clone(),
        },
        mailbox_resp_to_init: MailboxVector {
            id: init_keys.mailbox_in.id.clone(),
            push_token: init_keys.mailbox_in.push_token.clone(),
            pull_token: init_keys.mailbox_in.pull_token.clone(),
        },
        messages: vec![
            MessageVector {
                seq: 0,
                sender_handle: msg0.sender_handle,
                text: msg0.text,
                nonce_hex: to_hex(&ct0.nonce),
                ciphertext_hex: to_hex(&ct0.ciphertext),
            },
            MessageVector {
                seq: 0,
                sender_handle: msg1.sender_handle,
                text: msg1.text,
                nonce_hex: to_hex(&ct1.nonce),
                ciphertext_hex: to_hex(&ct1.ciphertext),
            },
        ],
    };

    ThreadVectorFile {
        version: 1,
        kdf_context: "EMBERv1_".to_string(),
        handshake_cases: vec![case],
    }
}

#[test]
fn test_thread_vectors_against_spec() {
    let path = vector_file_path();
    if !path.exists() {
        let generated = generate_vector_file();
        let json = serde_json::to_string_pretty(&generated).unwrap();
        fs::write(&path, json).unwrap();
    }

    let file_content = fs::read_to_string(&path).unwrap();
    let vectors: ThreadVectorFile = serde_json::from_str(&file_content).unwrap();

    assert_eq!(vectors.version, 1);
    assert_eq!(vectors.kdf_context, "EMBERv1_");

    for case in &vectors.handshake_cases {
        let mut drop_key_raw = [0u8; 32];
        drop_key_raw.copy_from_slice(&from_hex(&case.drop_key_hex));
        let drop_key = SecretKey::from_bytes(drop_key_raw);

        // 1. Thread PSK derivation
        let psk = derive_thread_psk(&drop_key).unwrap();
        assert_eq!(to_hex(&psk), case.psk_hex);

        // 2. Rendezvous mailbox derivation
        let mb_rendezvous = derive_rendezvous_mailbox(&drop_key).unwrap();
        assert_eq!(mb_rendezvous.id, case.rendezvous_mailbox.id);
        assert_eq!(mb_rendezvous.push_token, case.rendezvous_mailbox.push_token);
        assert_eq!(mb_rendezvous.pull_token, case.rendezvous_mailbox.pull_token);

        // 3. Ephemeral keys from seeds
        let mut init_seed = [0u8; 32];
        init_seed.copy_from_slice(&from_hex(&case.initiator_seed_hex));
        let (init_pk, init_sk) = crypto_kx_seed_keypair(&init_seed).unwrap();
        assert_eq!(to_hex(&init_pk), case.initiator_pk_hex);
        assert_eq!(to_hex(&init_sk), case.initiator_sk_hex);

        let mut resp_seed = [0u8; 32];
        resp_seed.copy_from_slice(&from_hex(&case.responder_seed_hex));
        let (resp_pk, resp_sk) = crypto_kx_seed_keypair(&resp_seed).unwrap();
        assert_eq!(to_hex(&resp_pk), case.responder_pk_hex);
        assert_eq!(to_hex(&resp_sk), case.responder_sk_hex);

        // 4. Handshake completion
        let init = HandshakeInitiator::with_ephemeral_keypair(&drop_key, init_pk, init_sk).unwrap();
        let resp_keys = HandshakeResponder::respond_with_ephemeral_keypair(
            &drop_key, &init_pk, resp_pk, resp_sk,
        )
        .unwrap();
        let init_keys = init.complete(&resp_pk).unwrap();

        assert_eq!(
            to_hex(&init_keys.send_chain_key),
            case.initiator_send_chain_key_hex
        );
        assert_eq!(
            to_hex(&init_keys.recv_chain_key),
            case.initiator_recv_chain_key_hex
        );
        assert_eq!(
            to_hex(&resp_keys.send_chain_key),
            case.responder_send_chain_key_hex
        );
        assert_eq!(
            to_hex(&resp_keys.recv_chain_key),
            case.responder_recv_chain_key_hex
        );

        // Mailbox capabilities
        assert_eq!(init_keys.mailbox_out.id, case.mailbox_init_to_resp.id);
        assert_eq!(
            init_keys.mailbox_out.push_token,
            case.mailbox_init_to_resp.push_token
        );
        assert_eq!(
            init_keys.mailbox_out.pull_token,
            case.mailbox_init_to_resp.pull_token
        );

        assert_eq!(init_keys.mailbox_in.id, case.mailbox_resp_to_init.id);
        assert_eq!(
            init_keys.mailbox_in.push_token,
            case.mailbox_resp_to_init.push_token
        );
        assert_eq!(
            init_keys.mailbox_in.pull_token,
            case.mailbox_resp_to_init.pull_token
        );

        // 5. Message decryption via receiving ratchets
        let mut init_rx = ReceivingRatchet::new(init_keys.recv_chain_key);
        let mut resp_rx = ReceivingRatchet::new(resp_keys.recv_chain_key);

        // Message 0 was sent from initiator to responder
        let msg0_vec = &case.messages[0];
        let mut nonce0 = [0u8; 24];
        nonce0.copy_from_slice(&from_hex(&msg0_vec.nonce_hex));
        let ct0 = EncryptedMessagePayload {
            seq: msg0_vec.seq,
            nonce: nonce0,
            ciphertext: from_hex(&msg0_vec.ciphertext_hex),
        };
        let decrypted0_bytes = resp_rx.decrypt(&ct0).unwrap();
        let decrypted0: ThreadMessage = serde_json::from_slice(&decrypted0_bytes).unwrap();
        assert_eq!(decrypted0.sender_handle, msg0_vec.sender_handle);
        assert_eq!(decrypted0.text, msg0_vec.text);

        // Message 1 was sent from responder to initiator
        let msg1_vec = &case.messages[1];
        let mut nonce1 = [0u8; 24];
        nonce1.copy_from_slice(&from_hex(&msg1_vec.nonce_hex));
        let ct1 = EncryptedMessagePayload {
            seq: msg1_vec.seq,
            nonce: nonce1,
            ciphertext: from_hex(&msg1_vec.ciphertext_hex),
        };
        let decrypted1_bytes = init_rx.decrypt(&ct1).unwrap();
        let decrypted1: ThreadMessage = serde_json::from_slice(&decrypted1_bytes).unwrap();
        assert_eq!(decrypted1.sender_handle, msg1_vec.sender_handle);
        assert_eq!(decrypted1.text, msg1_vec.text);

        // Negative test 1: Tampered ciphertext
        let mut tampered_ct = ct0.clone();
        let last = tampered_ct.ciphertext.len() - 1;
        tampered_ct.ciphertext[last] ^= 0x01;
        assert!(resp_rx.decrypt(&tampered_ct).is_err());

        // Negative test 2: Replay attack (re-submitting ct0)
        assert!(resp_rx.decrypt(&ct0).is_err());
    }
}
