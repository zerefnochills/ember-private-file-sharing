# Ember Handoff Log

This document tracks current progress and handoffs. See `docs/EMBER_BUILD_GUIDE.md` section 9 for the dated history.

## CURRENT STATE

- Scope: M11 Android finish: legal acceptance, languages, accessibility, privacy/network verification, low-end device checks, and expiry timer integration.
- Branch: `android-m11-finish`, branched from updated `main` at `3734b32`, then merged the Android thread implementation from `android-m11-threads`.
- Done before this branch: Rust/Kotlin thread bindings, relay client, Compose thread screen, v3 vector test, two-session real-relay instrumentation, and A-5 server expiry API. M10 CLI-to-Android text, web receiver, physical camera, and SAF gaps remain open independently.
- In progress: consume A-5 expiry for all three mailboxes and complete the remaining M11 acceptance tests.
- Exact next step: run the full Android gate on API 26/minSdk and API 34 devices with no skipped checks, record any low-end/profile gaps, and append the M11 finish handoff.
- Latest Android gate before this branch: `scripts/check-android.sh` passed on API 34 with 15 Rust binding tests and 19 instrumentation tests (0 failures, 0 skipped); lint, dependencies, ABIs and APK packaging passed.
## Project status

- Status: M0 through M10 integration gates and M12 are complete; the M10 acceptance gaps above remain open. Core streaming engine (Request A-1) is complete. M11 has not started.
- Latest backend work: O3 access-log persistence, O4 encrypted reports, O5 isolated admin listener, O8 test-hook feature gate, O10 banned-word alignment, plus explicit versions for workspace path dependencies.
- Cross-agent status: Request A-1 is documented in `docs/cross-agent-requests.md` and `core/README.md`.
