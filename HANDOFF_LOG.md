# Ember Handoff Log

This document tracks current progress and handoffs. See `docs/EMBER_BUILD_GUIDE.md` section 9 for the dated history.

## CURRENT STATE

- Scope: M11 Android finish: legal acceptance, languages, accessibility, privacy/network verification, low-end device checks, and expiry timer integration.
- Branch: `android-m11-finish`, branched from updated `main` at `3734b32`, then merged the Android thread implementation from `android-m11-threads`.
- Done before this branch: Rust/Kotlin thread bindings, relay client, Compose thread screen, v3 vector test, two-session real-relay instrumentation, and A-5 server expiry API. M10 CLI-to-Android text, web receiver, physical camera, and SAF gaps remain open independently.
- M11 implementation status: legal acceptance, English/Hindi app locale, accessibility checks, explicit report confirmation, server expiry countdown/auto-close, dependency checks, and low-end/API34 runs are implemented and verified. M11 remains open for live-relay network proof and owner manual checks below.
- Latest Android gate (2026-11-19): `scripts/check-android.sh` passed on `EmberM9_API34` (API 34): 15 Rust binding tests and 24 instrumentation tests, 0 failures and 0 skipped; lint, dependency scan, all three native ABIs, debug/R8 release builds, and APK split checks passed.
- Device checks: `EmberM11_API26_LowRAM` (API 26, 768 MiB RAM, 2 configured cores) installed and launched the debug APK with StrictMode enabled and no main-thread network violation or crash. Idle app PSS was 47,200 KiB. An x86_64 R8 release APK was signed with the local debug test key, installed, and launched on API 34.
- Open: `android/scripts/check-network-allowlist.sh` and its temporary instrumentation test are ready, but need a real HTTPS Ember relay hostname. The configured default remains `links.ember.invalid`; the owner did not have the live hostname available. The API 34 thread test uses the actual local `ember-server` router, not a deployed relay.
- OWNER MANUAL CHECKS (physical phone required): (1) Camera: install debug APK; open Receive > Scan QR code; grant camera permission; scan a valid Ember drop QR; verify link details, enter passphrase if requested, open and verify decrypted content. Record phone model and Android version. (2) SAF: choose a real file from Send > Choose file; verify name/size, send; receive the file drop; select Save and choose a location/name in the system picker; compare saved bytes with the original. Record phone model and Android version.
- Exact next step: set the real relay host in `EMBER_RELAY_HOST` and rerun `android/scripts/check-network-allowlist.sh`; then complete these owner checks: (1) physical camera scan steps in the dated guide entry; (2) physical SAF picker/save steps there; (3) enable TalkBack, open the report and burn confirmation dialogs, swipe through title, message/fields, Cancel, then Confirm, and verify focus stays in the dialog until dismissal and returns to the launch control. Keep M11 unchecked and do not update README status until all acceptance lines are closed.
## Project status

- Status: M0 through M10 integration gates and M12 are complete; M10 acceptance gaps above remain open. M11 is partial with the explicit open items listed above. Core streaming engine (Request A-1) is complete.
- Latest backend work: O3 access-log persistence, O4 encrypted reports, O5 isolated admin listener, O8 test-hook feature gate, O10 banned-word alignment, plus explicit versions for workspace path dependencies.
- Cross-agent status: Request A-1 is documented in `docs/cross-agent-requests.md` and `core/README.md`.
