# Review of milestones M0 to M10 (16 November 2026)

**Reviewer:** Claude (second pair of eyes, not an independent security audit).
**Reviewed state:** branch `android-work` at `5b70a87` (tag `a10-done`), which contains all of `main` plus the Android work.
**Method:** read the Rust core (keys, wrap, link, padding, metadata, streaming), the whole server request path (burn flow, tokens, limits, admin, reports, storage), the scripts and CI, the Android manifest and notes, and the legal drafts. Ran `cargo fmt`, `cargo clippy -D warnings` and the full workspace tests before and after the fixes below.
**Not done:** I could not run Gradle or an emulator here, so the Android code and its gate were not executed by me. I did not review the thread handshake and ratchet line by line, the Android Kotlin sources in depth, or the 545 KB web-receiver bundle internals.

**Result after fixes:** `cargo fmt --check` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean, 118 tests passing, 0 failing (`cargo test --workspace --no-fail-fast`).

---

## 1. Fixed in this review (with regression tests)

| ID | Severity | Finding | Fix |
|---|---|---|---|
| F1 | High | **Client address was forgeable and shared.** Rate limiting read `X-Forwarded-For` / `X-Real-IP` from any caller, and fell back to one shared `127.0.0.1` bucket when absent (the server never read the real TCP peer). Anyone could dodge limits by forging the header, and ordinary clients all shared a single bucket. | Server now uses the TCP peer address (`into_make_service_with_connect_info`). Forwarded headers are honoured only from `--trusted-proxy` addresses, using the right-most untrusted entry. Access log uses the same function. Tests: `forged_forwarded_for_does_not_bypass_rate_limit`, state unit tests. |
| F2 | High | **Path traversal in storage.** Identifiers were joined onto the storage root unchecked, and the admin takedown endpoint passed `target_id` straight to `remove_dir_all(root.join(id))`. `..`, `.` or an absolute path would delete the parent directory or the whole store. | Takedown validates IDs (400 on bad input). Filesystem storage maps any malformed ID to a placeholder (defence in depth). Tests: `admin_takedown_rejects_path_like_target_ids`, `filesystem_storage_ignores_path_traversal_ids`. |
| F3 | High | **Sender-controlled size could crash a receiver.** `metadata.size` had no upper bound; `calculate_padded_len` could overflow and whole-buffer decrypt pre-allocated `size` bytes. A malicious sender (who holds the key) could abort or exhaust the receiver. | Hard cap `MAX_CONTENT_BYTES` = 1 GiB in metadata validation, saturating padding arithmetic, bounded pre-allocation (16 MiB). Test: `test_validate_rejects_absurd_size`. |
| F4 | Medium | **Declared part count unbounded.** `parts` was not tied to `size`, so a client could declare billions of parts (storage reservation and a long loop in `complete`). | `size` and `parts` must agree with the spec's part layout. Test: `inconsistent_size_and_part_count_is_rejected`. |
| F5 | Medium | **Claim race.** Claim was read, checked and written non-atomically, so two simultaneous readers could both be given a claim token for a one-view blob (the later write silently invalidated the first). | Claim and finish are serialised with a lock. Single-instance only; see O6. |
| F6 | Medium | **Unthrottled endpoints.** Mailbox creation had no limit (disk exhaustion), admin endpoints allowed unlimited token guessing, report fields were unbounded. | Rate limit on mailbox creation and admin endpoints; length caps on report fields. Test: `oversized_report_is_rejected`. |
| F7 | Medium | **Unbounded access-log memory.** Every request appended to an in-memory vector that was never trimmed. | Capped ring (10,000 entries). Persistence is still missing, see O3. |
| F8 | Medium | **Server crash on `--help`/start.** `--host` used the short flag `-h`, which clap reserves for help (the Android agent had to work around it). | Short flag removed. |
| F9 | Low | **File-name sanitiser was platform-dependent** and its own test failed on Linux/CI (`C:\Windows\System32\cmd.exe` was not reduced to `cmd.exe`). Android runs Linux semantics. | Splits on both `/` and `\` itself, replaces more illegal characters, neutralises Windows reserved names. Tests added. |
| F10 | Low | **Link parser accepted any `http://` host and odd host strings** (for example user-info). | `http://` only for loopback and `10.0.2.2`; strict host character set. Tests added. |
| F11 | Low | Secret copies not zeroised (decoded link payload, unwrapped key array, encoded buffer). | Zeroised. |
| F12 | Low | `scripts/check.sh` referenced an undefined `SCRIPT_DIR` (fails under `set -u`) and `cargo audit` / `cargo deny` were ignored with `|| true`. | Fixed; failures now fail the gate. |
| F13 | Low | CLI end-to-end tests only passed if someone had built the CLI binaries beforehand, so a clean checkout or CI failed. | Tests build the binaries on demand. |
| F14 | Low | CSP lacked `base-uri`, `form-action`, `frame-ancestors`. | Added. |
| F15 | Legal | The legal drafts and F-Droid metadata named "Ember Technologies" as the operator. That is the name of an unrelated, existing company that owns the Ember trademark in other fields. The privacy policy's log buckets differed from the code, and its metadata-stripping claim was broader than the implementation. | Operator name replaced by a placeholder; buckets aligned with code; stripping claim narrowed to what is implemented. Still drafts pending legal review. |

