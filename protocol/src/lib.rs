//! Ember wire types and protocol constants shared by core, server, and clients.
//!
//! Follows `docs/protocol-spec.md` (version 1 drop format).

/// Maximum lifetime of any drop or thread, in seconds (12 hours).
pub const MAX_LIFETIME_SECS: u64 = 12 * 60 * 60;

/// Plaintext content chunk size (1 MiB = 1,048,576 bytes).
pub const CHUNK_SIZE: usize = 1024 * 1024;

/// Plaintext metadata record size (exactly 4096 bytes).
pub const METADATA_PLAIN_LEN: usize = 4096;

/// Secretstream stream header size in bytes.
pub const STREAM_HEADER_BYTES: usize = 24;

/// Secretstream authentication overhead per encrypted message (1 tag byte + 16 MAC bytes).
pub const ABYTES: usize = 17;

/// Exact size of transport Part 0 (stream header + message 0 ciphertext = 24 + 4096 + 17 = 4137 bytes).
pub const PART0_SIZE: usize = STREAM_HEADER_BYTES + METADATA_PLAIN_LEN + ABYTES;

/// Maximum size of any content transport Part (chunk_len + ABYTES = 1,048,576 + 17 = 1,048,593 bytes).
pub const MAX_PART_SIZE: usize = CHUNK_SIZE + ABYTES;

/// Default maximum total ciphertext bytes (50 MiB content padded + headers = 52,433,787 bytes).
pub const DEFAULT_MAX_CIPHERTEXT_BYTES: u64 = 52_433_787;

/// Minimum content padding length in bytes (4096 bytes).
pub const MIN_PADDING_BYTES: u64 = 4096;

/// Content padding granularity (64 KiB = 65,536 bytes).
pub const PADDING_BAND_BYTES: u64 = 65_536;

/// Sub-key derivation context (exactly 8 ASCII bytes).
pub const KDF_CONTEXT: [u8; 8] = *b"EMBERv1_";

/// Additional authenticated data for every secretstream drop message.
pub const AD_DROP: &[u8] = b"ember-drop-v1";

/// Sub-key identifier for drop content encryption.
pub const SUBKEY_ID_CONTENT: u64 = 1;

/// Sub-key identifier reserved for thread pre-shared key (milestone M7).
pub const SUBKEY_ID_THREAD_PSK: u64 = 3;

/// Sub-key identifier for thread sending chain key (initiator to responder).
pub const SUBKEY_ID_THREAD_CHAIN_INIT: u64 = 4;

/// Sub-key identifier for thread receiving chain key (responder to initiator).
pub const SUBKEY_ID_THREAD_CHAIN_RESP: u64 = 5;

/// Sub-key identifier for thread mailbox capability tokens derivation.
pub const SUBKEY_ID_THREAD_MAILBOX: u64 = 6;

/// Sub-key identifier for thread per-message key derivation.
pub const SUBKEY_ID_THREAD_MSG: u64 = 7;

/// Sub-key identifier for thread rendezvous mailbox derivation.
pub const SUBKEY_ID_THREAD_RENDEZVOUS: u64 = 8;

/// Maximum payload size for an encrypted thread message (64 KiB).
pub const MAX_THREAD_MSG_BYTES: usize = 65_536;

/// Maximum number of messages stored in a single mailbox queue.
pub const MAX_MAILBOX_MESSAGES: usize = 100;

/// Additional authenticated data for thread messages.
pub const AD_THREAD: &[u8] = b"ember-thread-v1";

/// Link format version byte (version 1).
pub const LINK_VERSION_1: u8 = 0x01;

/// Link flag bit 0: passphrase protected.
pub const FLAG_PASSPHRASE_PROTECTED: u8 = 0x01;

/// Default Argon2id passes for passphrase wrapping.
pub const DEFAULT_ARGON2_OPS: u8 = 3;

/// Default Argon2id memory parameter for passphrase wrapping (64 MiB in KiB).
pub const DEFAULT_ARGON2_MEM_KIB: u32 = 65_536;

/// Minimum permitted Argon2id passes.
pub const MIN_ARGON2_OPS: u8 = 1;

/// Maximum permitted Argon2id passes (DoS guard).
pub const MAX_ARGON2_OPS: u8 = 10;

/// Minimum permitted Argon2id memory (8 MiB in KiB).
pub const MIN_ARGON2_MEM_KIB: u32 = 8192;

/// Maximum permitted Argon2id memory (256 MiB in KiB, DoS guard).
pub const MAX_ARGON2_MEM_KIB: u32 = 262_144;

/// Byte length of server-generated blob identifiers (16 random bytes).
pub const BLOB_ID_BYTES: usize = 16;

/// Byte length of sender deletion tokens (32 random bytes).
pub const DELETE_TOKEN_BYTES: usize = 32;

/// Length of unencrypted link payload (version + flags + 32-byte K = 34 bytes).
pub const LINK_PAYLOAD_UNWRAPPED_LEN: usize = 34;

/// Length of passphrase-wrapped link payload:
/// 1 (version) + 1 (flags) + 1 (ops) + 4 (mem_kib) + 16 (salt) + 24 (nonce) + 48 (wrapped_K) = 95 bytes.
pub const LINK_PAYLOAD_WRAPPED_LEN: usize = 95;

/// Additional data length for key wrapping (first 23 bytes of payload).
pub const AD_WRAP_PREFIX_LEN: usize = 23;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_lifetime_is_twelve_hours() {
        assert_eq!(MAX_LIFETIME_SECS, 43_200);
    }

    #[test]
    fn test_sizes_match_spec() {
        assert_eq!(PART0_SIZE, 4137);
        assert_eq!(MAX_PART_SIZE, 1_048_593);
        assert_eq!(LINK_PAYLOAD_UNWRAPPED_LEN, 34);
        assert_eq!(LINK_PAYLOAD_WRAPPED_LEN, 95);
        assert_eq!(AD_WRAP_PREFIX_LEN, 23);
        assert_eq!(KDF_CONTEXT.len(), 8);
        assert_eq!(AD_DROP, b"ember-drop-v1");
    }
}
