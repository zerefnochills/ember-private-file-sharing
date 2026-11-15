# Ember

A private file-sharing app for Android and desktop. Send a file or message with no
account and no phone number. Everything is encrypted on your device. Each share can
carry a short private thread that is deleted after 12 hours (or after viewing, if the
sender chooses). When two people are physically near each other, a later version will
work with no internet.

**Status:** early development (milestones M0–M7 complete; M8 is in progress). Nothing here is ready for real use.

## Honest limits

- A recipient can keep a copy (screenshot, camera, saved file). Expiry cannot prevent that.
- A modified app can ignore expiry rules.
- The server cannot read content, but it still sees technical metadata such as timing and
  network addresses. Exactly what is logged is documented in `docs/privacy-and-logging.md`
  (draft, written in milestone M1).
- Whoever holds a link can open it, unless a passphrase is also set.

## Repository layout

| Path | Purpose |
|---|---|
| `core/` | Rust: encryption, link format, expiry rules, metadata stripping |
| `protocol/` | Rust: wire types shared by core and server |
| `server/` | Rust relay server (stores ciphertext only) |
| `tools/cli/` | Command-line test client |
| `tools/admin-cli/` | Admin tool for deletion by ID and takedown logging |
| `web-receiver/` | Minimal receive-only web page (M5) |
| `android/` | Android app (M9 onward) |
| `desktop/` | Desktop app (later) |
| `testvectors/` | Shared test vectors every implementation must pass |
| `docs/` | Blueprint, build guide, protocol spec, threat model, ADRs |

## Building

```
cargo test --workspace
```

Requires a recent stable Rust toolchain. Contributors and agents: read
`docs/EMBER_BUILD_GUIDE.md` first and follow its rules.

## Licence

AGPL-3.0-only. See `LICENSE`.
