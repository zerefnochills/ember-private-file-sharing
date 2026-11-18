# Ember Handoff Log

This document tracks current progress and handoffs. See `docs/EMBER_BUILD_GUIDE.md` section 9 for the dated history.

## CURRENT STATE

- Scope: M11 Android 1:1 threads, continuing M10 acceptance gaps.
- Branch: `android-m11-threads`, branched from `main` at `dc337b6`.
- Done: M11 UniFFI bindings, Kotlin relay client and session coordinator, Compose thread screen, Rust binding tests, v3/thread-vector instrumentation, and API 34 real-relay two-session test. New M11 strings are English-only per owner direction. See the 2026-11-18 M11 entry in `docs/EMBER_BUILD_GUIDE.md` section 9.
- Open: Server-provided mailbox `expires_at` is unavailable from the current API; request A-5 is in `docs/cross-agent-requests.md`. M10 A-3/A-4 and physical camera/SAF manual checks remain open as recorded below.
- Exact next step: Backend owner resolves A-5; then Android verifies the server-based countdown and timed close and reruns the full gate. Do not push this branch.
- Latest gate: `scripts/check-android.sh` passed with API 34 emulator `EmberM9_API34` running: 15 Rust binding tests; Android unit tests passed; 19 instrumentation tests, 0 failed and 0 skipped; lint, dependency scan, all three native ABI builds, and debug/release APK builds passed.
- Latest acceptance results: CLI-to-Android file passphrase/burn drop passed; Android-to-CLI passphrase/burn file and burn text passed with digest/text equality. The web link opened in Chrome, but the embedded receiver remained at `Initializing cryptography engine...`; no decryption occurred. `hsdpa` speed (14.4 Mbit/s) and delay (200 ms) 20 MiB retry run passed. 50 MiB send/receive peak PSS was 119547 KiB (116.75 MiB), 23 samples, below the 160000 KiB limit.
- M10 acceptance gaps: CLI-to-Android text (A-3); web receiver decryption (A-4); physical camera scan; SAF picker/save flows.

## Project status

- Status: M0 through M10 integration gates and M12 are complete; M11 Android thread work is implemented with A-5 still open. M10 acceptance gaps above remain open. Core streaming engine (Request A-1) is complete.
- Latest backend work: O3 access-log persistence, O4 encrypted reports, O5 isolated admin listener, O8 test-hook feature gate, O10 banned-word alignment, plus explicit versions for workspace path dependencies.
- Cross-agent status: Request A-1 is documented in `docs/cross-agent-requests.md` and `core/README.md`.
