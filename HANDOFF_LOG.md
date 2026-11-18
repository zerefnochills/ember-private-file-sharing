# Ember Handoff Log

This document tracks current progress and handoffs. See `docs/EMBER_BUILD_GUIDE.md` section 9 for the dated history.

## CURRENT STATE

- Scope: M10 Android closure only; no M11 work started.
- Branch: `android-m10-close`, branched from `main`.
- Done: configured-host HTTPS deep link and saved-state fragment test; API 34 CLI-to-Android file interop; Android-to-CLI file and text interop; weak-network retry; 50 MiB memory sampling; Android gate. See the 2026-11-18 entry in `docs/EMBER_BUILD_GUIDE.md` section 9.
- In progress: Backend-owned CLI text sending (A-3) and embedded web receiver crypto initialization (A-4) remain open. Physical camera scan and SAF picker/save are OWNER MANUAL CHECKS.
- Exact next step: Backend owner resolves A-3 and A-4; owner performs the physical-phone steps under OWNER MANUAL CHECKS in `android/scripts/M10_ACCEPTANCE.md`. Do not start M11 without owner direction.
- Latest gate: `scripts/check-android.sh` passed with API 34 emulator `EmberM9_API34` running: 13 Rust binding tests, 26 Android unit tests, 17 instrumentation tests, 0 failed and 0 skipped; lint, dependency scan, native ABI builds and release APK builds passed.
- Latest acceptance results: CLI-to-Android file passphrase/burn drop passed; Android-to-CLI passphrase/burn file and burn text passed with digest/text equality. The web link opened in Chrome, but the embedded receiver remained at `Initializing cryptography engine...`; no decryption occurred. `hsdpa` speed (14.4 Mbit/s) and delay (200 ms) 20 MiB retry run passed. 50 MiB send/receive peak PSS was 119547 KiB (116.75 MiB), 23 samples, below the 160000 KiB limit.
- M10 acceptance gaps: CLI-to-Android text (A-3); web receiver decryption (A-4); physical camera scan; SAF picker/save flows.

## Project status

- Status: M0 through M10 integration gates and M12 are complete; the M10 acceptance gaps above remain open. Core streaming engine (Request A-1) is complete. M11 has not started.
- Latest backend work: O3 access-log persistence, O4 encrypted reports, O5 isolated admin listener, O8 test-hook feature gate, O10 banned-word alignment, plus explicit versions for workspace path dependencies.
- Cross-agent status: Request A-1 is documented in `docs/cross-agent-requests.md` and `core/README.md`.
