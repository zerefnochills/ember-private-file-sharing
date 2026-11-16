# ADR 0008: Ephemeral Thread Protocol via Audited Noise Protocol Framework (`snow`)

- **Status:** accepted (supersedes ADR 0006)
- **Date:** 2026-11-16
- **Deciders:** Antigravity (Backend), Review Item O1 & O2 Resolution

---

## Context

Milestone M7 introduced short-lived (at most 12 hours) 1:1 encrypted threads associated with each drop.
In ADR 0006, the initial implementation used a hand-composed composition of libsodium primitives (`dryoc` X25519, BLAKE2b, and XChaCha20-Poly1305) and mistakenly claimed formal verification. Furthermore, code used KDF sub-key ID 2 and 3 in a way that conflicted with `docs/protocol-spec.md` (which reserves sub-key ID 3 for the thread pre-shared key).

In the code review of milestones M0-M10 (`docs/reviews/M0-M10-claude-review.md`), open item **O1** identified this hand-composed cryptography as a high-priority risk and recommended replacing it with the audited `snow` crate using standard Noise patterns. Item **O2** required that mailbox creation (`POST /v1/mailboxes`) require a valid proof-of-work solution, matching drop creation.

---

## Decision

We adopt the **Noise Protocol Framework** using the audited `snow` crate (version 0.10.x) with stock resolvers and primitives:

### 1. Handshake Pattern: `Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s`

```text
Noise_NNpsk2:
  -> e
  <- e, ee, psk
```

- **`NN` (No Static Keys):** Neither party possesses or transmits a long-term identity key. Both generate fresh ephemeral X25519 keypairs ($e_A, e_B$). This matches Ember's account-free, link-based ephemeral model.
- **`psk2` Modifier:** The pre-shared key (`PSK`) is mixed into the handshake state at token position 2, after the ephemeral-ephemeral Diffie-Hellman exchange (`ee = DH(e_A, e_B)`).
- **Primitives:**
  - DH: X25519 (Curve25519)
  - Cipher: ChaCha20-Poly1305 (RFC 7539 / RFC 8439)
  - Hash: BLAKE2s (256-bit)

### 2. Spec-Aligned Pre-Shared Key (PSK) Derivation

The thread PSK is derived directly from the master drop secret key `K` using spec-reserved sub-key ID 3 and context `"EMBERv1_"`:

$$\text{PSK} = \text{crypto\_kdf\_derive\_from\_key}(32, \text{id}=3, \text{context}=\text{"EMBERv1\_"}, K)$$

This resolves the discrepancy in ADR 0006 and conforms strictly to `docs/protocol-spec.md` section 3.

### 3. Wire Message Format and Envelope Versioning

To ensure old-format messages are rejected cleanly by a version byte (rather than failing unpredictably):
- **Handshake Messages:**
  - Both initiator (Part 1) and responder (Part 2) handshake messages are prepended with a 1-byte version header `0x02`.
  - Initiator Message: `[0x02, ..noise_msg1]` (1 byte version + 48 bytes Noise payload = 49 bytes).
  - Responder Message: `[0x02, ..noise_msg2]` (1 byte version + 48 bytes Noise payload = 49 bytes).
  - Any message lacking version byte `0x02` or differing in length is rejected immediately with `EmberError::UnsupportedVersion` or `EmberError::Auth`.
- **Transport Envelope:**
  ```rust
  pub struct EncryptedMessagePayload {
      pub v: u8, // must be 2
      pub seq: u64,
      pub ciphertext: Vec<u8>,
  }
  ```
  - Old envelopes (`v = 0` or `v = 1`) are cleanly rejected with `EmberError::UnsupportedVersion`.
  - Nonce is implicit: Noise `StatelessTransportState` maps the 64-bit sequence counter `seq` to the 96-bit ChaCha20-Poly1305 nonce per RFC 7539.

### 4. Directional Mailbox Capabilities from Handshake Hash

Upon handshake completion, both initiator and responder possess the identical 32-byte Noise handshake hash $h$:

$$h = \text{handshake.get\_handshake\_hash}()$$

Because $h$ binds the entire transcript (protocol name, prologue, ephemeral keys, DH output, and PSK), directional mailboxes are derived deterministically:

$$\text{seed} = \text{crypto\_kdf\_derive\_from\_key}(32, \text{id}=6, \text{"EMBERv1\_"}, h)$$

- `mailbox_init_to_resp`: Initiator pushes, responder pulls.
- `mailbox_resp_to_init`: Responder pushes, initiator pulls.

### 5. Proof-of-Work for Mailbox Creation (O2)

Creating a mailbox via `POST /v1/mailboxes` requires a valid Proof-of-Work solution, reusing the M4 challenge mechanism. Solutions are supplied either in `CreateMailboxRequest.pow` or via `X-Pow-Challenge` and `X-Pow-Nonce` HTTP headers.

---

## Security Analysis & Guarantees

### What It Guarantees

1. **Mutual Authentication:** Only holders of the drop master key `K` (and thus `PSK`) can complete the handshake and establish transport cipher states.
2. **Forward Secrecy:** Ephemeral secret keys ($e_A, e_B$) are zeroized upon handshake completion. Even if the link fragment $K$ is subsequently leaked or harvested, an adversary who recorded the ciphertext traffic cannot decrypt the messages.
3. **Replay & Reorder Protection:** Handshake sessions are fresh per ephemeral exchange. Transport messages enforce strictly monotonic sequence counters (`seq`); replayed or out-of-order messages are rejected.
4. **Audited Implementation:** Eliminates custom hand-composed cryptographic state machines in favor of the vetted, formally analyzed Noise specification implemented by the audited `snow` library.

### What It Does NOT Guarantee

1. **No Persistent Identity:** Ember drops are account-free. The protocol authenticates possession of the drop link, not the human identity of the holder.
2. **Traffic Analysis & Metadata:** The relay server observes connection times, capability tokens, and encrypted payload lengths (up to 64 KiB).
3. **Marketing Limitation:** Thread forward secrecy **must NOT be marketed** until an independent cryptographer has reviewed the integration during external audit (milestone M13).

---

## Consequences & Migration

- Supersedes ADR 0006.
- AEAD for threads shifts from `XChaCha20-Poly1305` (192-bit random nonce) to standard Noise `ChaCha20-Poly1305` (RFC 7539 96-bit counter nonce).
- Handshake messages and message payloads now carry explicit version byte `2`.
- Test vectors updated in `testvectors/thread-v1.json`.
- Public core API signatures for `HandshakeInitiator`, `HandshakeResponder`, and `ThreadSession` remain stable.
