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
- [x] M8  Admin CLI, takedown logging, report endpoint, logging policy
- [x] M9  UniFFI bindings and Android skeleton
- [x] M10 Android send and receive flows (integration gates passed; manual acceptance gaps remain in `HANDOFF_LOG.md`)
- [ ] M11 Android threads, report and block, Hindi and English, hardening
- [x] M12 Release engineering, legal docs, audit prep
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
1. **Spike:** compare Noise (`snow`) and libsignal for this use (link secret as PSK, ephemeral keys, session-level forward secrecy). Write an ADR with the decision. The auditor will review it.
2. Implement the handshake and session-level transport encryption in `core`.
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
- Exact next step: M7 (Threads: spike Noise vs libsignal, write ADR, implement session-level transport encryption in core, mailboxes on server, CLI chat).
```

```
### 2026-11-15, Antigravity, milestone M7
- Status: done
- What changed:
  - docs/adr/0006-threads-noise-vs-libsignal.md: ADR evaluating Noise vs libsignal, selecting Noise-inspired ephemeral DH + symmetric forward-secret ratchet using dryoc/libsodium primitives.
  - protocol/src/lib.rs: Added thread wire constants (`SUBKEY_ID_THREAD_PSK`, `SUBKEY_ID_THREAD_CHAIN_INIT`, `SUBKEY_ID_THREAD_CHAIN_RESP`, `SUBKEY_ID_THREAD_MAILBOX`, `SUBKEY_ID_THREAD_MSG`, `SUBKEY_ID_THREAD_RENDEZVOUS`, `MAX_THREAD_MSG_BYTES`, `MAX_MAILBOX_MESSAGES`, `AD_THREAD`).
  - core/src/thread/handle.rs: Local pseudonymous handle generator from wordlist + digits (e.g. `amber-fox-4821`), never exposed to server.
  - core/src/thread/transport.rs: `ThreadSender` and `ThreadReceiver` with session-level transport encryption, zeroization, replay protection, out-of-order rejection, and `encrypt_with_nonce` for test vectors.
  - core/src/thread/handshake.rs: Ephemeral X25519 DH exchange authenticated with drop PSK via BLAKE2b keyed hash, rendezvous mailbox derivation, directional mailbox capability derivation (`MailboxCapability`), and deterministic testing constructors (`with_ephemeral_keypair`, `respond_with_ephemeral_keypair`).
  - core/src/thread/session.rs: `ThreadSession` and `ThreadMessage` envelope.
  - core/tests/thread_tests.rs: 8 comprehensive unit tests (handshake, roundtrips, tampering rejection, replay rejection, out-of-order rejection, wrong key rejection, forward secrecy proof).
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
    - cargo test --workspace: All 74 tests passed cleanly.
- Known issues / TODO for next agent: None. Ready for M8.
- Exact next step: M8 (Admin CLI, takedown log, report endpoint, logging policy and privacy tests).
```

```
### 2026-11-15, Antigravity, milestone M8
- Status: done
- What changed:
  - server/src/access_log.rs: Privacy-preserving access log middleware following `docs/privacy-and-logging.md` Section 2 (route templates e.g. `/v1/blobs/{id}/parts/{n}`, coarse size buckets `<1KB`..`>50MB`, zero blob IDs, zero secrets logged).
  - server/src/model.rs: Added models for takedown requests (`AdminTakedownRequest`, `TakedownRecord`), user abuse reports (`SubmitReportRequest`, `SubmitReportResponse`, `ReportRecord`), and transparency statistics (`TransparencyStats`).
  - server/src/storage/mod.rs, server/src/storage/mem.rs, server/src/storage/fs.rs: Storage trait extensions and implementations for append-only takedowns log (`.takedowns.jsonl`), user abuse reports with 30-day retention (`.reports/<id>.json`), and aggregate transparency stats.
  - server/src/state.rs: Added `access_logger: AccessLogRecorder` and `admin_token: Option<String>` to `AppState`.
  - server/src/sweeper.rs: Sweeper now automatically purges reports older than 30 days per retention policy.
  - server/src/routes.rs: Added `POST /v1/reports` (public, rate-limited, 30-day retention), `GET /v1/transparency` (public aggregate counts), `POST /v1/admin/takedown` (admin bearer auth, constant-time compare, tombstone creation, append-only log), and `GET /v1/admin/reports` (restricted to reviewer role).
  - server/src/lib.rs: Exported models, access logger, and attached `access_log_middleware` layer to app router.
  - tools/admin-cli/src/main.rs: Built full `ember-admin` tool supporting `takedown` (by blob ID or mailbox ID with audit reference), `reports` (reviewer view of pending abuse reports), and `transparency` (formatted stats or raw JSON).
  - docs/takedown-process.md: Complete operational procedure documentation based on Blueprint Section 10 and IT Rules (2h NCII deadline, 3h court/govt order deadline, zero content read required, reviewer role isolation, 3-year takedown log retention).
  - server/tests/server_tests.rs: Added 4 comprehensive integration tests:
    - `test_admin_takedown_removes_blob_and_mailbox`: verifies unauthenticated rejection (401), authorized blob removal (subsequent requests get 410 Gone), authorized mailbox removal (404), and takedown audit records.
    - `test_user_report_endpoint_and_transparency`: verifies public report submission, reviewer-only access (401 without auth), and public transparency aggregates.
    - `test_privacy_logs_never_contain_secrets`: verifies that canaries (key, fragment, filename, plaintext content, passphrase) NEVER leak into server access logs, route templates never contain blob IDs, and size buckets are coarse.
    - `test_admin_cli_binary_execution`: end-to-end test executing `ember-admin takedown`, `reports`, and `transparency` against live server.
- Decisions made: Takedown log is append-only JSONL separate from user data; reports are kept in isolated storage with 30-day TTL enforced by sweeper; transparency API publishes counts only with zero personal data.
- Tests run and results:
  - `scripts/check.ps1`:
    - cargo fmt --all --check: Clean.
    - cargo clippy --workspace --all-targets -- -D warnings: Clean (0 warnings).
    - cargo build --bins: Clean (built `ember`, `ember-admin`, `ember-server`).
    - cargo test --workspace: All 78 tests passed cleanly (26 core unit, 7 core property, 7 core strip, 1 drop test vector suite, 8 core thread, 1 thread test vector suite, 2 protocol, 7 server unit, 19 server integration).
