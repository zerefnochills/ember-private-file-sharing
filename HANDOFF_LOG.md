# Ember Handoff Log

This document tracks current progress and handoffs. See `docs/EMBER_BUILD_GUIDE.md` section 9 for the dated history.

## CURRENT STATE

- Scope: Integration task only; do not start M11.
- Done: `integration` starts at `a10-done` (`5b70a87`); `claude-review` and `main` (`0846275`) are included by fast-forward, with no conflicts.
- In progress: Android gate, including API 34 connected instrumentation and the mandatory real-server send/receive, burn, and tamper case.
- Exact next step: Run `scripts/check-android.sh` with the API 34 emulator online, then inspect instrumentation XML and ABI-split release APK sizes.
- Known failing tests: None. `scripts/check.ps1` passed in this session (128 Rust tests passed); `cargo run -p ember-server -- --help` passed and showed `-H/--host` alongside `-h/--help`. The helper compiled with the configured LLVM-MinGW host toolchain; the initial default MinGW attempt failed before project compilation.

## Project status

- Status: M0 through M9, M12, and the core streaming engine (Request A-1) are complete. The M10 integration gates are in progress; acceptance gaps will be recorded separately.
- Latest backend work: O3 access-log persistence, O4 encrypted reports, O5 isolated admin listener, O8 test-hook feature gate, O10 banned-word alignment, plus explicit versions for workspace path dependencies.
- Cross-agent status: Request A-1 is documented in `docs/cross-agent-requests.md` and `core/README.md`.
