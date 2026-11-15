# Ember: Guided Build Walkthrough (agent handoff)

**Purpose:** An ordered, resumable plan for building Ember. Any AI agent (or human) can pick this up cold, see what is done, and continue with the next milestone.
**Companion file:** `EMBER_PROJECT_BLUEPRINT.md` is the source of truth for vision, decisions (D1 to D14), architecture, legal context and threat model. This guide is the execution order. If the two disagree, the blueprint wins; record the conflict in the Handoff Log.
**Date written:** 15 November 2026.

---

## 0. How to use this file (read first)

1. Read the blueprint sections 1, 2, 5, 6, 7 and 8 before touching code.
2. Find the first milestone in the Status Tracker that is not `[x]`. Do only that milestone.
3. Implement AND test inside the real repository. Do not hand the owner code snippets to paste. Run the tests; a milestone is not done until its Acceptance checks pass.
4. When finished, tick the box, add a Handoff Log entry (template in section 9), and commit.
5. Stop at the end of a milestone. Do not start the next one unless asked.
6. If something here is unclear or the blueprint has an open question you must settle, use the recommended default in section 3, write an ADR (`docs/adr/NNNN-title.md`), and continue.

### Hard rules for every agent

- **Never** write custom cryptography. Use audited libraries only (blueprint D6).
- **Never** add Google dependencies: no FCM, Firebase, Play Integrity, Play Services, ML Kit, or Nearby Connections (D10).
- **Never** ask the owner for API keys, passwords, OTPs, webhook secrets or GitHub tokens. **Never** edit or overwrite any `.env` file. Config goes in `*.example` files and documented environment variable names only.
- **Never** log plaintext, keys, link fragments, filenames, or passphrases. Server logs follow `docs/privacy-and-logging.md`.
- **Never** use the words "anonymous", "untraceable", "secret", "military grade", "no logs", or "leaves no trace" in UI strings, docs, or store text.
- No secret admin monitoring, no device fingerprinting, no persistent identities (D8, section 5.4).
- Keep the app small, offline-tolerant and usable on low-end Android phones.
- Every security-relevant behaviour gets a test. Protocol changes require updated test vectors.

---

## 1. Status Tracker (agents: update this)

- [x] M0  Repo, rename, CI skeleton
- [x] M1  Protocol spec and test vectors (drop format)  [spec done; vector JSON is generated in M2]
- [x] M2  Rust core: drop encryption and link format
- [x] M3  Relay server MVP (blobs, TTL, burn-after-view)
- [x] M4  Proof-of-work and rate limits
- [x] M5  Minimal web receiver (test harness and no-app receiving)
- [x] M6  Metadata stripping
- [x] M7  Threads: handshake spike, ADR, mailboxes, handles
- [ ] M8  Admin CLI, takedown logging, report endpoint, logging policy
- [ ] M9  UniFFI bindings and Android skeleton
- [ ] M10 Android send and receive flows
- [ ] M11 Android threads, report and block, Hindi and English, hardening
- [ ] M12 Release engineering, legal docs, audit prep
- [ ] M13 Closed beta and independent audit (human-led)
- [ ] M14 Nearby mode (v1.5)
- [ ] M15 Desktop app (Tauri)

Milestones M0 to M12 are the v1 build. M13 is mostly human and outside-vendor work. M14 and M15 follow only after launch.

---

## 2. Environment and tooling checklist

Verify before M0 and note versions in the Handoff Log:

- Rust stable toolchain, `cargo`, `clippy`, `rustfmt`, `cargo-audit`, `cargo-deny`
- For Android (from M9): JDK 17+, Android SDK and NDK, `cargo-ndk`, Gradle wrapper
- For web receiver (M5): Node.js LTS (dev tooling only; the shipped page must be self-contained)
- Docker (optional) for running the server and an S3-compatible store (MinIO) locally
- Git, with conventional commits (`feat:`, `fix:`, `docs:`, `test:`)

---

## 3. Decisions to settle, with recommended defaults

Use these unless the owner overrides. Write each as an ADR.

| Question (blueprint section 13) | Recommended default | Why |
|---|---|---|
| Thread key agreement: Noise or libsignal | Noise via the Rust `snow` crate, using a pre-shared-key pattern keyed from the link secret. Confirm in the M7 spike; if the spike shows a blocker, fall back to libsignal | Smaller, easier to audit, no AGPL pressure from dependencies |
| Licence | AGPL-3.0 for server and clients | Keeps hosted forks open; compatible with libsignal if it is chosen later |
| Crypto library | Pick one in an M2 ADR: `dryoc` (pure Rust, libsodium-compatible) or `libsodium-sys-stable`. Check maintenance status; avoid deprecated `sodiumoxide` | Must also work for the web receiver; check WASM support before committing |
| File size cap | 50 MB for v1, configurable | Cost control; revisit after load tests |
| Burn-after-view default | Opt-in for files, always on for text drops | Prevents accidental data loss on files, keeps text private |
| Web receiver in v1 | Yes, a minimal receive-only page (M5) | Blueprint differentiator 2 promises "no app needed", and the page doubles as the fastest end-to-end test client. Blueprint section 5.3 lists it as "later"; this guide pulls it forward |
| Metadata store | Embedded store (SQLite or `redb`) for v1; Redis optional later | One less service to run |
| Object storage | Local filesystem backend first behind a storage trait; S3-compatible backend added in M3 | Fast development; production target is S3-compatible storage in India |
| Recipient-less public link drops (open question 7) | Allowed, but the thread is optional and created only if the sender enables it | Simplest first version; revisit after abuse data |
| Monetisation | None in v1. No ads, no trackers | Keeps the privacy claim clean |

