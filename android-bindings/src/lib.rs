use std::sync::{Arc, Mutex};

use ember_core::thread::derive_rendezvous_mailbox;
use ember_core::{
    decrypt_drop, derive_thread_psk, encrypt_drop, generate_handle, is_valid_handle,
    start_decrypt_stream, start_encrypt_stream, DropDecryptStream, DropEncryptStream, DropMetadata,
    EncryptedMessagePayload, HandshakeInitiator, HandshakeResponder, HandshakeSessionKeys,
    LinkPayload, MailboxCapability, SecretKey, StripStatus, ThreadMessage, ThreadSession,
};
use zeroize::Zeroize;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("The encrypted data is invalid or could not be authenticated")]
    InvalidCiphertext,
    #[error("The encrypted data is not a text drop")]
    NotText,
    #[error("The text drop is not valid UTF-8")]
    InvalidText,
    #[error("The requested expiry is invalid")]
    InvalidExpiry,
    #[error("The provided secret key must be exactly 32 bytes")]
    InvalidKey,
    #[error("The link is invalid or uses an unsupported version")]
    InvalidLink,
    #[error("A passphrase is required for this link")]
    MissingPassphrase,
    #[error("A passphrase was supplied for an unprotected link")]
    UnexpectedPassphrase,
    #[error("The passphrase could not unlock this link")]
    InvalidPassphrase,
    #[error("The proof-of-work challenge could not be solved within the work limit")]
    PowFailed,
    #[error("The proof-of-work difficulty is outside the supported range")]
    InvalidPowDifficulty,
    #[error("Metadata stripping is limited to 8 MiB to preserve bounded memory")]
    StripInputTooLarge,
    #[error("The thread session has ended")]
    ThreadClosed,
    #[error("The serialized message is too long; escaped characters count toward the limit")]
    ThreadMessageTooLong,
    #[error("This thread message was already received")]
    ThreadReplay,
    #[error("The thread message or handshake is invalid")]
    InvalidThreadMessage,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DecryptedDrop {
    pub kind: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub burn: bool,
    pub created_at: u64,
    pub expires_at: u64,
    pub content: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DropMetadataInfo {
    pub kind: String,
    pub name: String,
    pub safe_name: String,
    pub mime: String,
    pub size: u64,
    pub burn: bool,
    pub created_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DecryptedChunk {
    pub plaintext: Vec<u8>,
    pub is_final: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MetadataStripReport {
    pub status: String,
    pub format: String,
    pub fields_removed: Vec<String>,
    pub reason: String,
    pub modified: bool,
    pub content: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DropLinkInfo {
    pub host: String,
    pub blob_id: String,
    pub passphrase_protected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ThreadMessageInfo {
    pub sequence: u64,
    pub timestamp: u64,
    pub sender_handle: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ThreadEnvelopeInfo {
    pub version: u8,
    pub sequence: u64,
    pub ciphertext: Vec<u8>,
}

#[derive(uniffi::Record)]
pub struct ThreadResponderHandshake {
    pub response_message: Vec<u8>,
    pub session: Arc<EmberThreadSession>,
}

/// A key handle that keeps secret key bytes in Rust and zeroizes them on drop.
#[derive(uniffi::Object)]
pub struct EmberCrypto {
    key: SecretKey,
}

/// Rust-owned incremental encryptor. Payload chunks are capped by the shared core engine.
#[derive(uniffi::Object)]
pub struct EmberEncryptStream {
    stream: Mutex<DropEncryptStream>,
    part0: Mutex<Option<Vec<u8>>>,
}

/// Rust-owned incremental decryptor. Callers must keep output private until `finish` succeeds.
#[derive(uniffi::Object)]
pub struct EmberDecryptStream {
    stream: Mutex<DropDecryptStream>,
    metadata: DropMetadataInfo,
}

/// An opaque relay mailbox capability. Access tokens are exposed only for the immediate
/// HTTPS request; key agreement and transport state never leave Rust.
#[derive(uniffi::Object)]
pub struct EmberThreadMailbox {
    capability: Mutex<Option<MailboxCapability>>,
}

/// An in-progress initiator handshake. Dropping it wipes the PSK and handshake message.
#[derive(uniffi::Object)]
pub struct EmberThreadInitiator {
    state: Mutex<Option<HandshakeInitiator>>,
}

/// An active session. It is deliberately not serializable; closing it drops the Noise state.
#[derive(uniffi::Object)]
pub struct EmberThreadSession {
    state: Mutex<Option<ThreadSession>>,
    mailbox_out: Arc<EmberThreadMailbox>,
    mailbox_in: Arc<EmberThreadMailbox>,
}

#[uniffi::export]
impl EmberEncryptStream {
    #[uniffi::constructor]
    pub fn new(
        crypto: Arc<EmberCrypto>,
        name: String,
        mime: String,
        size: u64,
        burn: bool,
        created_at: u64,
        expires_at: u64,
    ) -> Result<Arc<Self>, FfiError> {
        if expires_at <= created_at {
            return Err(FfiError::InvalidExpiry);
        }
        let metadata = DropMetadata::new_file(name, mime, size, burn, created_at, expires_at);
        let (stream, part0) = start_encrypt_stream(&crypto.key, &metadata)
            .map_err(|_| FfiError::InvalidCiphertext)?;
        Ok(Arc::new(Self {
            stream: Mutex::new(stream),
            part0: Mutex::new(Some(part0)),
        }))
    }

    /// Returns encrypted Message 0 exactly once.
    pub fn take_part0(&self) -> Result<Vec<u8>, FfiError> {
        self.part0
            .lock()
            .map_err(|_| FfiError::InvalidCiphertext)?
            .take()
            .ok_or(FfiError::InvalidCiphertext)
    }

    /// Encrypts one caller-provided chunk and wipes the input buffer before returning.
    pub fn update(&self, mut plaintext: Vec<u8>) -> Result<Vec<Vec<u8>>, FfiError> {
        let result = match self.stream.lock() {
            Ok(mut stream) => stream
                .update(&plaintext)
                .map_err(|_| FfiError::InvalidCiphertext),
            Err(_) => Err(FfiError::InvalidCiphertext),
        };
        plaintext.zeroize();
        result
    }

    pub fn finish(&self) -> Result<Vec<Vec<u8>>, FfiError> {
        self.stream
            .lock()
            .map_err(|_| FfiError::InvalidCiphertext)?
            .finish()
            .map_err(|_| FfiError::InvalidCiphertext)
    }
}

#[uniffi::export]
impl EmberDecryptStream {
    #[uniffi::constructor]
    pub fn new(crypto: Arc<EmberCrypto>, mut part0: Vec<u8>) -> Result<Arc<Self>, FfiError> {
        let result = start_decrypt_stream(&crypto.key, &part0);
        part0.zeroize();
        let (stream, metadata) = result.map_err(|_| FfiError::InvalidCiphertext)?;
        Ok(Arc::new(Self {
            stream: Mutex::new(stream),
            metadata: metadata_info(metadata),
        }))
    }

    pub fn metadata(&self) -> DropMetadataInfo {
        self.metadata.clone()
    }

    /// Authenticates one relay part and returns plaintext for an app-private temporary file.
    pub fn push_part(&self, mut ciphertext_part: Vec<u8>) -> Result<DecryptedChunk, FfiError> {
        let result = match self.stream.lock() {
            Ok(mut stream) => stream
                .push_part(&ciphertext_part)
                .map(|(plaintext, is_final)| DecryptedChunk {
                    plaintext,
                    is_final,
                })
                .map_err(|_| FfiError::InvalidCiphertext),
            Err(_) => Err(FfiError::InvalidCiphertext),
        };
        ciphertext_part.zeroize();
        result
    }

    pub fn finish(&self) -> Result<(), FfiError> {
        self.stream
            .lock()
            .map_err(|_| FfiError::InvalidCiphertext)?
            .finish()
            .map_err(|_| FfiError::InvalidCiphertext)
    }
}

fn metadata_info(metadata: DropMetadata) -> DropMetadataInfo {
    let safe_name = metadata.sanitized_file_name();
    DropMetadataInfo {
        kind: metadata.kind,
        name: metadata.name,
        safe_name,
        mime: metadata.mime,
        size: metadata.size,
        burn: metadata.burn,
        created_at: metadata.created,
        expires_at: metadata.expires,
    }
}

#[uniffi::export]
impl EmberCrypto {
    #[uniffi::constructor]
    pub fn generate() -> Arc<Self> {
        Arc::new(Self {
            key: SecretKey::generate(),
        })
    }

    /// Creates a key handle from existing 32-byte secret key material.
    #[uniffi::constructor]
    pub fn from_key_bytes(mut bytes: Vec<u8>) -> Result<Arc<Self>, FfiError> {
        if bytes.len() != 32 {
            bytes.zeroize();
            return Err(FfiError::InvalidKey);
        }
        let mut key_arr = [0u8; 32];
        key_arr.copy_from_slice(&bytes);
        bytes.zeroize();
        Ok(Arc::new(Self {
            key: SecretKey::from_bytes(key_arr),
        }))
    }

    /// Encrypts a UTF-8 text drop in the same format used by the shared Rust core.
    pub fn encrypt_text(
        &self,
        text: String,
        created_at: u64,
        expires_at: u64,
    ) -> Result<Vec<u8>, FfiError> {
        self.encrypt_text_with_options(text, false, created_at, expires_at)
    }

    /// Encrypts text with an authenticated burn-after-view intent in metadata.
    pub fn encrypt_text_with_options(
        &self,
        mut text: String,
        burn: bool,
        created_at: u64,
        expires_at: u64,
    ) -> Result<Vec<u8>, FfiError> {
        if expires_at <= created_at {
            text.zeroize();
            return Err(FfiError::InvalidExpiry);
        }

        let metadata = DropMetadata::new_text(text.len() as u64, burn, created_at, expires_at);
        let result = encrypt_drop(&self.key, &metadata, text.as_bytes())
            .map_err(|_| FfiError::InvalidCiphertext);
        text.zeroize();
        result
    }

    /// Decrypts a text drop and rejects file drops instead of silently decoding them.
    pub fn decrypt_text(&self, ciphertext: Vec<u8>) -> Result<String, FfiError> {
        let (metadata, mut plaintext) =
            decrypt_drop(&self.key, &ciphertext).map_err(|_| FfiError::InvalidCiphertext)?;
        if metadata.kind != "text" {
            plaintext.zeroize();
            return Err(FfiError::NotText);
        }
        match String::from_utf8(plaintext) {
            Ok(text) => Ok(text),
            Err(error) => {
                let mut bytes = error.into_bytes();
                bytes.zeroize();
                Err(FfiError::InvalidText)
            }
        }
    }

    /// Encrypts a file drop payload.
    pub fn encrypt_file(
        &self,
        name: String,
        mime: String,
        mut content: Vec<u8>,
        burn: bool,
        created_at: u64,
        expires_at: u64,
    ) -> Result<Vec<u8>, FfiError> {
        if expires_at <= created_at {
            content.zeroize();
            return Err(FfiError::InvalidExpiry);
        }

        let size = content.len() as u64;
        let metadata = DropMetadata::new_file(name, mime, size, burn, created_at, expires_at);
        let result =
            encrypt_drop(&self.key, &metadata, &content).map_err(|_| FfiError::InvalidCiphertext);
        content.zeroize();
        result
    }

    /// Decrypts a full drop payload returning its metadata and plaintext content.
    pub fn decrypt_drop_payload(&self, ciphertext: Vec<u8>) -> Result<DecryptedDrop, FfiError> {
        let (metadata, plaintext) =
            decrypt_drop(&self.key, &ciphertext).map_err(|_| FfiError::InvalidCiphertext)?;
        Ok(DecryptedDrop {
            kind: metadata.kind,
            name: metadata.name,
            mime: metadata.mime,
            size: metadata.size,
            burn: metadata.burn,
            created_at: metadata.created,
            expires_at: metadata.expires,
            content: plaintext,
        })
    }

    /// Builds an unprotected v1 link without returning the link key to Kotlin.
    pub fn build_link(&self, host: String, blob_id: String) -> Result<String, FfiError> {
        ember_core::build_link(&host, &blob_id, &LinkPayload::Unprotected(self.key.clone()))
            .map_err(|_| FfiError::InvalidLink)
    }

    /// Wraps this object's key in Rust and builds a passphrase-protected v1 link.
    pub fn build_protected_link(
        &self,
        host: String,
        blob_id: String,
        mut passphrase: String,
    ) -> Result<String, FfiError> {
        let wrapped_result = ember_core::wrap_key(&self.key, &passphrase, None);
        passphrase.zeroize();
        let mut payload =
            LinkPayload::Protected(wrapped_result.map_err(|_| FfiError::InvalidPassphrase)?);
        let result = ember_core::build_link(&host, &blob_id, &payload);
        zeroize_link_payload(&mut payload);
        result.map_err(|_| FfiError::InvalidLink)
    }

    /// Parses a link and creates a Rust-owned key handle; link and passphrase buffers are wiped.
    #[uniffi::constructor]
    pub fn from_link(
        mut link: String,
        mut passphrase: Option<String>,
    ) -> Result<std::sync::Arc<Self>, FfiError> {
        let parsed_result = ember_core::parse_link(&link);
        link.zeroize();
        let parsed = match parsed_result {
            Ok(parsed) => parsed,
            Err(_) => {
                zeroize_passphrase(&mut passphrase);
                return Err(FfiError::InvalidLink);
            }
        };

        let key = match parsed.payload {
            LinkPayload::Unprotected(key) => {
                if passphrase.is_some() {
                    zeroize_passphrase(&mut passphrase);
                    return Err(FfiError::UnexpectedPassphrase);
                }
                key
            }
            LinkPayload::Protected(mut wrapped) => {
                let Some(mut phrase) = passphrase.take() else {
                    wrapped.zeroize();
                    return Err(FfiError::MissingPassphrase);
                };
                let key_result = ember_core::unwrap_key(&wrapped, &phrase);
                phrase.zeroize();
                wrapped.zeroize();
                key_result.map_err(|_| FfiError::InvalidPassphrase)?
            }
        };

        Ok(std::sync::Arc::new(Self { key }))
    }

    /// Derives the rendezvous mailbox in Rust and returns an opaque capability handle.
    pub fn derive_thread_rendezvous_mailbox(&self) -> Result<Arc<EmberThreadMailbox>, FfiError> {
        let capability = derive_rendezvous_mailbox(&self.key).map_err(map_thread_error)?;
        Ok(Arc::new(EmberThreadMailbox {
            capability: Mutex::new(Some(capability)),
        }))
    }

    /// Checks a test-vector PSK inside Rust without returning the derived key to Kotlin.
    pub fn matches_thread_psk(&self, mut expected: Vec<u8>) -> Result<bool, FfiError> {
        if expected.len() != 32 {
            expected.zeroize();
            return Ok(false);
        }
        let mut actual = derive_thread_psk(&self.key).map_err(map_thread_error)?;
        let difference = actual
            .iter()
            .zip(expected.iter())
            .fold(0u8, |difference, (left, right)| difference | (left ^ right));
        actual.zeroize();
        expected.zeroize();
        Ok(difference == 0)
    }

    /// Starts an initiator handshake without exporting the thread PSK.
    pub fn start_thread_initiator(&self) -> Result<Arc<EmberThreadInitiator>, FfiError> {
        let state = HandshakeInitiator::new(&self.key).map_err(map_thread_error)?;
        Ok(Arc::new(EmberThreadInitiator {
            state: Mutex::new(Some(state)),
        }))
    }

    /// Responds to the first accepted rendezvous handshake message.
    pub fn respond_to_thread(
        &self,
        mut initiator_message: Vec<u8>,
    ) -> Result<ThreadResponderHandshake, FfiError> {
        let result = HandshakeResponder::respond(&self.key, &initiator_message);
        initiator_message.zeroize();
        let (response_message, keys) = result.map_err(map_thread_error)?;
        Ok(ThreadResponderHandshake {
            response_message,
            session: EmberThreadSession::from_keys(keys),
        })
    }
}

#[uniffi::export]
impl EmberThreadMailbox {
    /// Mailbox IDs are public routing values; push and pull tokens stay in Rust until requested
    /// for the corresponding HTTPS Authorization header.
    pub fn id(&self) -> Result<String, FfiError> {
        self.capability
            .lock()
            .map_err(|_| FfiError::ThreadClosed)?
            .as_ref()
            .map(|capability| capability.id.clone())
            .ok_or(FfiError::ThreadClosed)
    }

    pub fn push_token(&self) -> Result<String, FfiError> {
        self.capability
            .lock()
            .map_err(|_| FfiError::ThreadClosed)?
            .as_ref()
            .map(|capability| capability.push_token.clone())
            .ok_or(FfiError::ThreadClosed)
    }

    pub fn pull_token(&self) -> Result<String, FfiError> {
        self.capability
            .lock()
            .map_err(|_| FfiError::ThreadClosed)?
            .as_ref()
            .map(|capability| capability.pull_token.clone())
            .ok_or(FfiError::ThreadClosed)
    }

    pub fn push_token_hash(&self) -> Result<Vec<u8>, FfiError> {
        self.capability
            .lock()
            .map_err(|_| FfiError::ThreadClosed)?
            .as_ref()
            .map(|capability| capability.push_token_hash().to_vec())
            .ok_or(FfiError::ThreadClosed)
    }

    pub fn pull_token_hash(&self) -> Result<Vec<u8>, FfiError> {
        self.capability
            .lock()
            .map_err(|_| FfiError::ThreadClosed)?
            .as_ref()
            .map(|capability| capability.pull_token_hash().to_vec())
            .ok_or(FfiError::ThreadClosed)
    }

    /// Drops the local capability and wipes its bearer tokens.
    pub fn end_session(&self) {
        wipe_mailbox_handle(&self.capability);
    }
}

#[uniffi::export]
impl EmberThreadInitiator {
    pub fn handshake_message(&self) -> Result<Vec<u8>, FfiError> {
        self.state
            .lock()
            .map_err(|_| FfiError::ThreadClosed)?
            .as_ref()
            .map(|state| state.handshake_message().to_vec())
            .ok_or(FfiError::ThreadClosed)
    }

    pub fn complete(
        &self,
        mut responder_message: Vec<u8>,
    ) -> Result<Arc<EmberThreadSession>, FfiError> {
        let result = (|| {
            let initiator = self
                .state
                .lock()
                .map_err(|_| FfiError::ThreadClosed)?
                .take()
                .ok_or(FfiError::ThreadClosed)?;
            let keys = initiator
                .complete(&responder_message)
                .map_err(map_thread_error)?;
            Ok(EmberThreadSession::from_keys(keys))
        })();
        responder_message.zeroize();
        result
    }

    pub fn end_session(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.take();
        }
    }
}

#[uniffi::export]
impl EmberThreadSession {
    pub fn handle(&self) -> Result<String, FfiError> {
        self.state
            .lock()
            .map_err(|_| FfiError::ThreadClosed)?
            .as_ref()
            .map(|session| session.handle.clone())
            .ok_or(FfiError::ThreadClosed)
    }

    pub fn mailbox_out(&self) -> Arc<EmberThreadMailbox> {
        Arc::clone(&self.mailbox_out)
    }

    pub fn mailbox_in(&self) -> Arc<EmberThreadMailbox> {
        Arc::clone(&self.mailbox_in)
    }

    pub fn encrypt_text(&self, mut text: String) -> Result<Vec<u8>, FfiError> {
        let result = self
            .state
            .lock()
            .map_err(|_| FfiError::ThreadClosed)
            .and_then(|mut state| {
                let session = state.as_mut().ok_or(FfiError::ThreadClosed)?;
                session
                    .encrypt_text(&text)
                    .map(|envelope| envelope.to_bytes())
                    .map_err(map_thread_error)
            });
        text.zeroize();
        result
    }

    pub fn decrypt_message(&self, mut envelope: Vec<u8>) -> Result<ThreadMessageInfo, FfiError> {
        let result = self
            .state
            .lock()
            .map_err(|_| FfiError::ThreadClosed)
            .and_then(|mut state| {
                let session = state.as_mut().ok_or(FfiError::ThreadClosed)?;
                session
                    .decrypt_message_bytes(&envelope)
                    .map(thread_message_info)
                    .map_err(map_thread_error)
            });
        envelope.zeroize();
        result
    }

    /// Ends the session, wiping Rust-owned mailbox tokens and releasing the Noise transport.
    pub fn end_session(&self) {
        wipe_thread_session(&self.state);
        self.mailbox_out.end_session();
        self.mailbox_in.end_session();
    }
}

impl EmberThreadSession {
    fn from_keys(keys: HandshakeSessionKeys) -> Arc<Self> {
        let mailbox_out = Arc::new(EmberThreadMailbox {
            capability: Mutex::new(Some(keys.mailbox_out.clone())),
        });
        let mailbox_in = Arc::new(EmberThreadMailbox {
            capability: Mutex::new(Some(keys.mailbox_in.clone())),
        });
        let handle = generate_handle();
        Arc::new(Self {
            state: Mutex::new(Some(ThreadSession::new(handle, keys))),
            mailbox_out,
            mailbox_in,
        })
    }
}

impl Drop for EmberThreadMailbox {
    fn drop(&mut self) {
        wipe_mailbox_handle(&self.capability);
    }
}

impl Drop for EmberThreadInitiator {
    fn drop(&mut self) {
        if let Ok(state) = self.state.get_mut() {
            state.take();
        }
    }
}

impl Drop for EmberThreadSession {
    fn drop(&mut self) {
        wipe_thread_session(&self.state);
        self.mailbox_out.end_session();
        self.mailbox_in.end_session();
    }
}

fn thread_message_info(message: ThreadMessage) -> ThreadMessageInfo {
    ThreadMessageInfo {
        sequence: message.seq,
        timestamp: message.timestamp,
        sender_handle: message.sender_handle,
        text: message.text,
    }
}

fn wipe_mailbox_handle(capability: &Mutex<Option<MailboxCapability>>) {
    if let Ok(mut guard) = capability.lock() {
        if let Some(mut capability) = guard.take() {
            capability.id.zeroize();
            capability.push_token.zeroize();
            capability.pull_token.zeroize();
        }
    }
}

fn wipe_thread_session(session: &Mutex<Option<ThreadSession>>) {
    if let Ok(mut guard) = session.lock() {
        if let Some(mut session) = guard.take() {
            session.handle.zeroize();
            session.mailbox_out.id.zeroize();
            session.mailbox_out.push_token.zeroize();
            session.mailbox_out.pull_token.zeroize();
            session.mailbox_in.id.zeroize();
            session.mailbox_in.push_token.zeroize();
            session.mailbox_in.pull_token.zeroize();
        }
    }
}

fn map_thread_error(error: ember_core::EmberError) -> FfiError {
    match error {
        ember_core::EmberError::ReplayDetected => FfiError::ThreadReplay,
        ember_core::EmberError::InvalidMetadata(message)
            if message.contains("Message text length")
                || message.contains("Serialized message length") =>
        {
            FfiError::ThreadMessageTooLong
        }
        _ => FfiError::InvalidThreadMessage,
    }
}

#[uniffi::export]
pub fn generate_thread_handle() -> String {
    generate_handle()
}

#[uniffi::export]
pub fn is_valid_thread_handle(handle: String) -> bool {
    is_valid_handle(&handle)
}

/// Validates the fixed framing of a Noise handshake message without exposing its contents.
#[uniffi::export]
pub fn is_valid_thread_handshake_message(mut bytes: Vec<u8>) -> bool {
    let valid = bytes.len() == 49 && bytes.first() == Some(&2);
    bytes.zeroize();
    valid
}

#[uniffi::export]
pub fn parse_thread_envelope(mut bytes: Vec<u8>) -> Result<ThreadEnvelopeInfo, FfiError> {
    let result = EncryptedMessagePayload::from_bytes(&bytes)
        .map(|payload| ThreadEnvelopeInfo {
            version: payload.v,
            sequence: payload.seq,
            ciphertext: payload.ciphertext,
        })
        .map_err(map_thread_error);
    bytes.zeroize();
    result
}

#[uniffi::export]
pub fn serialize_thread_envelope(
    version: u8,
    sequence: u64,
    mut ciphertext: Vec<u8>,
) -> Result<Vec<u8>, FfiError> {
    let payload = EncryptedMessagePayload {
        v: version,
        seq: sequence,
        ciphertext: std::mem::take(&mut ciphertext),
    };
    let mut bytes = payload.to_bytes();
    let validation = EncryptedMessagePayload::from_bytes(&bytes).map_err(map_thread_error);
    if validation.is_err() {
        bytes.zeroize();
        return validation.map(|_| bytes);
    }
    Ok(bytes)
}

#[uniffi::export]
pub fn core_version() -> String {
    ember_core::version().to_string()
}

/// Validates the shared link format using the Rust core without exposing key bytes to Kotlin.
#[uniffi::export]
pub fn validate_link(mut link: String) -> Result<(), FfiError> {
    let parsed = ember_core::parse_link(&link);
    link.zeroize();
    parsed
        .map(|mut parsed| {
            zeroize_link_payload(&mut parsed.payload);
        })
        .map_err(|_| FfiError::InvalidLink)
}

/// Parses only the non-secret routing fields needed to contact the relay.
#[uniffi::export]
pub fn inspect_link(mut link: String) -> Result<DropLinkInfo, FfiError> {
    let parsed_result = ember_core::parse_link(&link);
    link.zeroize();
    let parsed = parsed_result.map_err(|_| FfiError::InvalidLink)?;
    let mut payload = parsed.payload;
    let passphrase_protected = matches!(&payload, LinkPayload::Protected(_));
    zeroize_link_payload(&mut payload);
    Ok(DropLinkInfo {
        host: parsed.host,
        blob_id: parsed.blob_id,
        passphrase_protected,
    })
}

/// Solves the server-issued hashcash challenge with a bounded amount of device work.
#[uniffi::export]
pub fn solve_pow(challenge: String, difficulty: u32) -> Result<u64, FfiError> {
    if difficulty > 32 {
        return Err(FfiError::InvalidPowDifficulty);
    }
    ember_core::solve_pow(challenge.as_bytes(), difficulty, Some(5_000_000))
        .map_err(|_| FfiError::PowFailed)
}

/// Runs the shared Rust metadata stripper with an Android memory cap and structured feedback.
#[uniffi::export]
pub fn strip_file_metadata(
    mut content: Vec<u8>,
    filename_hint: String,
) -> Result<MetadataStripReport, FfiError> {
    if content.len() > 8 * 1024 * 1024 {
        content.zeroize();
        return Err(FfiError::StripInputTooLarge);
    }
    let result = ember_core::strip_metadata(&content, Some(&filename_hint));
    content.zeroize();
    let result = result.map_err(|_| FfiError::InvalidCiphertext)?;
    let (status, format, fields_removed, reason) = match result.status {
        StripStatus::Stripped {
            format,
            fields_removed,
        } => (
            "stripped",
            format.to_string(),
            fields_removed,
            String::new(),
        ),
        StripStatus::NoMetadataFound { format } => {
            ("clean", format.to_string(), Vec::new(), String::new())
        }
        StripStatus::UnsupportedFormat { format, reason } => {
            ("unsupported", format.to_string(), Vec::new(), reason)
        }
    };
    Ok(MetadataStripReport {
        status: status.into(),
        format,
        fields_removed,
        reason,
        modified: result.modified,
        content: result.data,
    })
}

fn zeroize_passphrase(passphrase: &mut Option<String>) {
    if let Some(value) = passphrase.as_mut() {
        value.zeroize();
    }
}

fn zeroize_link_payload(payload: &mut LinkPayload) {
    if let LinkPayload::Protected(wrapped) = payload {
        wrapped.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_roundtrips_text_through_core_format() {
        let crypto = EmberCrypto::generate();
        let ciphertext = crypto
            .encrypt_text("M9 bridge check".into(), 100, 200)
            .expect("encryption should succeed");
        assert_eq!(crypto.decrypt_text(ciphertext).unwrap(), "M9 bridge check");
    }

    #[test]
    fn ffi_roundtrips_file_through_core_format() {
        let crypto = EmberCrypto::generate();
        let ciphertext = crypto
            .encrypt_file(
                "test.bin".into(),
                "application/octet-stream".into(),
                vec![0xAA, 0xBB, 0xCC],
                true,
                100,
                200,
            )
            .expect("encryption should succeed");
        let decrypted = crypto.decrypt_drop_payload(ciphertext).unwrap();
        assert_eq!(decrypted.kind, "file");
        assert_eq!(decrypted.name, "test.bin");
        assert_eq!(decrypted.content, vec![0xAA, 0xBB, 0xCC]);
        assert!(decrypted.burn);
    }

    #[test]
    fn ffi_streaming_file_roundtrip_keeps_parts_bounded_and_metadata_authenticated() {
        let sender = EmberCrypto::generate();
        let receiver_key = sender.key.clone();
        let content: Vec<u8> = (0..(1_048_576 + 37))
            .map(|index| (index % 251) as u8)
            .collect();
        let stream = EmberEncryptStream::new(
            Arc::clone(&sender),
            "../payload.bin".into(),
            "application/octet-stream".into(),
            content.len() as u64,
            true,
            100,
            200,
        )
        .unwrap();

        let mut encrypted_parts = vec![stream.take_part0().unwrap()];
        assert!(stream.take_part0().is_err());
        for chunk in content.chunks(64 * 1024) {
            encrypted_parts.extend(stream.update(chunk.to_vec()).unwrap());
        }
        encrypted_parts.extend(stream.finish().unwrap());
        assert!(encrypted_parts.iter().all(|part| part.len() <= 1_048_593));

        let decryptor = EmberDecryptStream::new(
            Arc::new(EmberCrypto { key: receiver_key }),
            encrypted_parts.remove(0),
        )
        .unwrap();
        let metadata = decryptor.metadata();
        assert_eq!(metadata.kind, "file");
        assert_eq!(metadata.name, "../payload.bin");
        assert_eq!(metadata.safe_name, "payload.bin");
        assert_eq!(metadata.size, content.len() as u64);
        assert!(metadata.burn);

        let mut recovered = Vec::new();
        let mut saw_final = false;
        for part in encrypted_parts {
            let chunk = decryptor.push_part(part).unwrap();
            recovered.extend(chunk.plaintext);
            saw_final = chunk.is_final;
        }
        assert!(saw_final);
        decryptor.finish().unwrap();
        assert_eq!(recovered, content);
    }

    #[test]
    fn ffi_rejects_expiry_before_creation() {
        let crypto = EmberCrypto::generate();
        assert!(matches!(
            crypto.encrypt_text("invalid".into(), 200, 100),
            Err(FfiError::InvalidExpiry)
        ));
    }

    #[test]
    fn ffi_text_options_authenticate_burn_intent() {
        let crypto = EmberCrypto::generate();
        let ciphertext = crypto
            .encrypt_text_with_options("burn intent".into(), true, 100, 200)
            .unwrap();
        let (metadata, plaintext) = ember_core::decrypt_drop(&crypto.key, &ciphertext).unwrap();
        assert!(metadata.burn);
        assert_eq!(plaintext, b"burn intent");
    }

    #[test]
    fn ffi_from_key_bytes_validates_key_length() {
        assert!(matches!(
            EmberCrypto::from_key_bytes(vec![0u8; 31]),
            Err(FfiError::InvalidKey)
        ));
        assert!(matches!(
            EmberCrypto::from_key_bytes(vec![0u8; 33]),
            Err(FfiError::InvalidKey)
        ));
        assert!(EmberCrypto::from_key_bytes(vec![0u8; 32]).is_ok());
    }

    #[test]
    fn ffi_rejects_unknown_link_version() {
        let link = "https://ember.example/d/AAAAAAAAAAAAAAAAAAAAAA#AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert!(matches!(
            validate_link(link.into()),
            Err(FfiError::InvalidLink)
        ));
    }

    #[test]
    fn ffi_builds_and_opens_unprotected_link_without_exporting_key() {
        let crypto = EmberCrypto::generate();
        let ciphertext = crypto
            .encrypt_text("link roundtrip".into(), 100, 200)
            .unwrap();
        let link = crypto
            .build_link("ember.example.com".into(), "AAAAAAAAAAAAAAAAAAAAAA".into())
            .unwrap();
        let info = inspect_link(link.clone()).unwrap();
        assert_eq!(info.host, "ember.example.com");
        assert_eq!(info.blob_id, "AAAAAAAAAAAAAAAAAAAAAA");
        assert!(!info.passphrase_protected);

        let opened = EmberCrypto::from_link(link, None).unwrap();
        assert_eq!(opened.decrypt_text(ciphertext).unwrap(), "link roundtrip");
    }

    #[test]
    fn ffi_builds_and_opens_passphrase_protected_link() {
        let crypto = EmberCrypto::generate();
        let ciphertext = crypto
            .encrypt_text("protected link roundtrip".into(), 100, 200)
            .unwrap();
        let link = crypto
            .build_protected_link(
                "ember.example.com".into(),
                "AAAAAAAAAAAAAAAAAAAAAA".into(),
                "correct horse battery staple".into(),
            )
            .unwrap();
        assert!(inspect_link(link.clone()).unwrap().passphrase_protected);
        assert!(matches!(
            EmberCrypto::from_link(link.clone(), None),
            Err(FfiError::MissingPassphrase)
        ));
        assert!(matches!(
            EmberCrypto::from_link(link.clone(), Some("wrong".into())),
            Err(FfiError::InvalidPassphrase)
        ));

        let opened =
            EmberCrypto::from_link(link, Some("correct horse battery staple".into())).unwrap();
        assert_eq!(
            opened.decrypt_text(ciphertext).unwrap(),
            "protected link roundtrip"
        );
    }

    #[test]
    fn ffi_solves_bounded_pow_challenge() {
        let challenge = "m10-test-challenge";
        let nonce = solve_pow(challenge.into(), 8).unwrap();
        assert!(ember_core::verify_pow(challenge.as_bytes(), nonce, 8));
        assert!(matches!(
            solve_pow(challenge.into(), 33),
            Err(FfiError::InvalidPowDifficulty)
        ));
    }

    #[test]
    fn ffi_metadata_strip_returns_removed_fields_and_content() {
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1, 0, 14];
        jpeg.extend_from_slice(b"Exif\0\0secret");
        jpeg.extend_from_slice(&[0xFF, 0xDA, 0, 8, 1, 1, 0, 0, 1, 2, 3, 0xFF, 0xD9]);

        let report = strip_file_metadata(jpeg, "photo.jpg".into()).unwrap();
        assert_eq!(report.status, "stripped");
        assert_eq!(report.format, "JPEG Image");
        assert!(report.modified);
        assert!(report
            .fields_removed
            .iter()
            .any(|field| field.contains("EXIF metadata")));
        assert!(!report.content.windows(6).any(|window| window == b"secret"));
    }

    #[test]
    fn ffi_metadata_strip_reports_unsupported_formats_without_modifying_bytes() {
        let bytes = b"opaque payload".to_vec();
        let report = strip_file_metadata(bytes.clone(), "payload.bin".into()).unwrap();
        assert_eq!(report.status, "unsupported");
        assert!(!report.modified);
        assert!(!report.reason.is_empty());
        assert_eq!(report.content, bytes);
    }

    #[test]
    fn ffi_metadata_strip_rejects_inputs_over_android_budget() {
        assert!(matches!(
            strip_file_metadata(vec![0; 8 * 1024 * 1024 + 1], "large.jpg".into()),
            Err(FfiError::StripInputTooLarge)
        ));
    }

    #[test]
    fn ffi_thread_handshake_encrypts_v3_messages_and_wipes_on_close() {
        let crypto = EmberCrypto::from_key_bytes(vec![0x42; 32]).unwrap();
        let rendezvous = crypto.derive_thread_rendezvous_mailbox().unwrap();
        assert!(rendezvous.id().unwrap() == "pCaOJMUTvhyytm-Dh5SuGQ");

        let initiator = crypto.start_thread_initiator().unwrap();
        let init_message = initiator.handshake_message().unwrap();
        assert!(init_message.len() == 49 && init_message[0] == 2);
        let responder_result = crypto.respond_to_thread(init_message.clone()).unwrap();
        assert!(responder_result.response_message.len() == 49);
        let initiator_session = initiator
            .complete(responder_result.response_message)
            .unwrap();
        let responder_session = responder_result.session;
        assert!(is_valid_thread_handle(initiator_session.handle().unwrap()));
        assert!(is_valid_thread_handle(responder_session.handle().unwrap()));

        let initiator_out = initiator_session.mailbox_out();
        let responder_in = responder_session.mailbox_in();
        assert!(initiator_out.id().unwrap() == responder_in.id().unwrap());
        assert!(initiator_out.push_token().unwrap() == responder_in.push_token().unwrap());
        assert!(initiator_out.pull_token().unwrap() == responder_in.pull_token().unwrap());

        let envelope = initiator_session
            .encrypt_text("binding thread test".into())
            .unwrap();
        let parsed = parse_thread_envelope(envelope.clone()).unwrap();
        assert!(parsed.version == 3 && parsed.sequence == 0);
        assert!(
            serialize_thread_envelope(parsed.version, parsed.sequence, parsed.ciphertext).unwrap()
                == envelope
        );
        let decrypted = responder_session.decrypt_message(envelope.clone()).unwrap();
        assert!(decrypted.sender_handle == initiator_session.handle().unwrap());
        assert!(decrypted.text == "binding thread test");
        assert!(matches!(
            responder_session.decrypt_message(envelope),
            Err(FfiError::ThreadReplay)
        ));

        initiator_session.end_session();
        responder_session.end_session();
        rendezvous.end_session();
        assert!(matches!(
            initiator_session.encrypt_text("after close".into()),
            Err(FfiError::ThreadClosed)
        ));
        assert!(matches!(
            rendezvous.push_token(),
            Err(FfiError::ThreadClosed)
        ));
    }

    #[test]
    fn ffi_thread_message_limit_errors_are_distinguishable_for_localization() {
        let crypto = EmberCrypto::generate();
        let initiator = crypto.start_thread_initiator().unwrap();
        let response = crypto
            .respond_to_thread(initiator.handshake_message().unwrap())
            .unwrap();
        let session = initiator.complete(response.response_message).unwrap();
        assert!(matches!(
            session.encrypt_text("x".repeat(5000)),
            Err(FfiError::ThreadMessageTooLong)
        ));
    }
}