Also corrected: ADR 0006 claimed a "formally verified state machine". The thread protocol is a Noise-inspired composition of libsodium primitives, not the Noise framework, and is not formally verified (see O1).

---

## 2. Open items (not fixed here)

| ID | Priority | Item |
|---|---|---|
| O1 | **High (before launch)** | **Thread protocol is hand-composed crypto.** [RESOLVED in integration] Replaced hand-composed handshake and ratchet with standard Noise pattern `Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s` via audited `snow` crate, using spec-reserved sub-key ID 3 for thread PSK, envelope versioning (`v = 2`), and deterministic vectors (ADR 0008). |
| O2 | Medium | Proof-of-work required for thread creation. [RESOLVED in integration] Mailbox creation (`POST /v1/mailboxes`) now enforces Proof-of-Work verification with rejection, replay, and recovery test coverage. |
| O3 | Medium | **Log retention is not implemented.** [RESOLVED in backend review] Persisted to disk with configurable retention (default 30 days) and scheduled deletion sweeper. |
| O4 | Medium | Reports stored in plaintext JSON. [RESOLVED in backend review] Encrypted at rest using reviewer public key (sealed box), with CLI commands to generate keys and decrypt. |
| O5 | Medium | Admin endpoints share the public listener. [RESOLVED in backend review] Added optional `--admin-listen` socket address flag to serve admin endpoints on an isolated interface. |
| O6 | Medium | Single-instance assumptions: claim lock, rate limiter and PoW replay cache are in process memory. Horizontal scaling needs shared state and an atomic claim operation in the store. |
| O7 | Medium | The web receiver is a committed 545 KB bundle (with embedded WASM). Add a reproducible build in CI and publish its hash. The trust limits in threat T3a still apply. |
| O8 | Medium | **Test hooks in the public API.** [RESOLVED in backend review] Gated `wrap_key_with_injected` and `encrypt_drop_*_with_header` behind the `test-hooks` Cargo feature. |
| O9 | Medium | Android gaps recorded by the Android agent: M11 not started, cross-process resume, low-memory testing, physical camera and file-picker QA, the real-server test is not part of the default gate. No `https://` app link (deep link) is declared in the manifest; links are opened via share, paste or QR. |
| O10 | Low | `check_banned_words` and CI hygiene job paths aligned. [RESOLVED in backend review] Scripts and CI now run the exact same scanner with all banned words. |
| O11 | Low | `testvectors/drop-v1.json` is 13 MB. Fine for now, but consider generating the large cases in tests instead of committing them. |
| O12 | Low | Public GitHub repository: make it private until the legal drafts, contact addresses and operator name are real. |

---

## 3. What looked good

- The drop format follows the spec closely (chunking, padding, tags, receiver checks), with tamper, truncation, reorder and trailing-data tests.
- Keys use zeroising wrappers with redacted `Debug`; `unsafe` is forbidden workspace-wide.
- Burn-after-view uses claim then finish, a plain GET never burns, and the tests cover bot-style requests.
- Delete tokens are stored hashed and compared in constant time; blob IDs are 128-bit random.
- Access logs use route templates, not blob IDs, and tests grep logs for planted secrets.
- Android: no Google services, `allowBackup=false`, `FLAG_SECURE`, streaming receive to private temporary storage exposed only after the final tag verifies, per-ABI release APKs of about 7.5 to 7.8 MiB.

---

## 4. Recommended next steps

1. Merge this review branch, then run the Android gate on your machine to confirm nothing broke (Rust API behaviour was kept compatible; the only visible changes are stricter validation and the `--host` flag).
2. Decide O1 (switch threads to `snow`, or commission the review). This is the main cryptographic risk.
3. Fix O3 and O5 together with the lawyer's answer on log retention.
4. Make the repository private; replace the operator-name placeholder once the company exists.
5. Start M11 only after the above, with a fresh budget.
