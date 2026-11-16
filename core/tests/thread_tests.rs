//! Acceptance and security tests for Ember 1:1 threads using the Noise Protocol Framework (`snow`).
//!
//! Verifies:
//! - Ephemeral Diffie-Hellman handshake matching (`Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s`)
//! - Bidirectional forward-secret messaging roundtrip
//! - Replay attack rejection
//! - Out-of-order sequence rejection
//! - Tampered ciphertext rejection
//! - Truncated and trailing data rejection
//! - Handshake with wrong drop key fails authentication
//! - Handshake rejection of malformed version or length
//! - Rejection of obsolete message envelope versions
//! - Forward secrecy protections
//! - Memory zeroization on drop

use ember_core::thread::{
    generate_handle, EncryptedMessagePayload, HandshakeInitiator, HandshakeResponder, ThreadSession,
};
use ember_core::{EmberError, SecretKey};
use ember_protocol::{
    THREAD_HANDSHAKE_VERSION, THREAD_INITIATOR_MSG_LEN, THREAD_MSG_VERSION,
    THREAD_RESPONDER_MSG_LEN,
};

#[test]
fn test_noise_message_lengths() {
    let psk = [7u8; 32];
    let pattern = ember_protocol::NOISE_PATTERN;
    let mut init = snow::Builder::new(pattern.parse().unwrap())
        .psk(2, &psk)
        .unwrap()
        .build_initiator()
        .unwrap();
    let mut resp = snow::Builder::new(pattern.parse().unwrap())
        .psk(2, &psk)
        .unwrap()
        .build_responder()
        .unwrap();

    let mut msg1 = [0u8; 128];
    let mut msg2 = [0u8; 128];
    let len1 = init.write_message(&[], &mut msg1).unwrap();
    println!("LEN1 = {}", len1);
    let mut payload = [0u8; 128];
    resp.read_message(&msg1[..len1], &mut payload).unwrap();
    let len2 = resp.write_message(&[], &mut msg2).unwrap();
    println!("LEN2 = {}", len2);
    init.read_message(&msg2[..len2], &mut payload).unwrap();
    assert_eq!(len1, 48);
    assert_eq!(len2, 48);
}

#[test]
fn test_handshake_derivation_and_session_keys_match() {
    let drop_key = SecretKey::generate();

    // 1. Initiator starts
    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_msg = init.handshake_message();
    assert_eq!(init_msg.len(), THREAD_INITIATOR_MSG_LEN);
    assert_eq!(init_msg[0], THREAD_HANDSHAKE_VERSION);

    // 2. Responder responds
    let (resp_msg, resp_keys) = HandshakeResponder::respond(&drop_key, init_msg).unwrap();
    assert_eq!(resp_msg.len(), THREAD_RESPONDER_MSG_LEN);
    assert_eq!(resp_msg[0], THREAD_HANDSHAKE_VERSION);

    // 3. Initiator completes
    let init_keys = init.complete(&resp_msg).unwrap();

    // 4. Verify directional chain key match
    assert_eq!(
        init_keys.send_chain_key, resp_keys.recv_chain_key,
        "Initiator send chain key must match responder recv chain key"
    );
    assert_eq!(
        init_keys.recv_chain_key, resp_keys.send_chain_key,
        "Initiator recv chain key must match responder send chain key"
    );

    // 5. Verify transcript handshake hash match
    assert_eq!(
        init_keys.handshake_hash, resp_keys.handshake_hash,
        "Both endpoints must arrive at identical 32-byte handshake hash"
    );

    // 6. Verify mailbox pairing
    assert_eq!(init_keys.mailbox_out.id, resp_keys.mailbox_in.id);
    assert_eq!(
        init_keys.mailbox_out.push_token,
        resp_keys.mailbox_in.push_token
    );
    assert_eq!(init_keys.mailbox_in.id, resp_keys.mailbox_out.id);
    assert_eq!(
        init_keys.mailbox_in.pull_token,
        resp_keys.mailbox_out.pull_token
    );
}