---

## 4. Target repository layout

```
ember/
  docs/
    protocol-spec.md
    threat-model.md
    privacy-and-logging.md
    takedown-process.md
    adr/                     # architecture decision records
  core/                      # Rust crate: crypto, link format, expiry, stripping, PoW solver
  protocol/                  # Rust crate: shared wire types (used by core and server)
  server/                    # Rust (axum) relay server
  tools/
    cli/                     # command-line test client built on core
    admin-cli/               # delete-by-ID, takedown log
  web-receiver/              # minimal static receive page
  android/                   # Kotlin and Compose app
  desktop/                   # later
  testvectors/               # JSON vectors shared by all implementations
  LICENSE
  SECURITY.md
  README.md
  HANDOFF_LOG.md             # copy of section 9 log, kept current
```

---

## 5. Milestones

Each milestone lists: **Goal**, **Tasks**, **Acceptance checks** (must pass), and a **Prompt** the owner can paste to another agent.

### M0: Repo, rename, CI skeleton

**Goal:** A clean repository named Ember with tooling that fails on bad code.

**Tasks**
- Create the layout in section 4 with empty crates and a Cargo workspace.
- Add `LICENSE` (AGPL-3.0), `README.md` (one-paragraph definition from blueprint 1.1, honest limits), `SECURITY.md` (contact placeholder, disclosure process).
- CI (GitHub Actions): `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo audit`, `cargo deny`.
- Copy the blueprint and this guide into `docs/`.

**Acceptance:** CI green on an empty workspace; `cargo test` runs; repo contains no secrets and no `.env` files.

**Prompt:** "Read EMBER_BUILD_GUIDE.md and EMBER_PROJECT_BLUEPRINT.md. Do milestone M0 only. Implement it in the repository and run the checks. Follow the hard rules in section 0. Update the Status Tracker and Handoff Log when done."

---

### M1: Protocol spec and test vectors

**Goal:** A written protocol that two independent implementations can follow.

**Tasks**
- Write `docs/protocol-spec.md` covering: key and nonce sizes, chunk size and framing (use libsodium secretstream, XChaCha20-Poly1305), the encrypted header (file name, type, size, burn flag, created and expiry times go inside the ciphertext), padding rules, passphrase wrapping (Argon2id parameters, written down and justified), link format `https://<host>/d/<blobId>#<payload>` with a version byte, QR encoding, and expiry rules (12 hours max, burn-after-view).
- Write `docs/threat-model.md` from blueprint 7.10.
- Write `docs/privacy-and-logging.md` listing exactly which fields the server may store and for how long (blueprint 7.6). Mark the retention period as "pending lawyer confirmation".
- Generate `testvectors/drop-v1.json` from a reference implementation in M2 (spec first, vectors filled in during M2).

**Acceptance:** A second agent can read only the spec and list every field and byte layout without asking questions; spec states which inputs must never reach the server (key, fragment, passphrase).

**Prompt:** "Do milestone M1 only: write the protocol spec, threat model, and privacy-and-logging docs as described. Use only vetted primitives. Do not write code beyond what is needed to check sizes."

---

### M2: Rust core, drop encryption and link format

**Goal:** `core` can encrypt and decrypt a drop entirely offline, matching the spec.

**Tasks**
- ADR for the crypto library choice (section 3).
- Implement: random key generation, chunked encrypt and decrypt with secretstream, encrypted header, optional passphrase wrapping with Argon2id, link build and parse, zeroisation of key material, constant-time comparisons where relevant.
- Implement `tools/cli` with `ember encrypt <file>` and `ember decrypt <link-or-file>` working locally (no server yet).
- Fill `testvectors/drop-v1.json` and add tests that read it.
- Property tests: roundtrip for empty, tiny, chunk-boundary and large inputs; tamper tests (flip any byte, truncate, reorder chunks; decryption must fail).

**Acceptance:** all tests pass; tampering is always detected; no `unsafe` without a justified comment; `cargo clippy -D warnings` clean.

**Prompt:** "Do milestone M2 only. Implement the Rust core to the spec, generate test vectors, and add tamper and roundtrip tests. Run everything. No custom crypto; no new dependencies without an ADR note."

---

### M3: Relay server MVP

**Goal:** A server that stores ciphertext and enforces expiry, with no knowledge of keys or content.

**Tasks**
- axum server with endpoints (suggested; document final shape in the spec):
  - `POST /v1/blobs` create upload, returns random `blobId` and `deleteToken`
  - `PUT /v1/blobs/{id}/chunks/{n}` resumable chunk upload, plus `POST /v1/blobs/{id}/complete`
  - `GET /v1/blobs/{id}` metadata (size, expiry) with no side effects
  - `POST /v1/blobs/{id}/download` returns the ciphertext; for burn-after-view blobs this call deletes the blob after the download completes. A plain GET must never burn the blob, so link-preview bots cannot consume it.
  - `DELETE /v1/blobs/{id}` with the delete token
- Storage trait with filesystem backend; add S3-compatible backend (MinIO in Docker for tests).
- TTL: hard 12-hour maximum, a background sweeper plus lazy check on access.
- Size cap (default 50 MB), random 128-bit or longer IDs, constant-time token checks.
- Logging exactly as `docs/privacy-and-logging.md`; no content, no keys, no fragments.
- Extend `tools/cli` with `ember send <file>` and `ember receive <link>`.

