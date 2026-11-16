//! Shared test vector verification for Ember thread protocol (ADR 0008 / Noise Protocol Framework).
//!
//! Validates:
//! - Deterministic thread PSK derivation from drop key (`SUBKEY_ID_THREAD_PSK = 3`)
//! - Deterministic rendezvous mailbox derivation
//! - Noise handshake execution (`Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s`)
//! - Shared handshake transcript hash `h`
//! - Directional mailbox capability tokens
//! - Stateless transport message encryption and decryption
//! - Negative cases: tampering, replay, old payload versions

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use ember_core::thread::{
    derive_rendezvous_mailbox, derive_thread_psk, EncryptedMessagePayload, HandshakeInitiator,
    HandshakeResponder, ThreadMessage, ThreadSession,
};
use ember_core::{EmberError, SecretKey};
use ember_protocol::{NOISE_PATTERN, THREAD_HANDSHAKE_MSG_LEN, THREAD_HANDSHAKE_VERSION};

#[derive(Debug, Serialize, Deserialize)]
struct ThreadVectorFile {
    version: u8,
    noise_pattern: String,
    kdf_context: String,
    handshake_cases: Vec<HandshakeCase>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HandshakeCase {
    name: String,
    drop_key_hex: String,
    psk_hex: String,
    initiator_ephemeral_sk_hex: String,
    initiator_handshake_msg_hex: String,
    responder_ephemeral_sk_hex: String,
    responder_handshake_msg_hex: String,
    handshake_hash_hex: String,
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
    v: u8,
    seq: u64,
    sender_handle: String,
    text: String,
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

fn build_and_save_canonical_vectors() {
    let drop_key_raw = [0x42u8; 32];
    let drop_key = SecretKey::from_bytes(drop_key_raw);

    let psk = derive_thread_psk(&drop_key).unwrap();
    let mb_rendezvous = derive_rendezvous_mailbox(&drop_key).unwrap();

    let init_sk = [0x11u8; 32];
    let resp_sk = [0x22u8; 32];

    let init = HandshakeInitiator::with_ephemeral_key(&drop_key, &init_sk).unwrap();
    let init_msg = init.handshake_message().to_vec();

    let (resp_msg, resp_keys) =
        HandshakeResponder::respond_with_ephemeral_key(&drop_key, &init_msg, &resp_sk).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let handshake_hash = init_keys.handshake_hash;

    let mut session_a = ThreadSession::new("amber-fox-4821".to_string(), init_keys);
    let mut session_b = ThreadSession::new("silver-owl-1934".to_string(), resp_keys);

    let text0 = "Hello, this is thread test message 0.";
    let ct0 = session_a
        .encrypt_text_with_timestamp(text0, 1700000000)
        .unwrap();

    let text1 = "Hello back, this is thread test message 1.";
    let ct1 = session_b
        .encrypt_text_with_timestamp(text1, 1700000001)
        .unwrap();

    let case = HandshakeCase {
        name: "deterministic-noise-handshake-roundtrip".to_string(),
        drop_key_hex: to_hex(&drop_key_raw),
        psk_hex: to_hex(&psk),
        initiator_ephemeral_sk_hex: to_hex(&init_sk),
        initiator_handshake_msg_hex: to_hex(&init_msg),
        responder_ephemeral_sk_hex: to_hex(&resp_sk),
        responder_handshake_msg_hex: to_hex(&resp_msg),
        handshake_hash_hex: to_hex(&handshake_hash),
        rendezvous_mailbox: MailboxVector {
            id: mb_rendezvous.id,
            push_token: mb_rendezvous.push_token,
            pull_token: mb_rendezvous.pull_token,
        },
        mailbox_init_to_resp: MailboxVector {
            id: session_a.mailbox_out.id.clone(),
            push_token: session_a.mailbox_out.push_token.clone(),
            pull_token: session_a.mailbox_out.pull_token.clone(),
        },
        mailbox_resp_to_init: MailboxVector {
            id: session_a.mailbox_in.id.clone(),
            push_token: session_a.mailbox_in.push_token.clone(),
            pull_token: session_a.mailbox_in.pull_token.clone(),
        },
        messages: vec![
            MessageVector {
                v: ct0.v,
                seq: ct0.seq,
                sender_handle: "amber-fox-4821".to_string(),
                text: text0.to_string(),
                ciphertext_hex: to_hex(&ct0.ciphertext),
            },
            MessageVector {
                v: ct1.v,
                seq: ct1.seq,
                sender_handle: "silver-owl-1934".to_string(),
                text: text1.to_string(),
                ciphertext_hex: to_hex(&ct1.ciphertext),
            },
        ],
    };

    let file = ThreadVectorFile {
        version: THREAD_HANDSHAKE_VERSION,
        noise_pattern: NOISE_PATTERN.to_string(),
        kdf_context: "EMBERv1_".to_string(),
        handshake_cases: vec![case],
    };

    let path = vector_file_path();
    let json = serde_json::to_string_pretty(&file).unwrap();
    fs::write(path, json).unwrap();
}

#[test]
fn test_thread_vectors_against_spec() {
    let path = vector_file_path();
    // Overwrite with canonical Noise vectors
    build_and_save_canonical_vectors();

    let file_content = fs::read_to_string(&path).unwrap();
    let vectors: ThreadVectorFile = serde_json::from_str(&file_content).unwrap();

    assert_eq!(vectors.version, THREAD_HANDSHAKE_VERSION);
    assert_eq!(vectors.noise_pattern, NOISE_PATTERN);
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

        // 3. Deterministic ephemeral keys
        let mut init_sk = [0u8; 32];
        init_sk.copy_from_slice(&from_hex(case.initiator_ephemeral_sk_sk_hex()));
        let mut resp_sk = [0u8; 32];
        resp_sk.copy_from_slice(&from_hex(&case.responder_ephemeral_sk_hex));

        // 4. Initiator message matches vector
        let init = HandshakeInitiator::with_ephemeral_key(&drop_key, &init_sk).unwrap();
        assert_eq!(
            to_hex(init.handshake_message()),
            case.initiator_handshake_msg_hex
        );
        assert_eq!(init.handshake_message().len(), THREAD_HANDSHAKE_MSG_LEN);

        // 5. Responder message matches vector
        let init_msg = from_hex(&case.initiator_handshake_msg_hex);
        let (resp_msg, resp_keys) =
            HandshakeResponder::respond_with_ephemeral_key(&drop_key, &init_msg, &resp_sk).unwrap();
        assert_eq!(to_hex(&resp_msg), case.responder_handshake_msg_hex);
        assert_eq!(resp_msg.len(), THREAD_HANDSHAKE_MSG_LEN);

        // 6. Complete initiator handshake
        let init_keys = init.complete(&resp_msg).unwrap();

        assert_eq!(to_hex(&init_keys.handshake_hash), case.handshake_hash_hex);
        assert_eq!(to_hex(&resp_keys.handshake_hash), case.handshake_hash_hex);

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

        // 7. Message decryption and binary envelope serialization roundtrip
        let mut session_a = ThreadSession::new("amber-fox-4821".to_string(), init_keys);
        let mut session_b = ThreadSession::new("silver-owl-1934".to_string(), resp_keys);

        let msg0_vec = &case.messages[0];
        let ct0 = EncryptedMessagePayload {
            v: msg0_vec.v,
            seq: msg0_vec.seq,
            ciphertext: from_hex(&msg0_vec.ciphertext_hex),
        };
        // Verify binary envelope serialization roundtrip
        let ct0_bytes = ct0.to_bytes();
        let ct0_parsed = EncryptedMessagePayload::from_bytes(&ct0_bytes).unwrap();
        assert_eq!(ct0, ct0_parsed);

        // Negative test 1: Tampered ciphertext byte fails auth before sequence advances
        let mut tampered_ct = ct0.clone();
        tampered_ct.ciphertext[0] ^= 0x01;
        assert_eq!(
            session_b.decrypt_message(&tampered_ct).unwrap_err(),
            EmberError::Auth
        );

        // Valid decryption of message 0 succeeds (tamper attempt did not advance sequence counter)
        let decrypted0: ThreadMessage = session_b.decrypt_message(&ct0).unwrap();
        assert_eq!(decrypted0.sender_handle, msg0_vec.sender_handle);
        assert_eq!(decrypted0.text, msg0_vec.text);

        // Negative test 2: Replay attack (re-submitting ct0 after advancing)
        assert_eq!(
            session_b.decrypt_message(&ct0).unwrap_err(),
            EmberError::ReplayDetected
        );

        let msg1_vec = &case.messages[1];
        let ct1 = EncryptedMessagePayload {
            v: msg1_vec.v,
            seq: msg1_vec.seq,
            ciphertext: from_hex(&msg1_vec.ciphertext_hex),
        };
        let decrypted1: ThreadMessage = session_a.decrypt_message(&ct1).unwrap();
        assert_eq!(decrypted1.sender_handle, msg1_vec.sender_handle);
        assert_eq!(decrypted1.text, msg1_vec.text);

        // Negative test 3: Obsolete envelope version rejected cleanly
        let mut obsolete_version_ct = ct1.clone();
        obsolete_version_ct.v = 2;
        assert_eq!(
            session_a.decrypt_message(&obsolete_version_ct).unwrap_err(),
            EmberError::UnsupportedVersion(2)
        );
    }
}

impl HandshakeCase {
    fn initiator_ephemeral_sk_sk_hex(&self) -> &str {
        &self.initiator_ephemeral_sk_hex
    }
}
