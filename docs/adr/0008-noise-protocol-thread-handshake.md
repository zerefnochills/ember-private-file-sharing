# ADR 0008: Ephemeral Thread Protocol via Noise Protocol Framework (`snow`)

- **Status:** accepted (supersedes ADR 0006)
- **Date:** 2026-11-16
- **Deciders:** Antigravity (Backend), Review Item O1 & O2 Resolution

---

## Context

Milestone M7 introduced short-lived (at most 12 hours) 1:1 encrypted threads associated with each drop.
In ADR 0006, the initial implementation used a hand-composed composition of libsodium primitives (`dryoc` X25519, BLAKE2b, and XChaCha20-Poly1305) and mistakenly claimed formal verification. Furthermore, code used KDF sub-key ID 2 and 3 in a way that conflicted with `docs/protocol-spec.md` (which reserves sub-key ID 3 for the thread pre-shared key).

In the code review of milestones M0-M10 (`docs/reviews/M0-M10-claude-review.md`), open item **O1** identified this hand-composed cryptography as a high-priority risk and recommended replacing it with the `snow` crate using standard Noise patterns. Item **O2** required that mailbox creation (`POST /v1/mailboxes`) require a valid proof-of-work solution, matching drop creation.

---

## Decision

We adopt the **Noise Protocol Framework** using the `snow` crate (version 0.10.0) with stock resolvers and primitives:

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

### 3. Wire Message Format and Binary Envelope (Version 3)

To ensure old-format messages are rejected cleanly by a version byte (rather than failing unpredictably):
- **Handshake Messages:**
  - Both initiator (Part 1) and responder (Part 2) handshake messages are prepended with a 1-byte version header `0x02`.
  - Initiator Message: `[0x02, ..noise_msg1]` (1 byte version + 48 bytes Noise payload = 49 bytes).
  - Responder Message: `[0x02, ..noise_msg2]` (1 byte version + 48 bytes Noise payload = 49 bytes).
  - Any message lacking version byte `0x02` or differing in length is rejected immediately with `EmberError::UnsupportedVersion` or `EmberError::Auth`.
- **Binary Transport Envelope (Version 3):**
  `v(1 byte = 0x03) | seq(8 bytes, big-endian) | ciphertext(variable)`
  - Replaces previous JSON envelope with a compact 9-byte binary header (`THREAD_MSG_HEADER_LEN = 9`).
  - Obsolete envelope versions (`v = 0`, `v = 1`, `v = 2`) or truncated envelopes (< 25 bytes) are rejected cleanly.
  - Text length capped at 4096 bytes (`MAX_THREAD_TEXT_BYTES = 4096`). Receivers validate ciphertext bounds (`MAX_THREAD_CIPHERTEXT_BYTES = 4624`) prior to allocating plaintext buffers.
  - Nonce is implicit: Noise `StatelessTransportState` maps the 64-bit sequence counter `seq` to the 96-bit ChaCha20-Poly1305 nonce per RFC 7539.
  - Monotonic gap acceptance: Receivers accept `seq >= expected_seq`, advancing state to `seq + 1` upon authenticated decryption, and reject `seq < expected_seq` as replays.
  - Inner message integrity: Inner `ThreadMessage.seq` MUST match the outer binary envelope sequence number, and `sender_handle` must match handle syntax `^[a-z]+-[a-z]+-\d{4}$`. Sequence counter is guarded against `u64::MAX` overflow.

### 4. Directional Mailbox Capabilities from Handshake Hash

Upon handshake completion, both initiator and responder possess the identical 32-byte Noise handshake hash $h$:

$$h = \text{handshake.get\_handshake\_hash}()$$

Because $h$ binds the entire transcript (protocol name, prologue, ephemeral keys, DH output, and PSK), directional mailboxes are derived deterministically:

$$\text{seed} = \text{crypto\_kdf\_derive\_from\_key}(32, \text{id}=6, \text{"EMBERv1\_"}, h)$$

- `mailbox_init_to_resp`: Initiator pushes, responder pulls.
- `mailbox_resp_to_init`: Responder pushes, initiator pulls.
- Sub-key IDs: Mailbox 1 (10, 11, 12); Mailbox 2 (20, 21, 22); Rendezvous Mailbox (1, 2, 3).

### 5. Proof-of-Work for Mailbox Creation (O2)

Creating a mailbox via `POST /v1/mailboxes` requires a valid Proof-of-Work solution, reusing the M4 challenge mechanism. Solutions are supplied either in `CreateMailboxRequest.pow` or via `X-Pow-Challenge` and `X-Pow-Nonce` HTTP headers.

---

## Security Analysis & Guarantees

### What It Guarantees

1. **Mutual Authentication:** Only holders of the drop master key `K` (and thus `PSK`) can complete the handshake and establish transport cipher states. The initiator is authenticated to the responder by its first valid transport message.
2. **Session-Level Forward Secrecy:** Ephemeral secret keys ($e_A, e_B$) are zeroized upon handshake completion. Even if the link fragment $K$ is subsequently leaked or harvested, an adversary who recorded the ciphertext traffic cannot decrypt the messages. Message keys are derived deterministically by the Noise transport cipher for the duration of the 12-hour session (session-level forward secrecy, not per-message ratcheting).
3. **Replay & Gap Protection:** Handshake sessions are fresh per ephemeral exchange. Transport messages enforce strictly monotonic sequence counters (`seq >= expected_seq`); replays (`seq < expected_seq`) are strictly rejected.
4. **Standard Specification:** Eliminates custom hand-composed cryptographic state machines in favor of the formally analyzed Noise Protocol specification implemented by the `snow` library (version 0.10.0).

### What It Does NOT Guarantee

1. **No Persistent Identity:** Ember drops are account-free. The protocol authenticates possession of the drop link, not the human identity of the holder.
2. **Rendezvous Mailbox Interception:** Any holder of the drop link fragment $K$ can derive the rendezvous mailbox capability and read or delete rendezvous messages before the peer retrieves them.
3. **Multiple Initiator Races:** If multiple parties scan the same QR code / drop link, each attempts a handshake on the rendezvous mailbox. The responder accepts the first valid handshake message it retrieves; subsequent or racing attempts fail or are ignored, creating a denial-of-service risk.
4. **Unpadded Message Traffic Analysis:** Thread messages are NOT padded to fixed lengths (unlike drop files which are padded to 64 KiB bands). Encrypted payload sizes leak approximate message length to relay observers.
5. **Marketing Limitation:** Thread forward secrecy **must NOT be marketed** until an independent cryptographer has reviewed the integration during external audit (milestone M13).

---

## Consequences & Migration

- Supersedes ADR 0006.
- AEAD for threads shifts from `XChaCha20-Poly1305` (192-bit random nonce) to standard Noise `ChaCha20-Poly1305` (RFC 7539 96-bit counter nonce).
- Handshake messages carry explicit version byte `2`. Message envelopes carry explicit version byte `3` in binary wire format `v(1) | seq(8) | ciphertext`.
- Test vectors updated in `testvectors/thread-v1.json`.
- Public core API signatures for `HandshakeInitiator`, `HandshakeResponder`, and `ThreadSession` remain stable.