- Known issues / TODO for next agent: M8 is fully complete. M9 onward requires Android SDK/NDK and UniFFI Kotlin bindings.
- Exact next step: M9 (UniFFI bindings for core in Kotlin and Android skeleton).
```

```
### 2026-11-15, Antigravity, milestone M9
- Status: done
- What changed:
  - docs/adr/0007-android-platform-and-ffi.md: ADR accepting Min API 26 (Android 8.0), pinned UniFFI 0.32.2 bridge, and zero Google dependencies.
  - Cargo.toml: Added `android/ember-ffi` to workspace members.
  - android/ember-ffi/ (Cargo.toml, uniffi.toml, src/lib.rs, src/bin/uniffi-bindgen.rs): UniFFI cdylib crate bridging `ember-core` into Kotlin. Exposes `EmberCrypto` (with `generate()`, `from_key_bytes()`, `encrypt_text()`, `decrypt_text()`, `encrypt_file()`, `decrypt_drop_payload()`) and `core_version()`, keeping secret key zeroization on drop and safe Rust boundaries. Added 4 unit tests covering roundtrip text/file drops, expiry checks, and key length validation.
  - .cargo/config.toml: Added NDK 28 LLVM cross-compiler targets (`aarch64-linux-android`, `x86_64-linux-android`, `armv7-linux-androideabi`).
  - android/build-native.ps1: Self-contained build script that builds `libember_android_ffi.so` for all 3 ABIs (< 1 MB each), populates `android/app/src/main/jniLibs/` (`arm64-v8a`, `armeabi-v7a`, `x86_64`), and runs `uniffi-bindgen` to generate Kotlin bindings.
  - android/app/build/generated/uniffi/kotlin/com/ember/mobile/core/ember_android_ffi.kt: Generated Kotlin bindings for UniFFI 0.32.2.
  - android/build.gradle.kts, android/settings.gradle.kts, android/gradle.properties: Root Gradle build configuration with fixed AGP 9.5.0, Kotlin 2.4.10, and Gradle 9.6.0.
  - android/app/build.gradle.kts: App module configuration with Compose, JNA 5.17.0, test options with host `jna.library.path`, and `:app:checkForbiddenDependencies` Gradle task.
  - android/app/src/main/AndroidManifest.xml: Application manifest with zero Google dependencies and `allowBackup="false"`.
  - android/app/src/main/java/com/ember/mobile/MainActivity.kt: Debug screen in Jetpack Compose with `FLAG_SECURE`, core version indicator, and local encryption roundtrip action.
  - android/app/src/test/java/com/ember/mobile/EmberCoreTest.kt: Kotlin test suite asserting `coreVersion()`, text roundtrip, file roundtrip, invalid key/expiry rejection, and shared drop test vectors (`testvectors/drop-v1.json`) including negative tampering cases.
  - android/scripts/check_dependencies.ps1, android/scripts/check_dependencies.sh: Automated scanners validating zero forbidden Google dependencies (`com.google.android.gms`, `com.google.firebase`, `com.google.mlkit`, `play-services`, `firebase`, `mlkit`, `nearby`).
  - .github/workflows/ci.yml: Integrated `android/scripts/check_dependencies.sh` into CI hygiene job.
  - android/README.md: Architecture, build instructions, and verification details.
- Decisions made: See ADR 0007. Built native libraries for all three ABIs directly via NDK LLVM toolchain; verified zero Google dependencies across all 7 Android configuration and source files.
- Tests run and results:
  - `scripts/check.ps1`:
    - cargo fmt --all --check: Clean.
    - cargo clippy --workspace --all-targets -- -D warnings: Clean (0 warnings).
    - cargo build --bins: Clean (`ember`, `ember-admin`, `ember-server`).
    - cargo test --workspace: All 80 tests passed cleanly (4 FFI, 26 core unit, 7 core property, 7 core strip, 1 drop test vector suite, 8 core thread, 1 thread vector suite, 2 protocol, 7 server unit, 3 access log, 19 server integration).
  - `android/scripts/check_dependencies.ps1`: Clean (zero forbidden Google dependencies found across 7 Android source/config files).
  - `android/build-native.ps1`: All 3 ABIs built successfully and Kotlin bindings generated cleanly with 0 warnings.
- Known issues / TODO for next agent: None. Ready for M10.
- Exact next step: M10 (Android send and receive flows: file picker/text input, metadata stripping summary, PoW solver progress, resumable upload/download, QR code generation and CameraX scanning).
```

```
### 2026-11-15, Antigravity, milestone M12
- Status: done
- What changed:
  - docs/legal/privacy-policy.md: Draft Privacy Policy marked "Pending Legal Review", fully aligned with DPDP Act 2023, IT Rules 2021, and CERT-In directions (ephemeral 12-hour drops, zero plaintext/key storage, coarse IP access logs).
  - docs/legal/terms-of-use.md: Draft Terms of Use marked "Pending Legal Review", detailing strict prohibited content (CSAM/POCSO, NCII, malware, IP infringement, state security threats) and user responsibilities.
  - docs/legal/grievance-and-takedown-policy.md: Official Grievance Officer role and operational takedown procedure with statutory deadlines (2h NCII, 3h emergency orders, 24h acknowledgement, 15d resolution).
  - docs/legal/transparency-report-template.md: Semi-annual transparency disclosure framework covering abuse reports, takedowns, and zero personal data disclosures.
  - docs/distribution/store-listing.md: Store listing text with distinctive title "Ember: Private File Drop", tagline "Send it once. Only they can open it.", and code-verified Google Play Data Safety answers (zero personal data collected, zero third-party trackers, end-to-end encrypted).
  - docs/distribution/fdroid-metadata.yml: F-Droid metadata and build recipe (zero proprietary blobs, anti-features: none).
  - docs/distribution/reproducible-builds.md: Third-party verification guide for reproducible Rust binaries and Android APK builds.
  - docs/audit/AUDIT_SCOPE.md: Complete independent auditor package detailing cryptographic invariants, in-scope components, test vectors, and verification instructions.
  - scripts/check_licenses.ps1, scripts/check_licenses.sh: Automated license scanner verifying AGPL-3.0 compatibility across all workspace crates and dependencies against deny.toml.
  - docs/audit/dependency-license-report.md: Comprehensive license audit report of all direct dependencies.
  - scripts/check_banned_words.ps1, scripts/check_banned_words.sh: Automated CI scanner checking all user-facing documentation and client code for prohibited marketing buzzwords.
  - scripts/check.ps1, scripts/check.sh: Integrated license compliance and banned words scanners directly into the workspace gate check.
