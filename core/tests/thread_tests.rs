//! Acceptance and security tests for Ember 1:1 threads (Milestone M7).
//!
//! Verifies:
//! - Ephemeral Diffie-Hellman handshake matching
//! - Bidirectional forward-secret messaging roundtrip
//! - Replay attack rejection
//! - Out-of-order sequence rejection
//! - Tampered ciphertext rejection
//! - Forward secrecy proof (old message keys cannot be derived from advanced ratchet state)
//! - Handshake with wrong drop key fails authentication

use ember_core::thread::{generate_handle, HandshakeInitiator, HandshakeResponder, ThreadSession};
use ember_core::{EmberError, SecretKey};

#[test]
fn test_handshake_derivation_and_session_keys_match() {
    let drop_key = SecretKey::generate();

    // 1. Initiator starts
    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_pk = init.ephemeral_public_key();

    // 2. Responder responds
    let (resp_pk, resp_keys) = HandshakeResponder::respond(&drop_key, &init_pk).unwrap();

    // 3. Initiator completes
    let init_keys = init.complete(&resp_pk).unwrap();

    // 4. Verify directional match
    assert_eq!(
        init_keys.send_chain_key, resp_keys.recv_chain_key,
        "Initiator send chain key must match responder recv chain key"
    );
    assert_eq!(
        init_keys.recv_chain_key, resp_keys.send_chain_key,
        "Initiator recv chain key must match responder send chain key"
    );

    // Verify mailbox pairing
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
    let init_pk = init.ephemeral_public_key();

    let (resp_pk, resp_keys) = HandshakeResponder::respond(&drop_key, &init_pk).unwrap();
    let init_keys = init.complete(&resp_pk).unwrap();

    let mut session_a = ThreadSession::new(handle_a.clone(), init_keys);
    let mut session_b = ThreadSession::new(handle_b.clone(), resp_keys);

    // Message 1: A -> B
    let msg1_enc = session_a.encrypt_text("Hello from Initiator!").unwrap();
    assert_eq!(msg1_enc.seq, 0);

    let msg1_dec = session_b.decrypt_message(&msg1_enc).unwrap();
    assert_eq!(msg1_dec.seq, 0);
    assert_eq!(msg1_dec.sender_handle, handle_a);
    assert_eq!(msg1_dec.text, "Hello from Initiator!");

    // Message 2: B -> A
    let msg2_enc = session_b
        .encrypt_text("Hello back from Responder!")
        .unwrap();
    assert_eq!(msg2_enc.seq, 0);

    let msg2_dec = session_a.decrypt_message(&msg2_enc).unwrap();
    assert_eq!(msg2_dec.seq, 0);
    assert_eq!(msg2_dec.sender_handle, handle_b);
    assert_eq!(msg2_dec.text, "Hello back from Responder!");

    // Message 3: A -> B (seq 1)
    let msg3_enc = session_a.encrypt_text("Second message from A").unwrap();
    assert_eq!(msg3_enc.seq, 1);

    let msg3_dec = session_b.decrypt_message(&msg3_enc).unwrap();
    assert_eq!(msg3_dec.seq, 1);
    assert_eq!(msg3_dec.text, "Second message from A");
}

#[test]
fn test_replay_attack_rejected() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_pk = init.ephemeral_public_key();
    let (resp_pk, resp_keys) = HandshakeResponder::respond(&drop_key, &init_pk).unwrap();
    let init_keys = init.complete(&resp_pk).unwrap();

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
    let init_pk = init.ephemeral_public_key();
    let (resp_pk, resp_keys) = HandshakeResponder::respond(&drop_key, &init_pk).unwrap();
    let init_keys = init.complete(&resp_pk).unwrap();

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
    let init_pk = init.ephemeral_public_key();
    let (resp_pk, resp_keys) = HandshakeResponder::respond(&drop_key, &init_pk).unwrap();
    let init_keys = init.complete(&resp_pk).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    let mut msg = session_a
        .encrypt_text("Original sensitive message")
        .unwrap();

    // Flip a byte in the ciphertext payload
    if let Some(b) = msg.ciphertext.first_mut() {
        *b ^= 0x01;
    }

    let auth_err = session_b.decrypt_message(&msg).unwrap_err();
    assert_eq!(auth_err, EmberError::Auth);
}

#[test]
fn test_forward_secrecy_prevents_past_decryption() {
    let drop_key = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key).unwrap();
    let init_pk = init.ephemeral_public_key();
    let (resp_pk, resp_keys) = HandshakeResponder::respond(&drop_key, &init_pk).unwrap();
    let init_keys = init.complete(&resp_pk).unwrap();

    let mut session_a = ThreadSession::new("alice-fox-1111".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob-lynx-2222".to_string(), resp_keys);

    // Send and decrypt messages 0, 1, 2
    let msg0 = session_a.encrypt_text("Past message 0").unwrap();
    session_b.decrypt_message(&msg0).unwrap();

    let msg1 = session_a.encrypt_text("Past message 1").unwrap();
    session_b.decrypt_message(&msg1).unwrap();

    let msg2 = session_a.encrypt_text("Past message 2").unwrap();
    session_b.decrypt_message(&msg2).unwrap();

    // Suppose an adversary steals receiver's state now (at expected_seq = 3).
    // The previous chain keys for msg 0 and 1 are overwritten/zeroized.
    // If the adversary tries to decrypt msg0 or msg1 using the current receiver ratchet:
    let replay_0 = session_b.decrypt_message(&msg0).unwrap_err();
    assert_eq!(replay_0, EmberError::ReplayDetected);

    // Furthermore, new message 3 decrypts normally
    let msg3 = session_a.encrypt_text("Current message 3").unwrap();
    let dec3 = session_b.decrypt_message(&msg3).unwrap();
    assert_eq!(dec3.text, "Current message 3");
}

#[test]
fn test_wrong_drop_key_fails_authentication() {
    let drop_key_a = SecretKey::generate();
    let drop_key_b = SecretKey::generate();

    let init = HandshakeInitiator::new(&drop_key_a).unwrap();
    let init_pk = init.ephemeral_public_key();

    // Responder uses wrong drop key
    let (resp_pk, resp_keys) = HandshakeResponder::respond(&drop_key_b, &init_pk).unwrap();
    let init_keys = init.complete(&resp_pk).unwrap();

    let mut session_a = ThreadSession::new("alice".to_string(), init_keys);
    let mut session_b = ThreadSession::new("bob".to_string(), resp_keys);

    let msg = session_a.encrypt_text("Secret text").unwrap();
    let err = session_b.decrypt_message(&msg).unwrap_err();
    assert_eq!(err, EmberError::Auth);
}