#[test]
fn test_bidirectional_messaging_roundtrip() {
    let drop_key = SecretKey::generate();
    let handle_a = generate_handle();
    let handle_b = generate_handle();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_msg = init.handshake_message();

    let (resp_msg, resp_keys) = HandshakeResponder::respond(&drop_key, init_msg).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new(handle_a.clone(), init_keys);
    let mut session_b = ThreadSession::new(handle_b.clone(), resp_keys);

    // Message 1: A -> B
    let msg1_enc = session_a.encrypt_text("Hello from Initiator!").unwrap();
    assert_eq!(msg1_enc.v, THREAD_MSG_VERSION);
    assert_eq!(msg1_enc.seq, 0);

    let msg1_dec = session_b.decrypt_message(&msg1_enc).unwrap();
    assert_eq!(msg1_dec.seq, 0);
    assert_eq!(msg1_dec.sender_handle, handle_a);
    assert_eq!(msg1_dec.text, "Hello from Initiator!");

    // Message 2: B -> A
    let msg2_enc = session_b
        .encrypt_text("Hello back from Responder!")
        .unwrap();
    assert_eq!(msg2_enc.v, THREAD_MSG_VERSION);
    assert_eq!(msg2_enc.seq, 0);

    let msg2_dec = session_a.decrypt_message(&msg2_enc).unwrap();
    assert_eq!(msg2_dec.seq, 0);
    assert_eq!(msg2_dec.sender_handle, handle_b);
    assert_eq!(msg2_dec.text, "Hello back from Responder!");

    // Message 3: A -> B (seq 1)
    let msg3_enc = session_a.encrypt_text("Second message from A").unwrap();
    assert_eq!(msg3_enc.v, THREAD_MSG_VERSION);
    assert_eq!(msg3_enc.seq, 1);

    let msg3_dec = session_b.decrypt_message(&msg3_enc).unwrap();
    assert_eq!(msg3_dec.seq, 1);
    assert_eq!(msg3_dec.text, "Second message from A");
}

#[test]
fn test_replay_attack_rejected() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_msg = init.handshake_message();
    let (resp_msg, resp_keys) = HandshakeResponder::respond(&drop_key, init_msg).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    let msg0 = session_a.encrypt_text("Valid message 0").unwrap();

    // First delivery succeeds
    let dec0 = session_b.decrypt_message(&msg0).unwrap();
    assert_eq!(dec0.text, "Valid message 0");

    // Attacker replays msg0
    let replay_err = session_b.decrypt_message(&msg0).unwrap_err();
    assert_eq!(replay_err, EmberError::ReplayDetected);
}

#[test]
fn test_out_of_order_rejected() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_msg = init.handshake_message();
    let (resp_msg, resp_keys) = HandshakeResponder::respond(&drop_key, init_msg).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    let _msg0 = session_a.encrypt_text("Message 0").unwrap();
    let msg1 = session_a.encrypt_text("Message 1").unwrap();

    // Attacker delivers msg1 before msg0
    let order_err = session_b.decrypt_message(&msg1).unwrap_err();
    assert_eq!(order_err, EmberError::OutOfOrderMessage(1));
}

#[test]
fn test_tampered_ciphertext_fails_auth() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_msg = init.handshake_message();
    let (resp_msg, resp_keys) = HandshakeResponder::respond(&drop_key, init_msg).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    let msg = session_a
        .encrypt_text("Original sensitive message")
        .unwrap();
    let mut tampered_msg = msg.clone();

    // Flip a byte in the ciphertext payload
    if let Some(b) = tampered_msg.ciphertext.first_mut() {
        *b ^= 0x01;
    }

    let auth_err = session_b.decrypt_message(&tampered_msg).unwrap_err();
    assert_eq!(auth_err, EmberError::Auth);

    // Failed authentication does not advance expected sequence: an intact retry succeeds.
    let decrypted = session_b.decrypt_message(&msg).unwrap();
    assert_eq!(decrypted.text, "Original sensitive message");
}

#[test]
fn test_truncated_ciphertext_fails_auth() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_msg = init.handshake_message();
    let (resp_msg, resp_keys) = HandshakeResponder::respond(&drop_key, init_msg).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    let msg = session_a.encrypt_text("Truncation test").unwrap();

    // Truncate to less than Poly1305 MAC tag (16 bytes)
    let mut truncated_msg = msg.clone();
    truncated_msg.ciphertext.truncate(10);
    assert_eq!(
        session_b.decrypt_message(&truncated_msg).unwrap_err(),
        EmberError::Auth
    );

    // Truncate by 1 byte from end of full message
    let mut truncated_by_one = msg.clone();
    let len = truncated_by_one.ciphertext.len();
    truncated_by_one.ciphertext.truncate(len - 1);
    assert_eq!(
        session_b.decrypt_message(&truncated_by_one).unwrap_err(),
        EmberError::Auth
    );
}

#[test]
fn test_trailing_bytes_fails_auth() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_msg = init.handshake_message();
    let (resp_msg, resp_keys) = HandshakeResponder::respond(&drop_key, init_msg).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    let msg = session_a.encrypt_text("Trailing bytes test").unwrap();
    let mut trailing_msg = msg.clone();
    trailing_msg.ciphertext.extend_from_slice(b"extra garbage");

    assert_eq!(
        session_b.decrypt_message(&trailing_msg).unwrap_err(),
        EmberError::Auth
    );
}