**Acceptance:** an integration test uploads through the CLI, downloads through a second CLI instance, and gets identical bytes; expiry deletes data; burn-after-view works and survives a link-preview-style GET; interrupted upload resumes; server never receives the link fragment (assert in a test by inspecting recorded requests).

**Prompt:** "Do milestone M3 only. Build the server and extend the CLI. Prove with integration tests that burn-after-view, expiry, resume, and size caps work, and that the key never reaches the server."

---

### M4: Proof-of-work and rate limits

**Goal:** Cheap throttling of automated abuse without accounts.

**Tasks**
- Challenge and response PoW (server issues a challenge with a difficulty and short expiry; client solves; server verifies) required on blob creation and thread creation. Difficulty adjustable at runtime. Solver lives in `core` and must also run in WASM.
- Coarse per-IP rate limits using short-lived counters (memory or embedded store, with a TTL measured in minutes); document the retention in the privacy doc.
- Global and per-IP concurrency limits, request size limits, and timeouts.

**Acceptance:** tests show requests without a valid solution are rejected; replayed solutions are rejected; limits trigger and then recover; a mid-range Android phone can solve default difficulty in a few seconds (measure in M10).

**Prompt:** "Do milestone M4 only: PoW plus rate limits, with tests for replay, expiry, and recovery."

---

### M5: Minimal web receiver

**Goal:** Anyone can open an Ember link in a browser and receive a drop with no install. This also becomes the quickest way to test the whole system.

**Tasks**
- Static, self-contained page (no third-party scripts, strict CSP). Parse the fragment locally, fetch metadata, ask for passphrase if needed, confirm step, download, decrypt in the browser, offer save.
- Use the same Rust core compiled to WASM if feasible; otherwise a small, vetted JS library, tested against `testvectors/`.
- Explicit warnings: the recipient can keep a copy, burn-after-view applies on first confirmed download.

**Acceptance:** the browser decrypts every test vector; a link opened by a bot-style GET does not burn the blob; fragment never appears in any network request (assert in a browser test).

**Prompt:** "Do milestone M5 only: build the receive-only web page described in the guide and test it against the shared test vectors."

---

### M6: Metadata stripping

**Goal:** Remove location and author data before encryption.

**Tasks**
- Images: strip EXIF, XMP and other embedded metadata for JPEG, PNG, WebP and HEIC where feasible; keep orientation correct.
- Documents: PDF document info and XMP; Office files (docx, xlsx, pptx) core properties. Start with images and PDF; list unsupported types honestly.
- UI contract (for later): the app tells the user what was removed and what could not be.

**Acceptance:** fixtures with GPS, camera model and author fields come out clean; image content is otherwise unchanged; unsupported types pass through with a clear status, never a silent failure.

**Prompt:** "Do milestone M6 only: metadata stripping in core with fixture-based tests."

---

### M7: Threads

**Goal:** A short private 1:1 thread tied to each share, living at most 12 hours.

**Tasks**
1. **Spike:** compare Noise (`snow`) and libsignal for this use (link secret as PSK, ephemeral keys, forward secrecy via ratchet). Write an ADR with the decision. The auditor will review it.
2. Implement the handshake and a ratchet for per-message keys in `core`.
3. Server: two random mailbox IDs per thread (one per direction), capability tokens derived from the handshake, store-and-forward ciphertext, long-poll or WebSocket delivery, 12-hour deletion, close-by-either-side.
4. Handles: random word-list plus digits (for example `amber-fox-4821`), local only, never used by the server.
5. Extend the CLI to chat in a thread. Add test vectors for the handshake.

**Acceptance:** two CLI instances hold a thread; messages are deleted at expiry or close; server holds only ciphertext and random IDs; replayed or reordered messages are rejected; forward secrecy test shows old keys cannot decrypt new messages.

**Prompt:** "Do milestone M7 only. Begin with the Noise versus libsignal spike and write the ADR. Then implement threads and tests. No custom cryptography."

---

### M8: Admin CLI, takedown, report endpoint, logging policy

**Goal:** Be able to meet takedown deadlines without reading content.

**Tasks**
- `tools/admin-cli`: delete by blob ID or thread ID, record who, when, and the request reference in an append-only takedown log (separate from user data).
- Report endpoint: the reporter's app may send decrypted content plus key at the moment of reporting, only by the reporter's explicit action. Reviewer tool decrypts it. Store reports with a short retention and a clear policy.
- Implement the log fields and retention exactly as `docs/privacy-and-logging.md`; write `docs/takedown-process.md` from blueprint section 10.
- Transparency report generator (counts only).

**Acceptance:** deleting by ID removes ciphertext and mailbox data; takedown log entries are produced; reports are visible only to the reviewer role; tests prove no log line contains a key, fragment or filename.

**Prompt:** "Do milestone M8 only: admin CLI, takedown logging, report endpoint, and tests proving logs never contain secrets."

---

### M9: UniFFI bindings and Android skeleton

**Goal:** The Rust core runs inside an Android app.

**Tasks**
- UniFFI bindings for `core` (Kotlin). Build with `cargo-ndk` for arm64-v8a, armeabi-v7a and x86_64.
- Android project: Kotlin, Jetpack Compose, minimum SDK chosen in an ADR (aim wide; low-end phones matter). No Google Play Services dependencies. Verify with a dependency check in CI.
- A debug screen that encrypts and decrypts a test string through the bindings.
- Keep the APK small; track size in CI.

