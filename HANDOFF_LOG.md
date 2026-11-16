# Ember Handoff Log

This document tracks current progress and handoffs. See `docs/EMBER_BUILD_GUIDE.md` section 9 for the dated history.

## CURRENT STATE

- Scope: Integration task only; do not start M11.
- Done: `integration` starts at `a10-done` (`5b70a87`); `claude-review` and `main` (`0846275`) are included by fast-forward, with no conflicts.
- In progress: None. Integration gates completed; no M11 work started.
- Exact next step: Continue M10 manual acceptance with Android-to-web receiver interoperability. Keep all gaps below open until each is verified; do not start M11 without owner direction.
- Known failing tests: None. `scripts/check.ps1` passed (128 Rust tests). `scripts/check-android.sh` passed (13 Rust binding tests, 25 Android unit tests, 7 API 34 instrumentation tests with 0 skipped). The real-server burn/receive/tamper case passed against the merged `ember-server`. CLI `--help` passed with `-H/--host` and `-h/--help` both available.
- M10 acceptance gaps not verified: Android-to-web receiver interop; Android-to-CLI interop; throttled weak-network run; large-file memory measurement; physical camera scan; SAF picker/save flows; missing HTTPS deep link in the manifest.

## Project status

- Status: M0 through M10 integration gates and M12 are complete; the M10 acceptance gaps above remain open. Core streaming engine (Request A-1) is complete. M11 has not started.
- Latest backend work: O3 access-log persistence, O4 encrypted reports, O5 isolated admin listener, O8 test-hook feature gate, O10 banned-word alignment, plus explicit versions for workspace path dependencies.
- Cross-agent status: Request A-1 is documented in `docs/cross-agent-requests.md` and `core/README.md`.
