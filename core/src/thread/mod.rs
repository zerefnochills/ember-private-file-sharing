//! Private 1:1 ephemeral threads module (Milestone M7).
//!
//! Provides:
//! - Local handles (`amber-fox-4821`).
//! - Ephemeral Diffie-Hellman handshake authenticated with drop PSK.
//! - Per-message forward-secret symmetric ratcheting.
//! - Replay and out-of-order rejection.
//! - Store-and-forward mailbox capabilities.

pub mod handle;
pub mod handshake;
pub mod ratchet;
pub mod session;

pub use handle::generate_handle;
pub use handshake::{
    derive_rendezvous_mailbox, derive_thread_psk, HandshakeInitiator, HandshakeResponder,
    HandshakeSessionKeys, MailboxCapability,
};
pub use ratchet::{EncryptedMessagePayload, ReceivingRatchet, SendingRatchet};
pub use session::{ThreadMessage, ThreadSession};