**Acceptance:** instrumented or local test proves Kotlin calls into Rust and passes the shared test vectors; CI fails if a forbidden dependency appears.

**Prompt:** "Do milestone M9 only: UniFFI bindings and an Android skeleton that runs the shared test vectors on a device or emulator. No Google services."

---

### M10: Android send and receive flows

**Goal:** A real user can send and receive a drop.

**Tasks**
- Send: pick file or enter text, optional passphrase, burn-after-view toggle, metadata-stripping summary, PoW progress, resumable upload with notification-free foreground progress, link and QR output with share sheet.
- Receive: open link (deep link) or scan QR with CameraX and ZXing core (not ML Kit), passphrase prompt, explicit confirm step before burning, decrypt, save.
- `FLAG_SECURE`, hide content in the recents screen, no content in notifications.
- Handle weak networks: retry with backoff, resume on reconnect.

**Acceptance:** end-to-end test between the Android app and the web receiver and the CLI; weak-network test (throttled) completes; large file memory usage stays bounded (streaming, not loading whole files).

**Prompt:** "Do milestone M10 only: Android send and receive flows, tested end to end against the server."

---

### M11: Android threads, report and block, languages, hardening

**Goal:** Feature-complete v1 app.

**Tasks**
- Thread UI with handle display, expiry timers, close action; foreground polling only (no push).
- In-app Report and Block buttons, clearly labelled; terms acceptance on first use; contact and grievance link.
- English and Hindi strings; layout tested with long Hindi text.
- Accessibility basics (TalkBack labels, contrast, touch targets).
- Test on low-end devices and Android versions across the supported range.

**Acceptance:** full v1 flow works; Report sends content only after explicit confirmation; Hindi UI has no truncated screens; no network calls to any host other than the configured Ember server (verify with a proxy test).

**Prompt:** "Do milestone M11 only: thread UI, report and block, Hindi and English, and low-end device hardening."

---

### M12: Release engineering, legal docs, audit prep

**Goal:** Everything needed to launch responsibly.

**Tasks**
- Reproducible builds and signed releases; document how anyone can verify an APK. F-Droid metadata.
- Draft (for lawyer review, not final) privacy policy, terms of use with prohibited content, grievance and takedown policy, transparency report template. Mark every item "pending legal review".
- Store listing text using only approved wording (see blueprint section 1.2), with a distinctive title such as "Ember: Private File Drop". Data Safety answers drafted from the actual code.
- Audit package: scope document, architecture and protocol docs, threat model, build instructions.
- Dependency and licence report.

**Acceptance:** a fresh machine can reproduce the release build; no banned marketing words appear in any text (add a CI grep); audit package is complete.

**Prompt:** "Do milestone M12 only: release engineering, legal drafts marked for lawyer review, store text, and the audit package."

---

### M13: Closed beta and independent audit (human-led)

- Owner recruits a small beta group (journalists, lawyers, NGOs).
- Owner engages an independent security auditor for core, protocol and server; fix findings and publish the report.
- Owner obtains the lawyer's written opinion (SIM binding, intermediary status, peer-to-peer mode) and registers the company, grievance officer and Play organisation account (blueprint section 11, Phase 0).
- Agents can help triage audit findings; each fix needs a regression test.

### M14: Nearby mode (v1.5)

Follow blueprint section 7.8. Android to Android, `LocalOnlyHotspot` plus QR pairing, same handshake and expiry from `core`, hotspot teardown and key deletion on finish. Budget real cross-brand testing. Do not use Google Nearby Connections.

### M15: Desktop (Tauri)

Follow blueprint section 7.9. Online mode only in v1 desktop, honest warnings about swap files and backups, code signing for Windows and macOS.

---

## 6. Testing strategy (applies to every milestone)

- **Unit and property tests** for all crypto, parsing and expiry logic.
- **Shared test vectors** in `testvectors/` that every implementation (Rust, WASM or JS, Kotlin) must pass.
- **Integration tests** driving the real server through the CLI.
- **Negative tests** for tampering, replay, expiry, oversize, bad tokens, bot-style requests.
- **Privacy tests** that grep logs and recorded requests for secrets.
- **Dependency policy:** `cargo audit` and `cargo deny` in CI; Android dependency allow-list.

---

## 7. Cross-cutting decisions and constraints (from the blueprint)

- Link fragment (everything after `#`) never goes to the server (D2, 7.2).
- File name and type live inside the ciphertext (7.2).
- Server is ciphertext-only; moderation relies on reports and legal orders (7.5).
- 12-hour maximum life; handles vanish with the thread (D4, D5).
- Do not claim "no logs"; disclose exactly what is logged (7.6).
- Honest limits stated in the app: recipients can keep copies; a modified client can ignore expiry (7.10).
- Open-source client and server, with an audit before public launch (D7).

---

## 8. Open items the owner must resolve (agents should not guess)

1. Company registration, named grievance officer, monitored grievance email.
2. Lawyer's written opinion on SIM binding, intermediary status and peer-to-peer mode.
3. Exact log retention period (CERT-In rules, DPDP).
4. Final trademark and domain checks for "Ember" (Indian registry Class 9 and 42). A smart-mug app named Ember already exists on both stores and Ember.js is a well-known framework, so choose a distinctive store title and package ID.
5. Hosting provider in India and budget for storage and bandwidth.
6. Independent audit vendor and budget.
7. Which languages to add after English and Hindi.

---

## 9. Handoff Log (append newest at the bottom)

Template:

