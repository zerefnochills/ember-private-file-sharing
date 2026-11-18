# Ember Handoff Log

This document tracks current progress and handoffs. See `docs/EMBER_BUILD_GUIDE.md` section 9 for the dated history.

## CURRENT STATE

- Scope: Request A-5 (authoritative mailbox expiry timestamp for Android M11).
- Branch: `main`.
- Done: Implemented `CreateMailboxResponse { id, expires_at }` on `POST /v1/mailboxes` (201 Created and 200 OK idempotent) and authenticated `GET /v1/mailboxes/{id}` returning `MailboxMetadataResponse { id, created_at, expires_at }` with push/pull bearer token authorization. All tests passed cleanly.
- In progress: Android agent (Codex) can now consume `expires_at` in `RelayClient.kt` and `ThreadCoordinator.kt` to drive the countdown timer and automatic thread close on API 34.
- Exact next step: Android agent branches from updated `main` to wire `expires_at` and close out M11.
- Latest gate: `scripts/check-android.sh` passed with API 34 emulator `EmberM9_API34` running: 13 Rust binding tests, 26 Android unit tests, 17 instrumentation tests, 0 failed and 0 skipped; lint, dependency scan, native ABI builds and release APK builds passed.
- Latest acceptance results: CLI-to-Android file passphrase/burn drop passed; Android-to-CLI passphrase/burn file and burn text passed with digest/text equality. The web link opened in Chrome, but the embedded receiver remained at `Initializing cryptography engine...`; no decryption occurred. `hsdpa` speed (14.4 Mbit/s) and delay (200 ms) 20 MiB retry run passed. 50 MiB send/receive peak PSS was 119547 KiB (116.75 MiB), 23 samples, below the 160000 KiB limit.
- M10 acceptance gaps: CLI-to-Android text (A-3); web receiver decryption (A-4); physical camera scan; SAF picker/save flows.

## Project status

- Status: M0 through M10 integration gates and M12 are complete; the M10 acceptance gaps above remain open. Core streaming engine (Request A-1) is complete. M11 has not started.
- Latest backend work: O3 access-log persistence, O4 encrypted reports, O5 isolated admin listener, O8 test-hook feature gate, O10 banned-word alignment, plus explicit versions for workspace path dependencies.
- Cross-agent status: Request A-1 is documented in `docs/cross-agent-requests.md` and `core/README.md`.
