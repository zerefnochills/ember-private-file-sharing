# Ember Threat Model

**Status:** Draft v1, milestone M1. Expanded from blueprint section 7.10. The independent auditor (milestone M13) should start here.
**Companion docs:** `protocol-spec.md`, `privacy-and-logging.md`, `EMBER_PROJECT_BLUEPRINT.md`.

---

## 1. What Ember protects

| Asset | Where it lives | Protection goal |
|---|---|---|
| File and text content | Sender device, ciphertext on server, recipient device | Only holders of the full link (and passphrase, if set) can read it |
| Link secret K | Link fragment (or passphrase-wrapped form), device memory | Never reaches the server or any log |
| File name and type | Inside the ciphertext | Hidden from the server |
| Existence and timing of a drop | Server | Reduced: random IDs, short life, minimal logs. Not hidden |
| Network address of sender and recipient | Server and network path | Not hidden. Minimised and disclosed in `privacy-and-logging.md` |

## 2. Who we defend against, and who we do not

**In scope**

1. **Passive network observer** (café Wi-Fi, ISP).
2. **Server compromise** (attacker reads disks or memory of the relay server and its storage).
3. **Curious or coerced operator** (the operator, or someone who forces the operator, reads stored data).
4. **Link-preview bots and crawlers** that fetch URLs shared in chat apps.
5. **Casual link leakage** (link shared over a channel others can see).
6. **Abusers** who want to use the service for scams, harassment or illegal content.
7. **Malicious sender** (sends harmful files or hostile file names to a recipient).

**Out of scope (stated openly)**

- A compromised or rooted device, malware on the sender or recipient device, or a malicious keyboard.
- A recipient who keeps a copy (screenshot, camera, copy of the saved file).
- A modified client that ignores expiry or burn rules.
- A global adversary performing large-scale traffic correlation.
- Legal compulsion directed at users themselves.

## 3. Trust assumptions

- The OS random generator is sound.
- libsodium (or the chosen compatible library) is correct, and is vetted by third parties.
- TLS protects the client-to-server channel against passive and active network attackers.
- The Android app a user installs matches the published source (hence reproducible builds, section 5, T3).

## 4. Threats and mitigations