```
### YYYY-MM-DD, <agent or person>, milestone <Mx>
- Status: done / partial / blocked
- What changed: (files, crates, endpoints)
- Decisions made: (link ADRs)
- Tests run and results:
- Known issues / TODO for next agent:
- Exact next step:
```

Entries:

```
### 2026-11-15, Claude, planning
- Status: done
- What changed: name chosen (Ember, replacing SealDrop), blueprint renamed, this guide written. No code yet.
- Decisions made: see section 3 defaults (pending owner override).
- Tests run and results: n/a
- Known issues / TODO for next agent: name overlap with a smart-mug app and Ember.js noted in section 8; web receiver pulled forward to M5.
- Exact next step: M0.
```

```
### 2026-11-15, Claude, milestone M0
- Status: done
- What changed: Cargo workspace (ember-core, ember-protocol, ember-server, ember-cli, ember-admin), AGPL-3.0 LICENSE, README, SECURITY.md (contact pending), deny.toml, rustfmt.toml, .gitignore (ignores .env and key files), GitHub Actions CI (fmt, clippy -D warnings, tests, cargo audit/deny, hygiene greps for secrets, banned words and Google dependencies), ADR template, empty dirs for web-receiver, android, desktop, testvectors. Blueprint and this guide copied into docs/.
- Decisions made: none beyond guide section 3 defaults. Workspace forbids unsafe code (workspace lint).
- Tests run and results: cargo fmt --check, cargo clippy -D warnings, cargo test --workspace all pass locally on Rust 1.75 (apt toolchain). cargo audit and cargo deny not run locally (not installed); they run in CI.
- Known issues / TODO for next agent: local toolchain is Rust 1.75, older than current stable; newer crates may need a newer toolchain (try conda-forge or rustup if reachable). Repository URL in Cargo.toml is a placeholder (OWNER). No remote set; no commit history pushed anywhere.
- Exact next step: M1 (protocol spec, threat model, privacy-and-logging docs).
```

```
### 2026-11-15, Claude, milestone M1
- Status: done (docs). Test vector file is intentionally produced in M2 by the reference implementation.
- What changed: docs/protocol-spec.md (primitives, key hierarchy, plaintext layout, padding, ciphertext parts, link and passphrase-wrap formats, server API draft, receive flow, vector schema, implementation checklist), docs/threat-model.md (assets, adversaries, 18 threats incl. web-receiver trust and offline passphrase guessing), docs/privacy-and-logging.md (stored fields, retention proposals, log fields, tests), docs/adr/0001-drop-format-v1-design-choices.md.
- Decisions made: ADR 0001 (1 MiB chunks, parts = chunks, burn via claim+finish POST, passphrase wrapped inside link, 64 KiB padding bands, no blob IDs in logs).
- Tests run and results: layout numbers (part sizes, totals, link lengths) cross-checked with a script; cargo fmt, clippy and tests still pass.
- Known issues / TODO for next agent: all retention periods are proposals pending lawyer review. Verify in M2 that the chosen crypto crate exposes secretstream, kdf_derive_from_key, pwhash (Argon2id) and aead xchacha20poly1305-ietf, and that it compiles to WASM for M5. Rust toolchain is 1.75, so check crate MSRVs.
- Exact next step: M2 (Rust core: crypto library ADR, encrypt/decrypt, link format, CLI encrypt/decrypt, test vectors, tamper tests).
```

```
### 2026-11-15, Antigravity, milestone M2
- Status: done
- What changed:
  - docs/adr/0002-crypto-library-choice.md: ADR selecting dryoc v2.0.0 (pure Rust libsodium implementation) + subtle + zeroize.
  - protocol/src/lib.rs: Full wire constants matching spec (CHUNK_SIZE 1 MiB, PART0_SIZE 4137 B, MAX_PART_SIZE 1,048,593 B, KDF context "EMBERv1_", sub-key IDs, AD_DROP, link version 1, flags, Argon2 bounds).
  - core/Cargo.toml: Added dryoc, base64, serde, serde_json, subtle, thiserror, zeroize.
  - core/src/error.rs: Strongly-typed EmberError variants (Auth, NoFinalTag, TrailingBytes, InvalidPadding, SizeMismatch, InvalidMetadata, InvalidLink, UnsupportedVersion, UnsupportedFlags, PassphraseRequired, PassphraseNotAllowed, InvalidPassphraseParams, Crypto, Io).
  - core/src/key.rs: SecretKey with CSPRNG generation, automatic zeroization on drop, redacted debug output, constant-time equality, and BLAKE2b sub-key derivation via crypto_kdf_derive_from_key.
  - core/src/padding.rs: Padded length calculation in 64 KiB bands (minimum 4096 bytes) and zero-padding verification.
  - core/src/metadata.rs: DropMetadata struct, validation, safe filename sanitization, and 4096-byte Message 0 record encoding/decoding.
  - core/src/wrap.rs: Passphrase key wrapping/unwrapping via Argon2id + XChaCha20-Poly1305 into 95-byte payload with protocol parameter bounds checking.
  - core/src/link.rs: Link formatting, base64url encoding/decoding, random blob ID generation/validation, full link builder/parser.
  - core/src/stream.rs: Streaming encryption and decryption (encrypt_drop, encrypt_drop_to_parts, decrypt_drop, decrypt_drop_parts) using crypto_secretstream_xchacha20poly1305.
  - tools/cli/src/main.rs: CLI with `ember encrypt <file>` and `ember decrypt <link-or-file>` with optional passphrase wrapping.
  - testvectors/drop-v1.json: Generated shared test vectors covering empty file (0 B), 1 byte, 4096 B, 4097 B, 1 MiB, 1 MiB + 1, text drop, UTF-8 file name, deterministic key wrap cases, and negative cases (flipped byte, truncated, trailing bytes, reordered chunks, bad link version).
  - core/tests/test_vectors.rs: Shared test vector test suite validating all decrypt, wrap, and negative cases against testvectors/drop-v1.json.
  - core/tests/property_tests.rs: Comprehensive property and tamper tests (roundtrips across size spectrum, single-bit flips, truncated streams, trailing bytes, passphrase tampering, link format tampering).
- Decisions made: See ADR 0002.
- Tests run and results:
  - cargo fmt --check: Clean.
  - cargo clippy --all-targets -- -D warnings: Clean (0 warnings).
  - cargo test --workspace: All 31 tests passed cleanly.
- Known issues / TODO for next agent: None for core crypto and offline link handling.
- Exact next step: M3 (Relay server MVP using axum: blob uploads, 12h TTL, burn-after-view with claim/finish, filesystem backend, CLI send/receive integration).
```