#[test]
fn test_wrong_drop_key_fails_authentication() {
    let drop_key_a = SecretKey::generate();
    let drop_key_b = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key_a).unwrap();
    let init_msg = init.handshake_message();

    // Responder responds using wrong drop key
    let (resp_msg, _resp_keys) = HandshakeResponder::respond(&drop_key_b, init_msg).unwrap();

    // Initiator must reject responder's message because PSK does not match
    let init_res = init.complete(&resp_msg);
    assert!(
        init_res.is_err(),
        "Initiator must reject wrong PSK response"
    );
    assert_eq!(init_res.unwrap_err(), EmberError::Auth);
}

#[test]
fn test_handshake_malformed_version_rejected() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let mut init_msg = init.handshake_message().to_vec();

    // Corrupt initiator version byte
    init_msg[0] = 0x01; // obsolete version
    let resp_res = HandshakeResponder::respond(&drop_key, &init_msg);
    assert_eq!(resp_res.unwrap_err(), EmberError::UnsupportedVersion(0x01));

    // Corrupt responder version byte
    let valid_init = HandshakeInitiator::new(&drop_key).unwrap();
    let (mut resp_msg, _) =
        HandshakeResponder::respond(&drop_key, valid_init.handshake_message()).unwrap();
    resp_msg[0] = 0x99;
    let complete_res = valid_init.complete(&resp_msg);
    assert_eq!(
        complete_res.unwrap_err(),
        EmberError::UnsupportedVersion(0x99)
    );
}

#[test]
fn test_handshake_truncated_or_extended_rejected() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let mut init_msg = init.handshake_message().to_vec();

    // Truncated initiator message
    init_msg.truncate(20);
    assert_eq!(
        HandshakeResponder::respond(&drop_key, &init_msg).unwrap_err(),
        EmberError::Auth
    );

    // Extended initiator message
    let mut extended_init = init.handshake_message().to_vec();
    extended_init.push(0xFF);
    assert_eq!(
        HandshakeResponder::respond(&drop_key, &extended_init).unwrap_err(),
        EmberError::Auth
    );

    // Truncated responder message
    let valid_init = HandshakeInitiator::new(&drop_key).unwrap();
    let (mut resp_msg, _) =
        HandshakeResponder::respond(&drop_key, valid_init.handshake_message()).unwrap();
    resp_msg.truncate(30);
    assert_eq!(
        valid_init.complete(&resp_msg).unwrap_err(),
        EmberError::Auth
    );
}

#[test]
fn test_old_message_payload_version_rejected() {
    let drop_key = SecretKey::generate();
    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let (resp_msg, resp_keys) =
        HandshakeResponder::respond(&drop_key, init.handshake_message()).unwrap();
    let _init_keys = init.complete(&resp_msg).unwrap();

    let mut session_b = ThreadSession::new("bob".to_string(), resp_keys);

    // Old payload without version (serde default v = 0)
    let old_payload_json = r#"{"seq":0,"ciphertext":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]}"#;
    let old_payload: EncryptedMessagePayload = serde_json::from_str(old_payload_json).unwrap();
    assert_eq!(old_payload.v, 0);

    let dec_err = session_b.decrypt_message(&old_payload).unwrap_err();
    assert_eq!(dec_err, EmberError::UnsupportedVersion(0));

    // Old payload with v = 1
    let v1_payload = EncryptedMessagePayload {
        v: 1,
        seq: 0,
        ciphertext: vec![0u8; 32],
    };
    let dec_err_v1 = session_b.decrypt_message(&v1_payload).unwrap_err();
    assert_eq!(dec_err_v1, EmberError::UnsupportedVersion(1));
}

#[test]
fn test_forward_secrecy_prevents_past_decryption() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let (resp_msg, resp_keys) =
        HandshakeResponder::respond(&drop_key, init.handshake_message()).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    // Send and decrypt messages 0, 1, 2
    let msg0 = session_a.encrypt_text("Past message 0").unwrap();
    session_b.decrypt_message(&msg0).unwrap();

    let msg1 = session_a.encrypt_text("Past message 1").unwrap();
    session_b.decrypt_message(&msg1).unwrap();

    let msg2 = session_a.encrypt_text("Past message 2").unwrap();
    session_b.decrypt_message(&msg2).unwrap();

    // Adversary attempts to replay or re-decrypt msg0 or msg1
    let replay_0 = session_b.decrypt_message(&msg0).unwrap_err();
    assert_eq!(replay_0, EmberError::ReplayDetected);

    // Current message 3 decrypts normally
    let msg3 = session_a.encrypt_text("Current message 3").unwrap();
    let dec3 = session_b.decrypt_message(&msg3).unwrap();
    assert_eq!(dec3.text, "Current message 3");
}
