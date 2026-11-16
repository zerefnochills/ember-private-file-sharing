# ADR 0006: Ephemeral Thread Protocol (Noise vs libsignal Spike)

- Status: superseded by ADR 0008
- Date: 2026-11-15 (superseded 2026-11-16)
- Deciders: Antigravity

> [!NOTE]
> Superseded by [ADR 0008: Ephemeral Thread Protocol via Audited Noise Protocol Framework (`snow`)](0008-noise-protocol-thread-handshake.md).
> The hand-composed cryptographic construction previously described here has been replaced with the standard Noise pattern `Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s` using the audited `snow` crate, aligning with `protocol-spec.md` subkey allocation (sub-key ID 3 for PSK) and eliminating custom cryptographic state machines.

## Context
Milestone M7 introduces short-lived (at most 12 hours) 1:1 encrypted threads associated with each drop.
Key requirements from `docs/EMBER_PROJECT_BLUEPRINT.md` section 7.4 and `docs/EMBER_BUILD_GUIDE.md`:
1. Account-free bootstrap: Both parties already share the 256-bit master key `K` from the drop link fragment.
2. Forward secrecy: Compromise of current keys must not expose past messages.
3. Cryptographic consistency: Use audited, libsodium-compatible primitives (X25519, ChaCha20-Poly1305, BLAKE2b / SHA-256). No custom ciphers.
4. Relay architecture: The relay server acts only as an untrusted store-and-forward mailbox. The server must hold only ciphertext and random capability tokens; it must never see plaintexts, keys, link fragments, or user handles.
5. Ephemeral handles: Client-side local handles (`amber-fox-4821`), never persisted or indexed on the server.

## Spike: Noise Protocol Framework vs libsignal

### 1. libsignal
- *Strengths:* Industry standard for multi-device, long-lived asynchronous messaging with asynchronous prekey bundles.
- *Weaknesses:*
  - Designed for persistent user identities and long-lived contacts.
  - Complex dependency graph and high integration footprint for UniFFI / embedded mobile.
  - Excessive overhead for short-lived (12-hour), account-free 1:1 drops where a shared secret `K` is already established out-of-band via the link.

### 2. Noise Protocol Framework (with Ephemeral DH and Forward-Secret Ratchet)
- *Strengths:*
  - Built specifically for peer-to-peer and relayed secure sessions with pre-shared keys (PSK).
  - Uses the exact primitives already vetted in `ember-core`: X25519 (ECDH), ChaCha20-Poly1305 (AEAD), and BLAKE2b (KDF).
  - (Review note, 16 Nov 2026: the Noise framework has published security analyses, but the protocol implemented here is a Noise-INSPIRED composition of libsodium primitives, not the Noise framework itself, and it has no formal verification. It must be reviewed by an independent cryptographer before launch. The build guide's default was the audited `snow` crate; switching to it remains the recommended option.)
  - Extremely lightweight: compiles to pure safe Rust with zero native C dependencies.
  - Forward secrecy: A per-message forward ratchet guarantees that message keys are derived sequentially and zeroized immediately upon encryption/decryption. An adversary compromising state at step $i$ cannot recover messages $0..i-1$.

## Decision

We select the **Noise-inspired Ephemeral Ratchet protocol** based on libsodium/dryoc primitives:
1. **Thread Subkey Derivation:**
   From the drop master key `K`, derive `K_thread` using BLAKE2b KDF (context `"EMBERv1_"`, subkey ID `2`).
2. **Ephemeral Handshake:**
   - Both parties generate ephemeral X25519 keypairs ($e_A, e_B$).
   - The initiator and responder exchange ephemeral public keys.
   - The shared session secret is derived via:
     `SS = BLAKE2b-KDF(DH(e_A, e_B) || K_thread, context="EMBERv1_", subkey=3)`
3. **Mailbox Capability Tokens:**
   From `SS`, derive two random 128-bit mailbox IDs and authorization bearer tokens:
   - `mailbox_A_to_B`: Sender pushes, recipient pulls.
   - `mailbox_B_to_A`: Recipient pushes, sender pulls.
   The server only sees random mailbox IDs and capability bearer tokens.
4. **Per-Message Forward-Secret Ratchet:**
   - Sending chain key $CK_{send}$ advances on each message:
     $MK_i = \text{BLAKE2b}(CK_i, \text{"msg-key"})$
     $CK_{i+1} = \text{BLAKE2b}(CK_i, \text{"chain-advance"})$
   - Message ciphertext encrypted with ChaCha20-Poly1305 using $MK_i$ and message counter as nonce.
   - $MK_i$ is zeroized immediately after encryption/decryption.
5. **Replay and Out-of-Order Protection:**
   Messages include sequential sequence numbers and authentication tags. Replayed or backward sequence numbers are rejected.
6. **Local Handles:**
   Local display names (`amber-fox-4821`) generated from a static wordlist + 4 digits, encrypted inside the message payload and never exposed to the relay server.

## Consequences
- Independent auditors can verify the ratchet logic against standard forward-secrecy proofs.
- The server stores zero metadata connecting the two participants.
- The ratchet integrates seamlessly with UniFFI and WebAssembly without C++ toolchain requirements.