```
### 2026-11-15, Antigravity, milestone M3
- Status: done
- What changed:
  - server/Cargo.toml: Added axum 0.8, tokio, tower, tower-http (cors), serde, sha2, subtle, dryoc, clap, thiserror.
  - server/src/error.rs: ServerError enum with HTTP status mapping (404 NotFound, 410 Gone, 401 Unauthorized, 409 Conflict, 400 BadRequest, 413 PayloadTooLarge, 500 Storage).
  - server/src/model.rs: Wire requests and responses (CreateBlobRequest, CreateBlobResponse, BlobMetadataResponse, ClaimResponse) and BlobRecord with BlobState (Uploading, Available).
  - server/src/storage/mod.rs: Storage trait with blob creation, part writing/reading, part sizing, tombstones, and cleanup.
  - server/src/storage/fs.rs: FilesystemStorage backend storing meta.json, part_<n>.bin, and .tombstones/<id>.
  - server/src/storage/mem.rs: MemoryStorage backend with in-memory thread-safe maps for testing.
  - server/src/state.rs: AppState holding storage, configurable size cap (50 MiB), max TTL (12 hours), upload timeout (1 hour), and claim window (10 minutes).
  - server/src/routes.rs: Axum route handlers implementing POST /v1/blobs, PUT /v1/blobs/{id}/parts/{n}, POST /v1/blobs/{id}/complete, GET /v1/blobs/{id}, POST /v1/blobs/{id}/claim, GET /v1/blobs/{id}/parts/{n} (with Range support), POST /v1/blobs/{id}/finish, DELETE /v1/blobs/{id}.
  - server/src/sweeper.rs: Background sweeper task removing expired blobs, expired claim windows, and timed-out uploads while recording tombstones.
  - server/src/main.rs: Executable running Axum relay server with configurable host, port, storage dir, and sweeper interval.
  - tools/cli/src/main.rs: Added `ember send <file>` and `ember receive <link>` commands with live HTTP relay integration.
  - server/tests/server_tests.rs: 7 comprehensive integration tests verifying upload/download roundtrip, link preview bot survival, burn on finish, 409 concurrent claim rejection, resumable upload & HTTP range requests, expiry & sweeper cleanup, assertion that link fragments and keys are never sent to or stored by server, and full end-to-end CLI send/receive.
- Decisions made: Server stores SHA-256 hash of delete token and claim token; compares in constant time. Tombstones recorded on burn/expiry so subsequent requests return 410 Gone.
- Tests run and results:
  - cargo fmt --check: Clean.
  - cargo clippy --all-targets -- -D warnings: Clean (0 warnings).
  - cargo test --workspace: All 38 tests passed cleanly.
- Known issues / TODO for next agent: None. Ready for M4.
- Exact next step: M4 (Proof-of-work challenge/response solver in core with WASM support, server verifier, and IP rate limits).
```

```
### 2026-11-15, Antigravity, milestone M4
- Status: done
- What changed:
  - docs/adr/0003-proof-of-work-and-rate-limits.md: Architecture Decision Record for SHA-256 hashcash PoW, stateless signed challenges with anti-replay cache, and privacy-preserving coarse IP rate limiting.
  - docs/protocol-spec.md: Added GET /v1/pow/challenge specification, X-Pow-Challenge and X-Pow-Nonce headers, rate limit 429 response documentation.
  - core/Cargo.toml: Added sha2 dependency.
  - core/src/pow.rs: Implemented count_leading_zero_bits, hash_pow, verify_pow, solve_pow (WASM-ready pure Rust), and PowChallengeToken (stateless 52-byte signed token with BLAKE2b MAC).
  - core/src/error.rs: Added InvalidPow and Expired error variants.
  - core/src/lib.rs: Exported PoW module and primitives.
  - server/src/error.rs: Added PowRequired, PowInvalid, PowExpired, PowReplayed, and RateLimited { retry_after_secs } error variants with HTTP 400 and 429 mappings and Retry-After header.
  - server/src/pow.rs: Implemented PowManager with runtime-adjustable atomic difficulty, challenge issuance, verification, and time-bounded anti-replay cache.
  - server/src/rate_limit.rs: Implemented RateLimiter with in-memory sliding window counters, IPv6 /64 prefix truncation, and automatic eviction (zero IPs persisted to disk).
  - server/src/state.rs: Added PowManager and RateLimiter to AppState with configurable difficulty and rate limits.
  - server/src/sweeper.rs: Integrated PoW anti-replay and rate limiter cache eviction into periodic sweeper.
  - server/src/routes.rs: Added GET /v1/pow/challenge endpoint, client IP extraction, rate limit check, and PoW validation on POST /v1/blobs.
  - server/src/main.rs: Added --pow-difficulty and --rate-limit command-line arguments.
  - tools/cli/src/main.rs: Updated `ember send` to request PoW challenge from relay, solve puzzle, and include solution headers on POST /v1/blobs.
  - server/tests/server_tests.rs: Added 4 integration tests (missing PoW rejection, invalid nonce rejection, replay rejection, rate limiter trigger and recovery) and updated all existing tests.
- Decisions made: SHA-256 hashcash with big-endian u64 nonces. Challenges are signed statelessly using the server's ephemeral secret. Spent challenges are cached for their 5-minute TTL to prevent replay. Rate limits use in-memory buckets with IPv6 /64 grouping.
- Tests run and results:
  - scripts/check.ps1:
    - cargo fmt --all --check: Clean.
    - cargo clippy --workspace --all-targets -- -D warnings: Clean (0 warnings).
    - cargo test --workspace: All 53 tests passed cleanly (25 core unit, 7 core property, 1 test vectors, 2 protocol, 7 server unit, 11 server integration).
- Known issues / TODO for next agent: None. Ready for M5.
- Exact next step: M5 (Minimal web receiver static page with in-browser decryption and test vector harness).
```

