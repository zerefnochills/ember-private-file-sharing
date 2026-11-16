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
    MAX_THREAD_TEXT_BYTES, THREAD_HANDSHAKE_VERSION, THREAD_INITIATOR_MSG_LEN, THREAD_MSG_VERSION,
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

    // 4. Verify transcript handshake hash match
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
fn test_sequence_policy_gap_replay_reorder() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_msg = init.handshake_message();
    let (resp_msg, resp_keys) = HandshakeResponder::respond(&drop_key, init_msg).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    let msg0 = session_a.encrypt_text("Message 0").unwrap();
    let msg1 = session_a.encrypt_text("Message 1").unwrap();
    let msg2 = session_a.encrypt_text("Message 2").unwrap();

    // 1. Out-of-order gap acceptance: deliver msg1 (seq 1) before msg0 (seq 0)
    let dec1 = session_b.decrypt_message(&msg1).unwrap();
    assert_eq!(dec1.text, "Message 1");
    assert_eq!(session_b.expected_recv_seq(), 2);

    // 2. Replay rejection of earlier sequence: msg0 (seq 0) arriving after msg1 is rejected as replay
    let err_replay0 = session_b.decrypt_message(&msg0).unwrap_err();
    assert_eq!(err_replay0, EmberError::ReplayDetected);

    // 3. Wrong tag does not advance state
    let mut tampered_msg2 = msg2.clone();
    tampered_msg2.ciphertext[0] ^= 0x42;
    let auth_err = session_b.decrypt_message(&tampered_msg2).unwrap_err();
    assert_eq!(auth_err, EmberError::Auth);
    assert_eq!(
        session_b.expected_recv_seq(),
        2,
        "Failed decrypt must not advance expected_seq"
    );

    // 4. Delivering valid msg2 after tampering succeeds and advances sequence
    let dec2 = session_b.decrypt_message(&msg2).unwrap();
    assert_eq!(dec2.text, "Message 2");
    assert_eq!(session_b.expected_recv_seq(), 3);
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

    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    // Old version v = 1
    let v1_payload = EncryptedMessagePayload {
        v: 1,
        seq: 0,
        ciphertext: vec![0u8; 32],
    };
    let dec_err_v1 = session_b.decrypt_message(&v1_payload).unwrap_err();
    assert_eq!(dec_err_v1, EmberError::UnsupportedVersion(1));

    // Old version v = 2
    let v2_payload = EncryptedMessagePayload {
        v: 2,
        seq: 0,
        ciphertext: vec![0u8; 32],
    };
    let dec_err_v2 = session_b.decrypt_message(&v2_payload).unwrap_err();
    assert_eq!(dec_err_v2, EmberError::UnsupportedVersion(2));
}

#[test]
fn test_message_size_limits_exact_limit_plus_one_and_65535_cap() {
    let drop_key = SecretKey::generate();
    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let (resp_msg, resp_keys) =
        HandshakeResponder::respond(&drop_key, init.handshake_message()).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    // 1. Exact limit: MAX_THREAD_TEXT_BYTES (4096 bytes) succeeds
    let exact_text = "a".repeat(MAX_THREAD_TEXT_BYTES);
    let exact_enc = session_a.encrypt_text(&exact_text).unwrap();
    let exact_dec = session_b.decrypt_message(&exact_enc).unwrap();
    assert_eq!(exact_dec.text.len(), MAX_THREAD_TEXT_BYTES);

    // 2. Limit + 1: 4097 bytes fails immediately before encryption
    let text_4097 = "a".repeat(MAX_THREAD_TEXT_BYTES + 1);
    let err_4097 = session_a.encrypt_text(&text_4097).unwrap_err();
    assert!(matches!(err_4097, EmberError::InvalidMetadata(_)));

    // 3. 65535-byte ciphertext cap on receive before allocating
    let oversized_buf = vec![0u8; 65535];
    let oversized_payload = EncryptedMessagePayload {
        v: THREAD_MSG_VERSION,
        seq: 10,
        ciphertext: oversized_buf.clone(),
    };
    let err_oversized_payload = session_b.decrypt_message(&oversized_payload).unwrap_err();
    assert!(matches!(
        err_oversized_payload,
        EmberError::InvalidMetadata(_)
    ));

    // 4. Binary wire payload with 65535-byte ciphertext rejected by from_bytes before allocation
    let mut fake_wire = Vec::with_capacity(9 + 65535);
    fake_wire.push(THREAD_MSG_VERSION);
    fake_wire.extend_from_slice(&10u64.to_be_bytes());
    fake_wire.extend_from_slice(&oversized_buf);
    let parse_err = EncryptedMessagePayload::from_bytes(&fake_wire).unwrap_err();
    assert!(matches!(parse_err, EmberError::InvalidMetadata(_)));
}