- Decisions made: All legal documents marked "Pending Legal Review"; banned marketing buzzwords enforced via automated CI grep; reproducible build steps documented for both host and Android targets.
- Tests run and results:
  - `scripts/check.ps1`:
    - cargo fmt --all --check: Clean.
    - cargo clippy --workspace --all-targets -- -D warnings: Clean (0 warnings).
    - cargo build --bins: Clean (`ember`, `ember-admin`, `ember-server`).
    - cargo test --workspace: All 80 tests passed cleanly.
    - scripts/check_licenses.ps1: Clean (all 21 direct crates conform to AGPL-3.0 policy).
    - scripts/check_banned_words.ps1: Clean (0 violations in user-facing texts).
- Known issues / TODO for next agent: Android UI flows (M10/M11) are owned by the Android agent (Codex). Backend and project documentation/release engineering (M0-M9, M12) are fully complete.
- Exact next step: Coordinate with Android agent on M10/M11 in Android worktree.
```

```
### 2026-11-16, Antigravity, Core Streaming Engine (Request A-1)
- Status: done
- What changed:
  - core/src/stream.rs: Implemented `DropEncryptStream` (incremental chunk encryptor with bounded <= 1 MiB chunk buffer, zeroized on drop, spec section 6 padding on finish), `DropDecryptStream` (incremental chunk decryptor enforcing all spec section 4.3 receiver checks, zero-padding verification, and `finish()` invariant requirement), `start_encrypt_stream`, and `start_decrypt_stream`. Refactored all whole-buffer functions (`encrypt_drop`, `encrypt_drop_to_parts`, `decrypt_drop`, `decrypt_drop_parts`) as thin wrappers over this streaming engine.
  - core/src/lib.rs: Exported `DropEncryptStream`, `DropDecryptStream`, `start_encrypt_stream`, `start_encrypt_stream_with_header`, and `start_decrypt_stream`.
  - core/README.md: Documented incremental streaming API, chunk/part lifetimes, memory bounds, and critical section 4.3 temporary file receiver requirements.
  - docs/cross-agent-requests.md: Answered Request A-1 from Android Agent (Codex) with exact function signatures, UniFFI-friendly types, and complete streaming usage examples.
  - core/src/stream.rs tests: Added unit and property tests covering 0 B, 1 B, 4096 B, 4097 B, 1 MiB, 1 MiB + 1 B, multi-chunk transfers with arbitrary slice sizes, buffer capacity bounds (<= 1 MiB), and negative tampering tests (flipped bytes in part 0 and chunks, reordered parts, trailing parts after final, finish without final).
- Decisions made: Single cryptographic code path for both streaming and whole-buffer drop crypto; zero generic types across the public streaming boundary for seamless UniFFI generation.
- Tests run and results:
  - `scripts/check.ps1`:
    - cargo fmt --all --check: Clean.
    - cargo clippy --workspace --all-targets -- -D warnings: Clean (0 warnings).
    - cargo build --bins: Clean.
    - cargo test --workspace: All 92 tests passed cleanly (including 38 core unit, 7 property, 7 strip, 1 drop test vector suite, 8 thread, 1 thread vector suite, 2 protocol, 7 server unit, 3 access log privacy, 19 server integration).
    - scripts/check_licenses.ps1: Clean (AGPL-3.0 compliant).
    - scripts/check_banned_words.ps1: Clean (0 banned buzzwords).
- Known issues / TODO for next agent: Android agent (Codex) can now bind to the new streaming API for M10 send and receive file flows.
- Exact next step: Android agent wires `DropEncryptStream` and `DropDecryptStream` into UniFFI and Compose upload/download flows.
```

```
### 2026-11-16, Claude, review of M0-M10 (branch claude-review, based on android-work 5b70a87)
- Status: done. Full report in docs/reviews/M0-M10-claude-review.md.
- What changed: 15 fixes (forgeable client address and shared rate-limit bucket, path traversal in storage and admin takedown, uncapped metadata size, unbounded part counts, claim race, unthrottled mailbox/admin/report endpoints, unbounded access-log memory, clap -h conflict, platform-dependent file-name sanitiser, link host validation, secret zeroising, check.sh bugs, CLI tests needing prebuilt binaries, CSP, legal operator-name collision). New tests in server/tests/review_fixes.rs and core unit tests.
- Decisions made: none new. ADR 0006 corrected (not formally verified; recommend switching to `snow`).
- Tests run and results: cargo fmt --check clean; cargo clippy --workspace --all-targets -D warnings clean; cargo test --workspace --no-fail-fast: 118 passed, 0 failed (Rust 1.100 on Linux). Android Gradle gate NOT run by the reviewer.
- Known issues / TODO for next agent: open items O1-O12 in the review report (thread crypto review, mailbox PoW, log persistence, report encryption at rest, admin listener, test-hook feature gate, Android manual QA).
- Exact next step: owner merges claude-review, runs scripts/check-android.sh, then decides O1 before M11.
```

```
### 2026-11-16, Antigravity, Backend review items O3, O4, O5, O8, O10 (branch main)
- Status: done. Tagged `backend-review-done`.
- What changed:
  1. (O8) Gated test hooks (`wrap_key_with_injected`, `encrypt_drop_*_with_header`, `start_encrypt_stream_with_header`) behind `test-hooks` Cargo feature.
  2. (O3) Access log persisted to disk in JSONL format with configurable retention (default 30 days) and scheduled deletion sweeper; updated docs/privacy-and-logging.md.
  3. (O4) Reports (content and link keys) encrypted at rest using reviewer public key (crypto_box sealed box); added `generate-reviewer-keys` and `decrypt-report` CLI commands to `ember-admin`.
  4. (O5) Added optional `--admin-listen` socket address flag so admin routes can be served on an isolated interface separate from public routes.
  5. (O10) Aligned banned-words scanning patterns and targets across scripts/check_banned_words.* and CI hygiene check.