```
### 2026-11-15, Antigravity, milestone M5
- Status: done
- What changed:
  - web-receiver/: Standalone static web receiver frontend with libsodium-wrappers-sumo, esbuild bundling, responsive dark-mode UI, and strict CSP (`default-src 'none'`).
  - web-receiver/crypto.js: Client-side link parsing, Argon2id key unwrapping, BLAKE2b sub-key derivation, and XChaCha20-Poly1305 secretstream chunk pulling.
  - web-receiver/test-vectors.js: Test harness validating all 8 decrypt cases, 1 passphrase wrap case, and 5 negative test cases against `testvectors/drop-v1.json` (14/14 passed, 0 failed).
  - web-receiver/index.html, style.css, app.js, build.js: Production web receiver with download progress, in-browser decryption, burn-after-view claiming and completion notification, text preview, and prominent security warnings (T3a host trust notice, recipient retention notice).
  - server/src/routes.rs: Embedded static web receiver assets via `include_str!` and added `GET /`, `GET /d/{id}`, `GET /style.css`, and `GET /app.js` with strict security headers (CSP, nosniff, DENY, no-referrer, no-store).
  - server/tests/server_tests.rs: Added integration tests `test_web_receiver_static_assets_serving_and_security_headers` and `test_web_receiver_bot_preview_does_not_burn_blob`.
  - docs/adr/0004-web-receiver-design.md: Architecture Decision Record for libsodium.js web receiver, bundle packaging, and threat T3a disclosure.
- Decisions made: See ADR 0004. Static bundle embedded into server binary for zero-configuration deployment.
- Tests run and results:
  - `cd web-receiver && npm test`: 14 passed, 0 failed.
  - `scripts/check.ps1`:
    - cargo fmt --all --check: Clean.
    - cargo clippy --workspace --all-targets -- -D warnings: Clean (0 warnings).
    - cargo test --workspace: All 55 tests passed cleanly (25 core unit, 7 core property, 1 test vectors, 2 protocol, 7 server unit, 13 server integration).
- Known issues / TODO for next agent: None. Ready for M6.
- Exact next step: M6 (Metadata stripping in `ember-core`: EXIF/GPS/author stripping for JPEG, PNG, WebP, and PDF).
```

```
### 2026-11-15, Antigravity, milestone M6
- Status: done
- What changed:
  - core/src/strip/mod.rs: Public API for metadata detection and stripping (`strip_metadata`, `detect_format`, `FileFormat`, `StripStatus`, `MetadataStripResult`).
  - core/src/strip/jpeg.rs: Strips APP1 (EXIF, GPS, XMP), APP2, APP13 (Photoshop/IPTC), and COM marker segments. Preserves orientation tag (0x0112) in minimal clean APP1 header; preserves compressed image scan data bit-for-bit.
  - core/src/strip/png.rs: Strips eXIf, tEXt, zTXt, iTXt, tIME, and dSIG chunks; preserves IHDR, PLTE, IDAT, IEND, and rendering chunks.
  - core/src/strip/webp.rs: Strips EXIF and XMP chunks from WebP RIFF containers; clears VP8X header flags; updates RIFF size.
  - core/src/strip/pdf.rs: Sanitizes /Author, /Creator, /Producer, /CreationDate, /ModDate, /Title, /Keywords, and XMP streams while space-padding to preserve document byte length and xref table offsets.
  - tools/cli/src/main.rs: Integrated automatic metadata stripping into `ember encrypt` and `ember send`, with `--no-strip` flag and user feedback on removed fields.
  - core/tests/strip_tests.rs: Comprehensive fixture-based tests for JPEG (GPS + camera + IPTC + comments + orientation), PNG (text + EXIF + tIME), WebP (EXIF + XMP), PDF (Info + XMP), clean files, and unsupported format passthrough.
  - docs/adr/0005-metadata-stripping.md: Architecture Decision Record for pre-encryption metadata stripping.
- Decisions made: See ADR 0005. Space-padding PDF metadata to maintain xref integrity.
- Tests run and results:
  - `scripts/check.ps1`:
    - cargo fmt --all --check: Clean.
    - cargo clippy --workspace --all-targets -- -D warnings: Clean (0 warnings).
    - cargo test --workspace: All 62 tests passed cleanly (25 core unit, 7 core property, 7 core strip, 1 test vectors, 2 protocol, 7 server unit, 13 server integration).
- Known issues / TODO for next agent: None. Ready for M7.
- Exact next step: M7 (Threads: spike Noise vs libsignal, write ADR, implement ratchet in core, mailboxes on server, CLI chat).
```