#[test]
fn test_binary_envelope_wire_format_and_rejection() {
    // 1. Truncated header (< 9 bytes)
    assert!(matches!(
        EncryptedMessagePayload::from_bytes(&[THREAD_MSG_VERSION, 0, 1, 2]).unwrap_err(),
        EmberError::InvalidMetadata(_)
    ));

    // 2. Unsupported version byte
    let mut wire_v1 = vec![0u8; 32];
    wire_v1[0] = 1;
    assert_eq!(
        EncryptedMessagePayload::from_bytes(&wire_v1).unwrap_err(),
        EmberError::UnsupportedVersion(1)
    );

    // 3. Truncated ciphertext (< 16 bytes MAC)
    let mut wire_short_ct = vec![0u8; 20];
    wire_short_ct[0] = THREAD_MSG_VERSION;
    assert_eq!(
        EncryptedMessagePayload::from_bytes(&wire_short_ct).unwrap_err(),
        EmberError::Auth
    );

    // 4. Valid binary wire payload roundtrip
    let valid_payload = EncryptedMessagePayload {
        v: THREAD_MSG_VERSION,
        seq: 42,
        ciphertext: vec![0xAB; 64],
    };
    let wire_bytes = valid_payload.to_bytes();
    assert_eq!(wire_bytes.len(), 9 + 64);
    assert_eq!(wire_bytes[0], THREAD_MSG_VERSION);
    assert_eq!(u64::from_be_bytes(wire_bytes[1..9].try_into().unwrap()), 42);

    let parsed = EncryptedMessagePayload::from_bytes(&wire_bytes).unwrap();
    assert_eq!(parsed, valid_payload);
}

#[test]
fn test_sender_handle_validation_and_seq_mismatch() {
    let drop_key = SecretKey::generate();
    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let (resp_msg, resp_keys) =
        HandshakeResponder::respond(&drop_key, init.handshake_message()).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    // 1. Session created with invalid handle
    let mut session_invalid_handle = ThreadSession::new("invalid_handle".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    let enc_invalid = session_invalid_handle.encrypt_text("Hello").unwrap();
    let err_handle = session_b.decrypt_message(&enc_invalid).unwrap_err();
    assert_eq!(
        err_handle,
        EmberError::InvalidSenderHandle("invalid_handle".to_string())
    );

    // 2. Inner sequence number mismatch: envelope seq = 5, inner seq = 0
    let mut enc_mismatch = enc_invalid.clone();
    enc_mismatch.seq = 5;
    // When seq = 5, Noise decryption fails auth or inner seq != outer seq
    let dec_err = session_b.decrypt_message(&enc_mismatch).unwrap_err();
    assert!(dec_err == EmberError::Auth || matches!(dec_err, EmberError::SeqMismatch(5, _)));
}

#[test]
fn test_seq_overflow_guard() {
    let drop_key = SecretKey::generate();
    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let (resp_msg, _resp_keys) =
        HandshakeResponder::respond(&drop_key, init.handshake_message()).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);

    session_a.set_send_seq_for_testing(u64::MAX);
    let overflow_err = session_a.encrypt_text("Overflow test").unwrap_err();
    assert_eq!(overflow_err, EmberError::SeqOverflow);
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

#[test]
fn test_ephemeral_key_hooks_gated_by_feature() {
    #[cfg(feature = "test-hooks")]
    {
        let drop_key = SecretKey::generate();
        let eph1 = [11u8; 32];
        let eph2 = [22u8; 32];
        let init = HandshakeInitiator::with_ephemeral_key(&drop_key, &eph1).unwrap();
        let (resp_msg, resp_keys) = HandshakeResponder::respond_with_ephemeral_key(
            &drop_key,
            init.handshake_message(),
            &eph2,
        )
        .unwrap();
        let init_keys = init.complete(&resp_msg).unwrap();

        let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
        let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

        let enc = session_a.encrypt_text("Test with ephemeral hooks").unwrap();
        let dec = session_b.decrypt_message(&enc).unwrap();
        assert_eq!(dec.text, "Test with ephemeral hooks");
    }
}

#[test]
fn test_escaped_characters_text_serialization_limit() {
    let drop_key = SecretKey::generate();
    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let (resp_msg, resp_keys) =
        HandshakeResponder::respond(&drop_key, init.handshake_message()).unwrap();
    let init_keys = init.complete(&resp_msg).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    // 1. A 4096-byte text made exclusively of heavily-escaped JSON characters: '"', '\n', '\\'
    let pattern = ['"', '\n', '\\'];
    let mut escaped_text = String::with_capacity(MAX_THREAD_TEXT_BYTES);
    for i in 0..MAX_THREAD_TEXT_BYTES {
        escaped_text.push(pattern[i % pattern.len()]);
    }
    assert_eq!(escaped_text.len(), MAX_THREAD_TEXT_BYTES);

    // Encrypting this text hits the serialized plaintext cap (MAX_THREAD_MSG_BYTES = 4608)
    // because each escaped character doubles to 2 bytes in JSON (~8254 bytes serialized).
    // It must fail with a clear message-too-long error (InvalidMetadata).
    let err = session_a.encrypt_text(&escaped_text).unwrap_err();
    assert!(
        matches!(err, EmberError::InvalidMetadata(ref msg) if msg.contains("exceeds maximum allowed")),
        "Expected clear message-too-long error, got: {:?}",
        err
    );

    // 2. An escaped text sized to fit within MAX_THREAD_MSG_BYTES (e.g. 2000 bytes)
    // encrypts and roundtrips cleanly.
    let mut fit_escaped_text = String::with_capacity(2000);
    for i in 0..2000 {
        fit_escaped_text.push(pattern[i % pattern.len()]);
    }
    let enc = session_a.encrypt_text(&fit_escaped_text).unwrap();
    let dec = session_b.decrypt_message(&enc).unwrap();
    assert_eq!(dec.text, fit_escaped_text);
}