| ID | Threat | Mitigation | Residual risk |
|---|---|---|---|
| T1 | Server breach exposes stored drops | Server holds ciphertext only. K lives in the URL fragment and never reaches the server. Blobs are deleted after 12 hours at most | Attacker learns metadata still held: sizes, timing, IPs in logs within retention |
| T2 | Operator or coerced operator reads drops | Same as T1. Operator cannot decrypt. Takedown works by deleting ciphertext by ID | Operator can delete, withhold or serve altered ciphertext (detected by authentication, not prevented) |
| T3 | **Malicious operator ships a modified client or web page** that steals K | App: open source, reproducible builds, signed releases, independent audit, F-Droid distribution. Web receiver: see T3a | Users must verify the build. Trust in the distribution channel is unavoidable |
| T3a | **Web receiver is delivered by the same server it protects against.** A compromised server can serve JavaScript that reads the fragment | Web receiver is self-contained, no third-party scripts, strict CSP, published hashes of each release. The app is the recommended receiver for sensitive drops. The page tells users that the web version trusts the host | This is a known limit of browser-based end-to-end encryption. It cannot be fully removed |
| T4 | Network observer reads content | TLS plus end-to-end encryption | Observer sees that a connection to the Ember server happened, plus sizes and timing |
| T5 | Traffic analysis (size, timing) | Padding to 64 KiB bands for content; fixed 4096-byte metadata record; short life | Approximate size and timing remain observable |
| T6 | Link interception (whoever sees the link can open the drop) | Optional passphrase shared on a different channel, burn-after-view, 12-hour expiry | Anyone with the full link (and passphrase) can open it. Said plainly in the app |
| T6a | **Offline guessing of a weak passphrase.** The passphrase-wrapped key is inside the link itself, so anyone who obtains the link can try guesses offline | Argon2id (default 3 passes, 64 MiB) slows each guess. The app nudges toward long passphrases and offers generated ones | Weak passphrases can be cracked. Passphrase strength is the only barrier |
| T7 | Link-preview bot or crawler consumes a burn-after-view drop | Burning needs an explicit `claim` then `finish` (POST), never a `GET`. Bots cannot complete it. The fragment is never sent in the request, so bots cannot decrypt anyway | A bot that executes the full web page could still claim. The confirm step needs a human tap |
| T8 | Tampering, truncation or reordering of ciphertext | Authenticated secretstream with `FINAL` tag; strict receiver checks (protocol-spec 4.3) | Denial of service (the drop fails to open) cannot be prevented |
| T9 | Recipient keeps a copy | Cannot be prevented. `FLAG_SECURE` and hidden recents reduce casual screenshots on Android | Stated openly in the app and docs |
| T10 | Modified client ignores expiry | Server enforces expiry and burn for stored data | Copies already downloaded stay downloaded |
| T11 | Malicious file or file name (path traversal, double extension, executable disguise) | File name is sanitised on receive; media type is informational; the app never auto-opens files; files are saved to app-controlled storage | The recipient can still open a dangerous file by choice. The app shows type and size first |
| T12 | Abuse: scams, harassment, illegal content | Proof-of-work (M4 / O2 on both drops and mailboxes), per-IP limits, size cap, 12-hour life, report and block, terms, takedown by ID (M8), transparency report | Determined abusers rotate IPs. The server cannot scan encrypted content. Moderation relies on recipient reports and legal orders; this is stated in the terms |
| T13 | Resource exhaustion (storage, bandwidth, CPU through Argon2id) | Size caps, TTL, rate limits, PoW on blob and mailbox creation, parameter bounds on passphrase wrapping (protocol-spec 5.2), upload expiry | Large coordinated attacks need infrastructure-level protection |
| T14 | Enumeration of blob IDs | 128-bit random IDs, no listing endpoint, uniform `410/404` responses, rate limits | Negligible |
| T15 | Replay or reuse of upload and claim tokens | Delete token stored hashed and compared in constant time; claim tokens single-use, short-lived | Low |
| T16 | Crash reports, analytics or logs leaking the fragment or filenames | No analytics SDKs. The fragment and file names are never logged. CI privacy tests grep logs and recorded requests (milestones M3 and M8) | Developer mistakes; hence tests and audit |
| T17 | Dependency compromise (supply chain) | `cargo audit`, `cargo deny`, pinned lockfiles, minimal dependency list, reproducible builds | Residual supply-chain risk remains |
| T18 | Android-specific leakage (screenshots, recents thumbnails, clipboard, notifications) | `FLAG_SECURE`, hide content in recents, no content in notifications, avoid clipboard use for secrets | OEM behaviour varies; tested on low-end devices |
| T19 | Thread session compromise or relay abuse | Standard Noise handshake (`Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s` via audited `snow` crate) authenticated by drop PSK (KDF subkey 3); proof-of-work on mailbox creation; directional mailbox capabilities derived from transcript hash | Relay sees capability tokens, sizes, and timing; forward secrecy is not to be marketed until independent cryptographer review |

## 5. Design rules that follow from this model

1. No custom cryptography; audited libraries only.
2. K, the fragment, passphrases and file names never appear in logs, analytics or crash reports.
3. A plain `GET` never changes state.
4. Server enforces expiry; client displays it but is not trusted to enforce it.
5. The web receiver is a convenience path and is labelled as trusting its host.
6. Every security claim in the app or store listing must have a matching test or a stated limit in this document.

## 6. User-facing honesty (must appear in the app, terms and README)

- Recipients can keep copies; expiry cannot stop that.
- Whoever holds the full link (and passphrase) can open the drop.
- The server cannot read your files, but it can see technical details such as when you connected and from which network address, for a limited time. See the privacy policy for exact periods.
- Ember cannot review content. Harmful content is handled through reports and legal orders.
- Thread forward secrecy is not to be marketed until the independent cryptographer has reviewed the integration.

## 7. Open points for the auditor

- Review of the passphrase wrapping (section 5.2 of the spec) and Argon2id defaults on low-end hardware.
- Burn-after-view claim and finish flow (races, retries, bot behaviour).
- Noise protocol thread handshake choice (`Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s` via `snow` crate, ADR 0008), thread PSK derivation from drop secret, and session-level transport encryption implementation.
- Web receiver delivery model (T3a) and whether a stronger integrity mechanism is practical.