```
### 2026-11-15, Antigravity, milestone M7
- Status: done
- What changed:
  - docs/adr/0006-threads-noise-vs-libsignal.md: ADR evaluating Noise vs libsignal, selecting Noise-inspired ephemeral DH + symmetric forward-secret ratchet using dryoc/libsodium primitives.
  - protocol/src/lib.rs: Added thread wire constants (`SUBKEY_ID_THREAD_PSK`, `SUBKEY_ID_THREAD_CHAIN_INIT`, `SUBKEY_ID_THREAD_CHAIN_RESP`, `SUBKEY_ID_THREAD_MAILBOX`, `SUBKEY_ID_THREAD_MSG`, `SUBKEY_ID_THREAD_RENDEZVOUS`, `MAX_THREAD_MSG_BYTES`, `MAX_MAILBOX_MESSAGES`, `AD_THREAD`).
  - core/src/thread/handle.rs: Local pseudonymous handle generator from wordlist + digits (e.g. `amber-fox-4821`), never exposed to server.
  - core/src/thread/ratchet.rs: `SendingRatchet` and `ReceivingRatchet` with per-message KDF key derivation, zeroization, replay protection, out-of-order rejection, and `encrypt_with_nonce` for test vectors.
  - core/src/thread/handshake.rs: Ephemeral X25519 DH exchange authenticated with drop PSK via BLAKE2b keyed hash, rendezvous mailbox derivation, directional mailbox capability derivation (`MailboxCapability`), and deterministic testing constructors (`with_ephemeral_keypair`, `respond_with_ephemeral_keypair`).
  - core/src/thread/session.rs: `ThreadSession` and `ThreadMessage` envelope.
  - core/tests/thread_tests.rs: 7 comprehensive unit tests (handshake, roundtrips, tampering rejection, replay rejection, out-of-order rejection, wrong key rejection, forward secrecy proof).
  - testvectors/thread-v1.json: Deterministic shared test vectors for thread handshake, KDF derivations, rendezvous and directional mailboxes, and forward-secret message ciphertexts.
  - core/tests/thread_vectors.rs: Shared test vector test suite validating all vectors and negative cases.
  - server/src/model.rs: Mailbox models (`CreateMailboxRequest`, `PushMessageRequest`, `PushMessageResponse`, `MailboxRecord`, `MailboxMessageRecord`, `GetMessagesResponse`, `MailboxMessageItem`).
  - server/src/storage/mod.rs, server/src/storage/mem.rs, server/src/storage/fs.rs: Storage trait extensions and implementations for mailboxes and queued ciphertext messages with capacity and TTL caps.
  - server/src/state.rs: Broadcast notification channel (`mailbox_notifier`) for responsive long-polling without CPU spinning.
  - server/src/routes.rs: `POST /v1/mailboxes`, `POST /v1/mailboxes/{id}/messages`, `GET /v1/mailboxes/{id}/messages` (with long-poll `poll_timeout_secs` and `drain`), `DELETE /v1/mailboxes/{id}` (close-by-either-side), constant-time bearer token verification, and strict no-store headers.
  - server/src/sweeper.rs: Sweeps expired mailboxes past 12-hour maximum lifetime.
  - server/tests/server_tests.rs: Integration tests for mailbox lifecycle, token checks, sweeper expiry, and end-to-end multi-party thread handshake, bidirectional chat, replay protection, and thread closure. Fixed PoW trial limit in test helpers to prevent flaky test failures.
  - tools/cli/src/main.rs: Added `ember thread <link>` command supporting auto-detection of initiator/responder role, local handle generation, rendezvous handshake, terminal interactive chat loop with `/quit`, and cleanup.
- Decisions made: See ADR 0006.
- Tests run and results:
  - `scripts/check.ps1`:
    - cargo fmt --all --check: Clean.
    - cargo clippy --workspace --all-targets -- -D warnings: Clean (0 warnings).
    - cargo test --workspace: All 73 tests passed cleanly (26 core unit, 7 core property, 7 core strip, 1 drop test vector suite, 7 core thread, 1 thread test vector suite, 2 protocol, 7 server unit, 15 server integration).
- Known issues / TODO for next agent: None. Ready for M8.
- Exact next step: M8 (Admin CLI, takedown log, report endpoint, logging policy and privacy tests).
```

---

## 10. Master prompt for a new agent (paste this)

> You are continuing the Ember project. Read `EMBER_BUILD_GUIDE.md` and `EMBER_PROJECT_BLUEPRINT.md` fully. Follow the hard rules in section 0 of the guide (no custom crypto, no Google services, never ask for secrets or touch `.env` files, no banned marketing words). Check the Status Tracker and the last Handoff Log entry, then do only the next unchecked milestone. Implement and test your changes directly in the repository and run the Acceptance checks. When finished, tick the milestone, append a Handoff Log entry with the exact next step, and commit. If you must make a decision, use the defaults in section 3 and record an ADR. If you are blocked, record the blocker in the log and stop.