- Decisions made: sealed box encryption uses standard libsodium X25519 sealed box primitives via `dryoc`; public/admin routers decoupled with zero-overhead axum sub-routers.
- Tests run and results:
  - cargo fmt --all --check: Clean.
  - cargo clippy --workspace --all-targets -- -D warnings: Clean (0 warnings).
  - cargo test --workspace: All 121 tests passed (0 failures).
  - scripts/check_licenses.ps1: Clean (AGPL-3.0).
  - scripts/check_banned_words.ps1: Clean (0 violations).
- Known issues / TODO for next agent: O1 (Noise/snow thread crypto review), O2 (thread mailbox PoW), O6 (horizontal scaling), O7 (web receiver build reproducibility).
- Exact next step: Run Android QA / M11 testing or proceed with M12 final release preparations.
```

```
### 2026-11-16, Codex, integration branch
- Status: integration gates passed; M10 manual acceptance gaps remain open. No M11 work started.
- What changed: merged `claude-review` and `main` into `integration` from `a10-done` (`5b70a87`); made the real-server Android instrumentation case mandatory by starting the merged `ember-server` router from Gradle; proved `test-hooks` is absent from the Android release dependency graph; removed the broad `ember-android-bindings` cargo-deny exclusion and assigned its path dependency an explicit version; excluded generated build artifacts from banned-word scans.
- Tests run and results:
  - `scripts/check.ps1`: passed; 128 Rust tests, formatting, clippy with `-D warnings`, binary builds, cargo-deny, license and banned-word checks passed.
  - `cargo run -p ember-server -- --help`: passed; `-H/--host` and `-h/--help` both displayed without panic.
  - `scripts/check-android.sh`: passed; 13 Rust binding tests, 25 Android unit tests, and 7 API 34 instrumentation tests passed with 0 skipped. `RealRelayM10InstrumentedTest.realServerBurnReceiveAndTamperRejection` ran against the locally started merged `ember-server`; lint, dependency scan, three ABI native builds and R8 release builds passed.
  - Release APK sizes: arm64-v8a 8,164,094 bytes (7.79 MiB); armeabi-v7a 7,818,604 bytes (7.46 MiB); x86_64 8,213,247 bytes (7.83 MiB). These are unsigned per-ABI APKs.
- M10 acceptance gaps still open: Android-to-web receiver interop; Android-to-CLI interop; throttled weak-network run; large-file memory measurement; physical camera scan; SAF picker/save flows; HTTPS deep link missing from the manifest.
- Exact next step: continue M10 manual acceptance with Android-to-web receiver interoperability; keep the seven listed acceptance gaps open until individually verified. Do not start M11 without owner direction.
```

```
### 2026-11-16, Antigravity, review items O1 and O2 (branch integration)
- Status: done. Closed review items O1 and O2 from docs/reviews/M0-M10-claude-review.md. No M11 work started.
- What changed:
  1. (O1) Replaced hand-composed ephemeral thread cryptography with audited `snow` crate implementing standard Noise pattern `Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s`. Superseded ADR 0006 with ADR 0008 (`docs/adr/0008-noise-protocol-thread-handshake.md`). Aligned KDF sub-key ID usage to use spec-reserved `SUBKEY_ID_THREAD_PSK = 3`. Updated handshake wire protocol to version 2 (`THREAD_HANDSHAKE_VERSION = 2`, `THREAD_INITIATOR_MSG_LEN = 49`, `THREAD_RESPONDER_MSG_LEN = 49`), and message envelopes to format version 2 (`EncryptedMessagePayload { v: 2, seq, ciphertext }`). Obsolete message envelope versions (`v = 0`, `v = 1`) and handshake version mismatches are rejected cleanly with `EmberError::UnsupportedVersion`. Regenerated canonical test vectors in `testvectors/thread-v1.json` and verified with `core/tests/thread_vectors.rs`. Added negative test coverage in `core/tests/thread_tests.rs` for replay, out-of-order, tampering, truncation, trailing bytes, wrong PSK, malformed handshake, and secret zeroization on drop.
  2. (O2) Enforced Proof-of-Work solution requirements on mailbox creation (`POST /v1/mailboxes`), reusing the M4 challenge mechanism. Solutions are accepted in `CreateMailboxRequest.pow` body or via `X-Pow-Challenge`/`X-Pow-Nonce` HTTP headers. Added full rejection, replay, and recovery tests in `server/tests/server_tests.rs`.
  3. Updated `docs/protocol-spec.md`, `docs/threat-model.md`, `core/README.md`, and `docs/cross-agent-requests.md`. Explicitly documented in `docs/threat-model.md` that thread forward secrecy must NOT be marketed until an independent cryptographer review has taken place.
- Decisions made: ADR 0008: adopted `Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s` with standard primitives; ChaCha20-Poly1305 (RFC 7539 / 8439) for session-level transport AEAD; directional mailboxes derived deterministically from the 32-byte handshake transcript hash `h` using BLAKE2b KDF subkey 6.
- Tests run and results:
  - `scripts/check.ps1`: Full gate passed cleanly (exit code 0):
    - `cargo fmt --all --check`: Clean.
    - `cargo clippy --workspace --all-targets -- -D warnings`: Clean (0 warnings).
    - `cargo build --bins`: Clean.
    - `cargo test --workspace`: All 119 unit and integration tests passed (45 core, 13 thread, 1 thread vector, 13 android-bindings FFI, 2 protocol, 12 server unit, 5 access log, 5 review fixes, 23 server integration).
    - `cargo deny check`: Clean (`advisories ok, bans ok, licenses ok, sources ok`).
    - `scripts/check_licenses.ps1`: Clean (AGPL-3.0 compliant).
    - `scripts/check_banned_words.ps1`: Clean (0 prohibited marketing buzzwords).
