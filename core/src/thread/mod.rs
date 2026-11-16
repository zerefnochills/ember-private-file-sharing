//! Private 1:1 ephemeral threads module (Milestone M7).
//!
//! Provides:
//! - Local handles (`amber-fox-4821`).
//! - Ephemeral Diffie-Hellman handshake authenticated with drop PSK.
//! - Session-level forward-secret symmetric transport.
//! - Replay and out-of-order rejection.
//! - Store-and-forward mailbox capabilities.

pub mod handle;
pub mod handshake;
pub mod session;
pub mod transport;

pub use handle::{generate_handle, is_valid_handle};
pub use handshake::{
    derive_rendezvous_mailbox, derive_thread_psk, HandshakeInitiator, HandshakeResponder,
    HandshakeSessionKeys, MailboxCapability,
};
pub use session::{ThreadMessage, ThreadSession};
pub use transport::{EncryptedMessagePayload, ThreadReceiver, ThreadSender};