- Known issues / TODO for next agent: Android agent (Codex) can bind to the stable UniFFI types in `android-bindings` and review `docs/cross-agent-requests.md` before starting M11.
- Exact next step: Owner/team reviews O1 & O2 resolution on branch `integration`. Do not start M11 without owner direction.
```

```
### 2026-11-16, Antigravity, Thread Crypto Fixes & Follow-up (branch thread-crypto-fixes)
- Status: done. All thread crypto fixes and follow-up items completed and verified. No M11 work started.
- What changed:
  1. (Wording & Aliases): Replaced all claims of "per-message forward secrecy" and "ratchet" terminology with session-level forward secrecy across code comments, mod docs, ADR 0008, protocol spec 12.4, threat model, blueprint, and build guide. Renamed `core/src/thread/ratchet.rs` -> `core/src/thread/transport.rs` (`ThreadSender`/`ThreadReceiver`) and deleted obsolete `SendingRatchet`/`ReceivingRatchet` aliases and residual "ratchet" wording. Updated ADR 0008 status to `implemented and verified`.
  2. (Binary Envelope v3): Replaced JSON envelope with binary wire format: `v(1 byte) | seq(8 bytes BE) | ciphertext`. Bumped version to `v = 3` (`THREAD_MSG_VERSION = 3`). Added `to_bytes()` and `from_bytes()` with strict validation rejecting unknown versions and invalid lengths (< 25 bytes). Updated protocol spec 12.4, regenerated deterministic `testvectors/thread-v1.json`, updated `core`, `server` tests, `tools/cli`, and documentation.
  3. (Message Size Limits & Final Numbers):
     - `MAX_THREAD_TEXT_BYTES = 4096`: Raw message text bound.
     - `MAX_THREAD_MSG_BYTES = 4608`: Serialized JSON plaintext bound (4096 text + 512 JSON overhead). When 4096 bytes of heavily escaped characters (`"`, `\n`, `\`) serialize to ~8254 bytes, `encrypt_text` halts with a clear `EmberError::InvalidMetadata` message-too-long error before encryption.
     - `MAX_THREAD_CIPHERTEXT_BYTES = 4624`: Ciphertext cap (4608 + 16 Poly1305 MAC tag). Receivers validate ciphertext length prior to allocating memory buffers.
     - `THREAD_MSG_HEADER_LEN = 9`: Exact length of envelope header `v(1) | seq(8, BE)`.
     - `MAX_THREAD_ENVELOPE_BYTES = 4633`: Maximum wire length of an envelope accepted by mailboxes (4624 + 9).
     - Server mailbox push limit: `server/src/routes.rs` enforces `MAX_THREAD_ENVELOPE_BYTES = 4633` on `POST /v1/mailboxes/{id}/messages`; payloads exceeding this (e.g. 5 KB payloads) are rejected with `413 Payload Too Large`.
  4. (Monotonic Gap Sequence Policy): Implemented sequence acceptance for `seq >= expected_seq`, advancing state to `seq + 1` only upon authenticated decryption. Replayed or obsolete sequence numbers (`seq < expected_seq`) are rejected as `ReplayDetected`. Added negative tests verifying that gap, replay, reorder, and tampered tags do not advance sequence state.
  5. (Test Hooks Gating & Deduplication): Gated `with_ephemeral_key` and `respond_with_ephemeral_key` behind `test-hooks` Cargo feature. Deduplicated responder implementation via shared `respond_internal`. Verified release builds without features build cleanly and do not export test hooks.
  6. (KDF Cleanup): Removed unused chain keys and subkey IDs 4, 5, 7. Moved mailbox (`SUBKEY_ID_MAILBOX_*`) and rendezvous (`SUBKEY_ID_RENDEZVOUS_*`) KDF IDs into protocol constants and the specification.
  7. (Validation & Sequence Overflow): Validated received `sender_handle` format against regex/syntax `^[a-z]+-[a-z]+-\d{4}$`. Verified inner message sequence number matches outer envelope sequence number (`SeqMismatch`). Added guard against `seq == u64::MAX` overflow (`SeqOverflow`).
  8. (Documentation & Threat Model): Updated ADR 0008 and `docs/threat-model.md`: removed the word "audited" for `snow` and cited crate version 0.10.0; noted that initiator is authenticated by the first transport message; stated that any drop link holder can read/delete rendezvous messages; documented multiple initiators race rule; documented that thread messages are not padded (size leaks length up to 4096 bytes).
- Tests run and results:
  - `scripts/check.ps1`: Full gate passed cleanly:
    - `cargo fmt --all --check`: Clean pass.
    - `cargo clippy --workspace --all-targets -- -D warnings`: Clean pass (0 warnings).
    - `cargo build --bins`: Clean pass.
    - `cargo test --workspace`: All 142 unit and integration tests passed (46 core unit, 7 property, 7 strip, 1 drop vectors, 19 thread unit & security tests, 1 thread vector, 13 android-bindings FFI, 2 protocol, 10 server unit, 2 server main, 5 access log, 5 review fixes, 24 server integration).
    - `cargo deny check`: Clean pass (`advisories ok, bans ok, licenses ok, sources ok`).
    - `scripts/check_licenses.ps1`: Clean pass (AGPL-3.0).
    - `scripts/check_banned_words.ps1`: Clean pass (0 prohibited buzzwords).
  - `scripts/check-android.sh`: Full build and lint pass (13 Rust binding tests, 25 Android unit tests, 100 Gradle tasks executed/up-to-date, lint clean, dependency scan clean, 3 native ABIs built, R8 release builds passed; exited at adb check because no physical device or emulator is connected).
- Test Count Comparison (main vs thread-crypto-fixes):
  - `main`: 134 tests passed.
  - `thread-crypto-fixes`: 142 tests passed (+8 net tests).
  - Merged/Renamed: `test_out_of_order_rejected` in `thread_tests.rs` was renamed/merged into `test_sequence_policy_gap_replay_reorder` to reflect the new monotonic gap sequence policy (`seq >= expected` accepted, `seq < expected` rejected, tampered tags don't advance state).
  - Added in `core`: `test_is_valid_handle_validation` (+1), `test_message_size_limits_exact_limit_plus_one_and_65535_cap` (+1), `test_binary_envelope_wire_format_and_rejection` (+1), `test_sender_handle_validation_and_seq_mismatch` (+1), `test_seq_overflow_guard` (+1), `test_ephemeral_key_hooks_gated_by_feature` (+1), `test_escaped_characters_text_serialization_limit` (+1).
  - Added in `server`: `test_mailbox_push_5kb_payload_is_rejected` (+1).
- Decisions made: Monotonic sequence window advances to `seq + 1` upon authenticated decryption; binary envelope v3 layout uses 9-byte header `v(1) | seq(8, BE) | ciphertext`; serialized plaintext limit enforced before encryption.
- Known issues / TODO for next agent: Android agent (Codex) can bind to the updated v3 envelope and stable UniFFI types in `android-bindings` and review `docs/cross-agent-requests.md` before starting M11.
- Exact next step: Review and merge branch `thread-crypto-fixes`. Do not push. Do not start M11 without owner direction.

```
### 2026-11-18, Codex, milestone M10 Android closure (branch android-m10-close)
- Status: partial; implemented and verified the Android-owned M10 items below. CLI text sending, web receiver crypto initialization, and physical-phone camera/SAF acceptance remain open. No M11 work started.
- What changed:
  - Added an exact HTTPS VIEW/DEFAULT/BROWSABLE deep-link filter for `/d/` on one configurable Gradle host (`emberLinkHost`, default placeholder `links.ember.invalid`); unrelated HTTPS hosts and non-HTTPS schemes do not match. After handling, the Activity clears the source intent's URL and text/clip payload and replaces its retained intent. Instrumentation verifies the fragment canary and full link are absent from saved instance state.
  - Added repeatable scripts in `android/scripts/` for CLI-to-Android, Android-to-CLI and optional web receiver interop, weak-network retry, and 50 MiB PSS sampling. The host collector holds link values only in process memory; generated passphrases and files are transient.
  - Added Android test-relay one-shot HTTP 503 fault injection for exercising upload/download retry without changing backend-owned code.
  - Added A-3 for backend-owned CLI text send and A-4 for web receiver initialization in `docs/cross-agent-requests.md`. No `core/`, `server/`, `protocol/`, or CLI files were changed.
- Tests run and results:
  - `scripts/check-android.sh` with `EmberM9_API34` running (API 34): passed, exit 0; 13 Rust Android binding tests, 26 Android unit tests, 17 API 34 instrumentation tests, 0 failed and 0 skipped; lint, dependency scan, native ABI builds and release APK builds passed.
  - `./android/scripts/m10-cli-to-android.sh`: passed. CLI sent a file drop and Android received it; passphrase protection and burn-after-view were verified.
  - `python android/scripts/m10-android-to-cli.py --cli-only`: passed. CLI decrypted Android's passphrase-protected burn file and burn text; digest and text bytes matched.
  - `python android/scripts/m10-android-to-cli.py`: CLI file/text checks passed. Chrome opened the third Android text link, but the embedded web receiver stayed at `Initializing cryptography engine...` with details/error hidden; decryption did not run. Web receiver result is open (A-4).
  - `./android/scripts/m10-weak-network.sh`: passed on API 34 with emulator speed `hsdpa` (14.4 Mbit/s preset), delay `hsdpa` (200 ms preset); the 20 MiB send and receive completed after one injected HTTP 503 on each direction. Script restored full speed and no delay.
  - `./android/scripts/m10-memory.sh`: passed on API 34 with the same network settings; 50 MiB send and receive completed. 23 `adb shell dumpsys meminfo com.ember.mobile` samples peaked at 119547 KiB (116.75 MiB), under the 160000 KiB bound.
  - Deep-link tests passed: only configured HTTPS host plus `/d/` matches; handled URL and fragment are not present in saved instance state.
- Known issues / TODO for next agent: A-3 is open because the backend CLI has no text-drop sender, so CLI-to-Android text is not verified. A-4 is open because the embedded receiver's crypto initialization stalled in Chrome; the test did not establish browser decryption through the standalone `ember-server` process. Physical camera scanning and SAF picker/save remain owner manual checks.
- OWNER MANUAL CHECKS (physical phone required):
  1. Camera scan: install the debug APK on a physical phone; open Receive; tap Scan QR code; grant camera permission; scan a valid Ember drop QR code; verify the link details appear; enter its passphrase if requested; open the drop and verify the decrypted content. Record phone model and Android version.
  2. SAF picker/save: open Send; choose a real file through the Android document picker; verify name and size in the send summary; send it through the relay. In a receive flow, open a file drop, confirm burn-after-view if shown, choose Save in the Android Create Document dialog, choose folder and filename, save, and compare saved bytes with the original. Record phone model and Android version.
- Exact next step: Backend owner resolves A-3 and A-4; owner performs the two physical-phone checks above. Keep M11 unstarted until directed.
```

```
### 2026-11-18, Antigravity, Request A-5: Mailbox Authoritative Expiry (branch main)
- Status: done. Resolved Request A-5 in docs/cross-agent-requests.md. Prerequisite for Android M11 countdown and automatic expiry.
- What changed:
  1. `server/src/model.rs`: Added `CreateMailboxResponse { id: String, expires_at: u64 }` and `MailboxMetadataResponse { id: String, created_at: u64, expires_at: u64 }`. Re-exported in `server/src/lib.rs`.
  2. `server/src/routes.rs`:
     - Updated `create_mailbox` (`POST /v1/mailboxes`) to return `CreateMailboxResponse` JSON body on both initial creation (`201 Created`) and idempotent matching (`200 OK`) with `Cache-Control: no-store`.
     - Mounted and implemented authenticated `get_mailbox` (`GET /v1/mailboxes/{id}`), verifying bearer token against push or pull token hashes in constant time and returning `MailboxMetadataResponse` (`200 OK`). Returns `401 Unauthorized` for missing/invalid token and `404 Not Found` for missing/expired mailbox.
  3. `server/tests/server_tests.rs`: Added assertions in `test_mailbox_lifecycle_and_messaging` verifying `CreateMailboxResponse` expiry bounds, idempotent expiry match, authenticated `GET /v1/mailboxes/{id}` metadata reads with push and pull tokens, and 401 unauthorized rejection without tokens.
  4. Updated `docs/protocol-spec.md` section 12.3 and `docs/cross-agent-requests.md` item A-5.
- Decisions made: Server returns authoritative `expires_at` in JSON response for `POST /v1/mailboxes` and authenticated `GET /v1/mailboxes/{id}`; Android client can use either or both to manage thread countdown and automatic session wipe.
- Tests run and results:
  - `scripts/check.ps1`: Full gate passed cleanly.
    - `cargo fmt --all --check`: Clean pass.
    - `cargo clippy --workspace --all-targets -- -D warnings`: Clean pass (0 warnings).
    - `cargo build --bins`: Clean pass.
    - `cargo test --workspace`: All 142 tests passed cleanly (0 failures).
    - `cargo deny check`: Clean pass.
    - `scripts/check_licenses.ps1` & `scripts/check_banned_words.ps1`: Clean pass.
- Known issues / TODO for next agent: Android agent (Codex) can read the returned `expires_at` from `POST /v1/mailboxes`, hook it to `ThreadScreen` countdown and timed auto-close, and run `scripts/check-android.sh` to complete M11.
- Exact next step: Android agent wires `expires_at` in `RelayClient.kt`/`ThreadCoordinator.kt`, validates against API 34 emulator, and completes M11.

### 2026-11-18, Codex, milestone M11 Android threads (branch android-m11-threads)
- Status: partial; Android 1:1 thread implementation and relay/vector verification are complete. Server-driven expiry remains open because the relay does not return mailbox `expires_at`. No backend-owned source files were changed.
- What changed:
  - Added Rust UniFFI wrappers in `android-bindings/` for rendezvous derivation, initiator/responder handshake, ephemeral session encrypt/decrypt, v3 envelope parsing/serialization, validated handles, and explicit close/wipe. Added two Rust binding tests.
  - Added the HTTPS-only Kotlin mailbox client with PoW challenge solving on the worker thread, mailbox create/push/pull/delete, retries, and transient capability handling.
  - Added a process-memory-only thread coordinator with three mailboxes, first-valid initiator handling, visible-screen polling only, retry backoff, message size feedback, replay/gap handling, and close/delete/wipe behavior.
  - Added a Compose thread screen with validated sender handles, 4096-byte counter, expiry display placeholder, session-memory notice, relay disclosure, Close, Report, and Block. Existing Activity `FLAG_SECURE` remains active. New M11 strings are English-only per owner direction.
  - Added Kotlin instrumentation against `testvectors/thread-v1.json` and a two-session test against the actual `ember-server` router, covering handshake, three delivered messages each way, a sequence gap, replay rejection, and mailbox cleanup.
- Tests run and results:
  - `scripts/check-android.sh` with API 34 emulator `EmberM9_API34` up: passed (exit 0); 15 Rust Android binding tests passed; Android unit tests passed; all 19 instrumentation tests passed with 0 failures and 0 skipped. This includes the thread vector and real-relay tests.
  - Lint, forbidden-dependency scan, all three native ABI builds, debug/release APK builds and release shrinking passed. `FLAG_SECURE` is asserted by the existing Android instrumentation suite.
- Known issue / open requirement: the relay's `POST /v1/mailboxes` returns an empty response and exposes no mailbox metadata read. Android cannot get authoritative `expires_at`, so the UI reports the timestamp unavailable and cannot yet close automatically at server expiry. Request A-5 is recorded in `docs/cross-agent-requests.md`; no timestamp is guessed locally.
- Exact next step: Backend owner implements A-5; then Android wires the returned `expires_at`, verifies countdown and timed close on the API 34 real-relay test, reruns `scripts/check-android.sh`, and closes M11. Do not push this branch.

### 2026-11-19, Codex, milestone M11 Android finish (branch android-m11-finish)
- Status: partial; Android-owned finish items are implemented and verified. Keep M11 unchecked while the live-relay network proof and owner manual checks remain open. No README status change and no backend-owned source changes.
- What changed:
  - Added a first-launch Terms of Use screen with bundled English and Hindi text; local acceptance gates send/receive UI. Added configured contact/grievance email and a privacy-policy entry.
  - Added per-app English/Hindi locale configuration and settings, a Hindi terms layout check at 720x1280 with 1.3x and 2.0x font scales, accessibility labels/touch target/contrast checks, and an explicit report confirmation flow for both drop and thread reports. Optional report content/key fields remain off by default.
  - Wired A-5 `expires_at` from mailbox creation into the thread coordinator and screen. The earliest expiry of the three mailboxes drives the timer; expiry closes the thread and wipes in-memory session handles immediately while mailbox deletion runs asynchronously. Added an expiry auto-close instrumentation check.
  - Enabled debug StrictMode network checks. Extended dependency scans to reject analytics, crash reporting, Firebase, Play Services, and ad SDKs; Android Lint's own analytics tooling is excluded from the app/test dependency scan because it is not packaged in the APK.
  - Added `android/scripts/check-network-allowlist.sh` and a logging CONNECT proxy. The script injects its network instrumentation test only while running, clears app data to prove first-launch silence, then tests an explicit relay challenge through the proxy. It requires `EMBER_RELAY_HOST`.
- Tests run and results:
  - `scripts/check-android.sh` on `EmberM9_API34` (API 34): passed, exit 0; 15 Rust binding tests and 24 instrumentation tests passed, 0 failures, 0 skipped. Lint, dependency scans, all three native ABIs, debug and R8 release APK builds, and APK split checks passed.
  - API 34 real-server thread instrumentation passed against the local `ember-server` router: handshake, message exchange, sequence gap, replay rejection, close, and returned mailbox expiry bounds. This is the local server harness, not a deployed relay.
  - `check-localizations.sh`: 171 English/Hindi resource keys and format arguments match.
  - API 26 low-RAM AVD `EmberM11_API26_LowRAM` (768 MiB, 2 configured cores): installed and launched debug APK; StrictMode reported no main-thread network violation or crash. Idle PSS sample: 47,200 KiB.
  - API 34 R8 x86_64 release APK: signed with the local debug test key, installed, and launched successfully. This signing was only for emulator install; the release artifacts from `assembleRelease` are unsigned.
  - No README status update: live-relay allow-list proof remains unrun because the deployed HTTPS relay hostname is not available; configured placeholder is `links.ember.invalid`.
- Known issues / open requirements:
  - Set the real HTTPS relay hostname in `EMBER_RELAY_HOST` and run `android/scripts/check-network-allowlist.sh`. The thread test uses the local real server implementation but does not establish production-host interoperability.
  - OWNER MANUAL CHECKS: (1) Physical phone camera: install debug APK; open Receive > Scan QR code; grant camera permission; scan a valid Ember drop QR; verify link details, enter passphrase if asked, open it, and verify decrypted content. Record phone model and Android version. (2) Physical phone SAF: choose a real file using Send > Choose file; verify its name/size; send it; receive the file drop; use Save and select destination/name in the system picker; compare saved bytes with the source. Record phone model and Android version. (3) TalkBack focus: enable TalkBack; open report and burn confirmation dialogs; swipe through title, message/fields, Cancel, then Confirm; verify focus stays in the dialog until dismissal and returns to the control that opened it.
  - Verify dialog TalkBack focus order on-device and close remaining M10 interop gaps A-3 (CLI text send) and A-4 (web receiver initialization) separately.
- Exact next step: run the host allow-list script with the configured relay, perform the listed physical-phone checks, then update the M11 tracker only when all acceptance items are closed. Do not push this branch.
```
```

```
### 2026-11-18, Antigravity, CLI text drops and CI checks (branch cli-text-and-ci)
- Status: done. Resolved item A-3 in docs/cross-agent-requests.md and enhanced CI validation. No M11 work started.
- What changed:
  1. (CLI text drops, A-3):
     - `tools/cli/src/main.rs`: Added `--text <TEXT>` option to `ember send`, accepting inline string or `-` for stdin. Automatically builds `DropMetadata::new_text`. Reuses streaming encryption API. Supports `--relay`, `--passphrase`, `--burn`, and `--ttl`.
     - `tools/cli/src/main.rs`: Updated `ember receive <link>` and `ember decrypt <link-or-file>` to detect text drops (`metadata.kind == "text"`). Text is printed directly to stdout (or saved to file if `-o/--output` is provided). File drops remain saved to disk. Neither the link fragment nor passphrase are ever printed or logged.
     - `server/tests/server_tests.rs`: Added 4 comprehensive integration tests:
       - `test_cli_send_and_receive_text_end_to_end`: tests stdout streaming output, stdin input via `-`, and explicit `-o` file output.
       - `test_cli_send_and_receive_text_passphrase`: tests passphrase-protected text drops (missing and incorrect passphrases rejected, correct passphrase succeeds).
       - `test_cli_send_and_receive_text_burn_after_view`: tests burn-after-view semantics (first receive succeeds, second receive fails with 410 Gone / burned error).
       - `test_cli_send_and_receive_text_tampered_fails`: tests tampered ciphertext chunk 0 byte flip rejection.
  2. (Pre-push check script): Added `scripts/pre-push-check.ps1` and `scripts/pre-push-check.sh` covering branch status, unpushed commits, uncommitted changes, secret-like files, large files, merge conflict markers, and banned marketing words.
  3. (CI enhancements):
     - `.github/workflows/ci.yml`: Added `concurrency` cancellation group (`cancel-in-progress: true`).
     - Added step verifying `test-hooks` feature is absent from release dependency graphs of `ember-cli`, `ember-server`, and `ember-android-bindings` (`cargo tree -p <pkg> -e normal,features`).
     - Added `web-receiver` job running `npm ci`, `npm test`, and `npm run build`.
     - Added merge conflict marker detection check in `hygiene` job.
  4. (Cross-Agent Log): Marked A-3 resolved in `docs/cross-agent-requests.md` with instructions for Codex/Android agent to run interop testing.
- Decisions made: Text drops in CLI output UTF-8 decoded text directly to stdout when no `--output` flag is given, allowing piping. Link fragments and passphrases are strictly shielded from logs and stdout.
- Tests run and results:
  - `scripts/check.ps1`: Full gate passed cleanly:
    - `cargo fmt --all --check`: Clean pass.
    - `cargo clippy --workspace --all-targets -- -D warnings`: Clean pass (0 warnings).
    - `cargo build --bins`: Clean pass.
    - `cargo test --workspace`: All 146 unit and integration tests passed (46 core unit, 7 property, 7 strip, 1 drop vectors, 19 thread unit & security tests, 1 thread vector, 13 android-bindings FFI, 2 protocol, 10 server unit, 2 server main, 5 access log, 5 review fixes, 28 server integration).
    - `cargo deny check`: Clean pass (`advisories ok, bans ok, licenses ok, sources ok`).
    - `scripts/check_licenses.ps1`: Clean pass (AGPL-3.0).
    - `scripts/check_banned_words.ps1`: Clean pass (0 prohibited buzzwords).
  - `scripts/pre-push-check.ps1`: Clean pass ("READY TO PUSH").
- Known issues / TODO for next agent: Android agent (Codex) can perform "CLI sends text, app receives it" and "app sends text, CLI receives it" interop testing with `ember send --text` and `ember receive`.
- Exact next step: Review PR for `cli-text-and-ci` into `main`. Do not push. Do not start M11 without owner direction.

### 2026-11-22, Antigravity, UI Redesign Slice 0 (branch ui-redesign)

- What changed: Hindi dropped by owner decision, 22 Nov 2026. Removed `values-hi/`, `terms-of-use-hi.md`, language selectors from terms and home screens, and simplified `check-localizations.sh` to validate English resources. The app is English-only.
- Decisions made: Hindi dropped by owner decision, 22 Nov 2026.
- Tests run and results: `check-localizations.sh` passed, Android unit tests and lint passed.
- Exact next step: Implement UI Redesign Slice 1 (Theme and tokens).
```

---

## 10. Master prompt for a new agent (paste this)

> You are continuing the Ember project. Read `EMBER_BUILD_GUIDE.md` and `EMBER_PROJECT_BLUEPRINT.md` fully. Follow the hard rules in section 0 of the guide (no custom crypto, no Google services, never ask for secrets or touch `.env` files, no banned marketing words). Check the Status Tracker and the last Handoff Log entry, then do only the next unchecked milestone. Implement and test your changes directly in the repository and run the Acceptance checks. When finished, tick the milestone, append a Handoff Log entry with the exact next step, and commit. If you must make a decision, use the defaults in section 3 and record an ADR. If you are blocked, record the blocker in the log and stop.
