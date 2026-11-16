# Ember: Project Blueprint

**Status:** Product and architecture plan. Implementation progress is tracked in `docs/EMBER_BUILD_GUIDE.md`.
**Snapshot date:** 15 November 2026. Regulatory dates and store policies change; re-verify before launch.
**Disclaimer:** This is a planning document, not legal advice. Get an Indian technology lawyer's opinion before launch.

> "Ember" is the chosen working name (replaced the placeholder "SealDrop"). Overlap check on 15 Nov 2026 found no encrypted file-sharing product with this name, but "Ember" is also the name of a smart-mug app (Ember Technologies, registered trademark) on both app stores, and of the Ember.js framework. Still check the Indian trademark registry (Class 9 and 42) and domains before public launch, and use a distinctive store title such as "Ember: Private File Drop".

---

## 1. Vision and positioning

### 1.1 One-paragraph definition

A private file-sharing app for Android and desktop. Send a file or message with no account and no phone number. Everything is encrypted on your device. Each share comes with a short private thread that vanishes after 12 hours (or after viewing, if the sender chooses). When two people are physically near each other, it works with no internet at all.

### 1.2 Positioning decision

**Brand as: a private file-sharing app.**
**Do not brand as: an anonymous social network.**

Reasons:

- App stores treat anonymous or random chat apps as high risk. Apple's guidelines say apps used mainly for anonymous or random chat do not belong on the App Store, and Apple removed Vibely in June 2026. Google requires reporting, blocking and moderation for anything with user-generated content.
- "Anonymous social network" attracts harassers, scammers and people trading illegal content. "Private file sharing" attracts people with a task to finish.
- A social network competes with WhatsApp, Telegram and Instagram. A one-time secure drop competes with clunky tools that can be beaten.
- The original need (share something once, then no further interaction) is a file-sharing product.

**Tagline options:**

- "Send it once. Only they can open it."
- "Private sharing, even without internet."

**Words to avoid in marketing and store listings:** "anonymous", "untraceable", "secret", "military grade", "no logs", "leaves no trace". They overpromise, attract the wrong users, and risk store and regulatory trouble.

**Offline positioning (honest version):** "Works with no internet when you're near each other." Long-distance offline is not promised (see section 7.8).

---

## 2. Decisions log

| # | Decision | Reason |
|---|---|---|
| D1 | Product is a file-sharing app with a short thread per share, not a general chat app | Store policy, abuse risk, regulatory burden |
| D2 | No phone numbers, no names, no email at signup. No accounts at all in v1 | Core privacy promise |
| D3 | Temporary handles (e.g. `amber-fox-4821`) instead of "proxy numbers" | Avoids confusion with phone-number traceability rules |
| D4 | Handles are per-thread and disappear with the thread | No persistent identity to track |
| D5 | Messages and files vanish after viewing (if set) or after 12 hours | Matches the "one-time sharing" goal |
| D6 | End-to-end encryption using audited libraries only. No custom cryptography | The most common way projects like this fail |
| D7 | Open source (client and server) plus an independent audit before public launch | Trust is the product |
| D8 | No secret admin monitoring. Operational data collected is minimal and disclosed | Deception risk, DPDP notice rules, and it defeats the product's purpose |
| D9 | Not marketed to government or military users | Classified comms need accredited systems; draws wrong attention |
| D10 | Avoid Google dependencies (no FCM, no Play Integrity, no Firebase, no Nearby Connections library) | Stated goal; also reduces metadata leakage |
| D11 | Android first. Desktop (PC) second. iOS later, if at all | Store-policy and effort reasons |
| D12 | Shared Rust core library with thin native interfaces per platform | One implementation of the security-critical code |
| D13 | Nearby (offline, short range) mode is Android-to-Android in v1.5 | Realistic and valuable; mesh and long range deferred |
| D14 | Developer is publicly identifiable; publish through an Indian company | Google developer verification, Indian grievance-officer rules |

---

## 3. Audience

### 3.1 Intended users

- Journalists and sources exchanging documents once
- Lawyers, consultants, NGOs and small businesses sending a sensitive file one time
- Activists and whistleblowers in places where phone numbers are tied to ID
- Privacy-minded individuals sharing medical records, ID scans or private photos
- People in internet-shutdown situations who need short-range offline transfer

### 3.2 Unintended users (plan for them)

Any anonymous, free, no-signup, ephemeral app attracts scammers, harassers, extortionists and people sharing illegal content. This is the biggest operational risk (section 9). The design includes abuse controls from day one.

---

## 4. Competitive landscape

| Competitor | Type | Gap we can exploit |
|---|---|---|
| OnionShare | Tor-based file share | Sender must stay online; recipient normally needs Tor; poor mobile UX |
| Send (Firefox Send forks) | Link-based E2EE file share | Discontinued by Mozilla in 2020; community forks only; no mobile-first successor |
| Magic Wormhole | CLI transfer | Command-line only |
| Proton Drive links | E2EE cloud storage | Requires an account |
| SecureDrop | Newsroom submission system | Heavy infrastructure; not for individuals |
| Signal / SimpleX / Session | Private messengers | Chat-first; not a one-time drop |
| WhatsApp view-once, Telegram secret chats | Mainstream | Tied to phone numbers |

### 4.1 Differentiators

1. Android-native, with store-and-forward so the sender can go offline after upload
2. No-app receiving: recipient opens a link or scans a QR code in a browser (key in the link fragment)
3. Expiry as a core feature (burn-after-view or 12 hours)
4. Disposable per-share identities
5. Automatic metadata stripping (EXIF location, author fields) before upload
6. Nearby offline mode
7. Provable privacy: open source, reproducible builds, audit, published logging policy, transparency report
8. India-specific polish: Hindi and regional-language UI, small APK, resumable uploads on weak networks, tested on low-end phones
9. Abuse resistance built in

---

## 5. Feature scope

### 5.1 Version 1 (online, Android)

- One-time encrypted file or text drop
- Link and QR code sharing; key stays in the URL fragment and is never sent to the server
- Optional passphrase as a second factor
- Short 1:1 thread tied to each share (12-hour life)
- Rotating per-thread handles
- Burn-after-view option; otherwise 12-hour auto-expiry
- Automatic metadata stripping for images and common document types
- Report button and block-sender button (in-app, clearly labelled)
- Terms of use that define prohibited content
- Grievance contact page and in-app contact link
- Screenshot blocking (`FLAG_SECURE`) and hiding content in the recents screen
- File size limit and rate limits
- Proof-of-work on uploads and thread creation
- English and Hindi UI (more languages later)

### 5.2 Version 1.5

- **Nearby mode** (Android-to-Android, no internet): files and thread messages over a local hotspot after QR pairing

### 5.3 Later (only if demand exists)

- Desktop app (Tauri)
- Web receiver page (receive-only, no install)
- PC-to-phone Nearby over the same Wi-Fi or hotspot
- Optional Tor / onion-service access for high-risk users
- Optional LoRa device integration (e.g. Meshtastic) for short text over distance
- UnifiedPush notifications
- Anonymous rate-limit tokens (Privacy Pass style)

### 5.4 Deliberately excluded

- General chat, groups, contact lists, public profiles, avatars
- User search or discovery; random matching or "meet new people"
- In-app coins, gifts or paid "private" interactions
- Secret admin monitoring or any undisclosed data collection
- Government or military positioning; claims like "untraceable" or "no logs"
- Custom cryptography
- Google services dependencies
- Device fingerprinting for "one account per device"
- Persistent identities or global usernames
- Backups and multi-device sync
- Multi-hop mesh and long-distance offline (not feasible on phones alone)
- iOS in v1

---

## 6. Architecture overview

### 6.1 Principle

Put everything security-critical (encryption, key handling, thread protocol, expiry rules, server protocol) in **one shared Rust core**. Platform apps are thin interfaces on top. The server never sees plaintext, keys or file contents.

### 6.2 High-level diagram

```
                +------------------------------------------+
                |              Shared Core (Rust)          |
                |  crypto | handshake | thread | expiry    |
                |  metadata stripping | protocol | tests   |
                +------------------+-----------------------+
                                   |
         +-------------------------+--------------------------+
         |                         |                          |
  Android app (Kotlin)     Desktop app (Tauri)      Web receiver (WASM, later)
         |                         |                          |
         +----------+--------------+--------------------------+
                    | HTTPS (ciphertext only)
                    v
          +-------------------------------+
          |     Relay Server (Rust)       |
          |  blob store | mailboxes | TTL |
          |  rate limits | PoW verifier   |
          |  delete-by-ID | admin tool    |
          +---------------+---------------+
                          |
              Object storage in India (S3-compatible)

   Nearby mode (Android <-> Android): no server
   QR pairing -> local hotspot -> encrypted direct link
```

### 6.3 Components

| Component | Technology | Responsibility |
|---|---|---|
| Core library | Rust, libsodium bindings, vetted protocol library | Encryption, key exchange, thread state machine, expiry logic, metadata stripping, test vectors |
| Android app | Kotlin, Jetpack Compose, UniFFI bindings to core | UI, camera/QR, storage, Nearby mode, OS integration |
| Desktop app | Tauri (Rust backend plus lightweight web UI) | Same flows as Android, online mode only in v1 |
| Web receiver (later) | WASM build of core, or libsodium.js | Receive-only page for people who don't install anything |
| Relay server | Rust (axum), shares protocol crate with the core | Store ciphertext, route thread messages, enforce TTL, rate limiting, takedown by ID |
| Storage | S3-compatible object storage hosted in India (e.g. MinIO or provider equivalent) | Encrypted blobs with lifecycle expiry |
| Metadata store | Small TTL-backed key-value store (e.g. Redis or embedded) | Mailbox state, delete tokens, rate counters |
| Admin tool | CLI plus minimal internal web page | Delete by ID, log takedown requests and actions |

### 6.4 Why Rust for the core

- One implementation shared by Android, desktop and server
- Memory safety suits security-critical code
- Good bindings: UniFFI generates Kotlin bindings; Tauri is native Rust; WASM compiles for the web

---

## 7. Detailed design

### 7.1 No-accounts model

- There are no user accounts, no logins, no phone numbers and no email in v1.
- Each share (drop) creates its own cryptographic state. Nothing links one share to another.
- The app generates a **random handle** per thread (e.g. `amber-fox-4821`) for display only. It is not an identifier the server relies on.

### 7.2 One-time drop flow

1. **Prepare.** Sender picks a file or text. The core strips metadata (EXIF, document properties) where supported.
2. **Encrypt.** The core generates a random 256-bit key `K`. The file is encrypted in chunks using libsodium's secretstream (XChaCha20-Poly1305). File name and type go inside the encrypted payload, not in the clear.
3. **Optional passphrase.** If set, derive a key with Argon2id and use it to wrap `K`.
4. **Proof-of-work.** The client solves a small puzzle before upload to throttle abuse.
5. **Upload.** The ciphertext goes to the server. The server returns a random blob ID and a delete token. It stores the ciphertext with an expiry time. It never sees `K`.
6. **Share.** The link is `https://<host>/d/<blobId>#<K-or-wrapped-K>`. Everything after `#` is never sent to the server. A QR code encodes the same link.
7. **Receive.** The recipient opens the link in the app or browser, downloads the ciphertext, decrypts locally.
8. **Burn-after-view.** The server deletes the blob after the first completed download. A confirm step (not a plain GET) prevents link-preview bots from consuming the one-time view.
9. **Expiry.** If burn-after-view is off, the server deletes the blob after 12 hours (TTL plus storage lifecycle rule). The sender can also delete early using the delete token.

### 7.3 Thread flow (short 1:1 conversation per share)

- A thread is created together with a drop and shares its link or QR code.
- **Key agreement:** an authenticated handshake using ephemeral keys (Noise protocol via the Rust `snow` library, or libsignal). The final choice is made in the Phase 1 spike and reviewed by the auditor. The rule is to use a vetted library, never custom code.
- **Transport keys** provide session-level forward secrecy within the thread.
- **Mailboxes:** the server holds two random mailbox IDs per thread (one per direction). Mailbox access uses capability tokens derived during the handshake. The server routes ciphertext and cannot read it.
- **Delivery:** long-polling or WebSocket while the app is open. No push notifications in v1 (avoids FCM; optional UnifiedPush later).
- **Lifetime:** the thread, its mailboxes and keys are deleted after 12 hours or when either side closes it. Handles disappear with it.

### 7.4 Identity and handles

- The handle is local to the thread, randomly generated from a word list plus digits, and not unique across the system.
- There are no global usernames and no contact book.
- Reconnecting with the same person requires a new link or QR code.

### 7.5 Anti-abuse design

| Control | Detail |
|---|---|
| Proof-of-work | Required on drop upload and thread creation; difficulty adjustable under attack |
| Rate limits | Coarse per-IP limits with short-lived counters, not stored long term |
| Size limits | Per-file cap (value to be set after cost testing, e.g. 50-100 MB) |
| Short life | 12-hour maximum in v1 |
| Reporting | Recipient taps Report, and the app sends the decrypted content plus key to the report endpoint **only at that moment and only by the reporter's choice**, so a reviewer can see it before it vanishes |
| Blocking | Recipient can block the sender's handle for that thread and report the link |
| Terms | Prohibited content clearly defined (required by Google Play) |
| Server-side deletion | By blob or thread ID on valid request |
| Transparency report | Published counts of requests received and actions taken |

Note: because content is encrypted, the server cannot scan it. Moderation depends on recipient reports and legal orders. Say this plainly in the terms and policy.

### 7.6 Data minimisation and logging

Data the server may hold:

- Ciphertext blobs (until expiry)
- Random IDs, expiry times, delete tokens
- Short-lived rate-limit counters
- Operational and security logs (see below)

Data the server must **not** hold: names, phone numbers, emails, contact graphs, plaintext, encryption keys, permanent device identifiers.

**Logging reality in India:** CERT-In's April 2022 directions require covered entities to keep ICT system logs for a rolling 180 days within India and report listed incidents within 6 hours. Scope for a minimal service needs lawyer confirmation. Therefore:

- Do not claim "no logs".
- Keep security and access logs only, never content, stored in India, with a stated retention period.
- Disclose exactly what is logged in the privacy policy and the repository.

IP addresses and device identifiers count as personal data under India's DPDP Act even without names. Keep them for as short a time as legally allowed.

### 7.7 Notifications

- No FCM. v1 uses foreground polling while the app is open.
- Optional later: UnifiedPush with content-free wake-ups.

### 7.8 Nearby mode (offline, short range)

**Goal:** two Android phones near each other exchange files and thread messages with no internet and no server.

**Flow:**

1. Sender taps "Nearby". The app starts a temporary local-only hotspot (Android `LocalOnlyHotspot`, API 26+) and generates a one-time session secret plus an ephemeral public key.
2. The app shows a QR code containing: hotspot SSID and password, host address and port, the session secret, the sender's ephemeral public key, and an expiry.
3. Receiver scans the QR code, joins the hotspot (`WifiNetworkSpecifier`, API 29+) and connects.
4. Both sides run the same handshake as online threads, using the QR secret for authentication. The secret travels visually, so nearby eavesdroppers cannot obtain it.
5. File and thread messages transfer directly. Same encryption and 12-hour expiry logic from the shared core.
6. On finish, the hotspot is shut down and session keys are deleted.

**Permissions and platform notes:** camera (QR), nearby Wi-Fi devices permission on Android 13+, location permission on older versions where required. Behaviour varies across phone manufacturers, so budget real testing time on low-end Indian handsets.

**Fallback:** if both devices are on the same Wi-Fi or one is running a normal hotspot, connect over the local network using the same QR handshake.

**Avoid:** Google's Nearby Connections library (requires Play Services, conflicts with D10).

**Limits to state honestly:**

- Range is roughly tens of metres.
- Android-to-Android only in v1.5; iPhone interoperability is not planned.
- The other party could run a modified app that ignores expiry.
- No server means no moderation or takedown path; get legal advice on how peer-to-peer mode is treated.

**Not promised:** long-distance offline. Phone radios (Bluetooth, Wi-Fi) cannot do it. Options such as multi-hop mesh, LoRa devices (e.g. Meshtastic, short text only), satellite, or physical carry (store-and-forward) are deferred. Mesh has had published security flaws and leaks that a device is transmitting.

### 7.9 Desktop considerations

- Phones have hardware-backed key storage and sandboxing. Desktops have swap files, crash dumps, backups and malware. Expiry and secure deletion are weaker there, and the app must say so.
- Distribution needs code signing: a Windows certificate and macOS notarization. Unsigned apps show warnings.
- v1 desktop supports online mode only. Nearby is Android-to-Android first.

### 7.10 Threat model summary

| Threat | Mitigation | Residual risk |
|---|---|---|
| Server breach | Only ciphertext stored; keys in URL fragment | Metadata (timing, IP) if retained |
| Malicious operator | Open source, reproducible builds, audit | Users must verify the build |
| Network eavesdropper | TLS plus end-to-end encryption | Traffic analysis |
| Link interception | Optional passphrase; short expiry; burn-after-view | Whoever holds the link can open it |
| Recipient keeps a copy | Cannot be prevented (screenshots, camera) | Stated openly in app and policy |
| Modified client ignores expiry | Cannot be prevented client-side | Stated openly |
| Abuse and spam | Proof-of-work, limits, reports, takedown | Persistent abusers shifting IPs |
| Device compromise | Platform security only | Out of scope |

---

## 8. Technology stack summary

| Layer | Choice |
|---|---|
| Core | Rust; libsodium (secretstream, X25519, Argon2id); Noise (`snow`) or libsignal for threads |
| Android | Kotlin, Jetpack Compose, UniFFI bindings, CameraX plus ML-free QR scanner (avoid Google ML Kit) |
| Desktop | Tauri |
| Web receiver | WASM build of core or libsodium.js |
| Server | Rust (axum) |
| Storage | S3-compatible in India; TTL-based metadata store |
| Distribution | Google Play, F-Droid, direct APK; reproducible builds |
| CI | GitHub Actions or self-hosted; dependency scanning; signed releases |
| Licence | Open source (AGPL-3.0 is compatible with libsignal; choose deliberately) |

### 8.1 Suggested repository layout

```
ember/
  docs/
    protocol-spec.md        # messages, keys, expiry rules, test vectors
    threat-model.md
    privacy-and-logging.md  # exactly what the server sees and keeps
    takedown-process.md
  core/                     # Rust crate: crypto, protocol, expiry, metadata stripping
  server/                   # Rust relay server
  android/                  # Kotlin app
  desktop/                  # Tauri app
  web-receiver/             # later
  tools/
    admin-cli/              # delete-by-ID, takedown logging
  LICENSE
  SECURITY.md               # vulnerability disclosure policy
```

---

## 9. Legal, store and compliance plan (India focus)

### 9.1 Google Play

- Apps with user-generated content must provide clear terms of use, an in-app system for reporting objectionable content and users, and a function to block users. Terms must define objectionable content.
- Complete the Data Safety form accurately, publish a privacy policy, and set an accurate content rating.
- Google's developer verification links real-world identities to apps. It began on 30 September 2026 in Brazil, Indonesia, Singapore and Thailand; global rollout is planned for 2027 with no India date published as of early November 2026. Register as an organisation.
- Google Play has no encryption questionnaire, but US export law still applies (see 9.5).

### 9.2 Apple App Store (later)

- Guideline 1.2: apps used mainly for random or anonymous chat can be removed. Required: filtering of objectionable material, a reporting mechanism with timely responses, blocking, and published contact information.
- Keep the product positioned as file transfer with a short thread, no discovery or matching.
- Answer the export compliance questions in App Store Connect.

### 9.3 Indian intermediary rules (IT Act and IT Rules)

- An intermediary is any service that stores or transmits third-party content. The 2026 amendments (in force from 20 February 2026) require compliance with a lawful takedown direction within **3 hours** (previously 36), removal of certain non-consensual intimate imagery within **2 hours**, general grievance resolution within **7 days**, and certain urgent complaints within **36 hours**.
- Safe harbour depends on due diligence. Knowingly failing to act can lose it.
- Obligations for larger platforms (50 lakh+ users) are heavier (chief compliance officer, resident grievance officer, traceability duties under Rule 4(2)). Rule 4(2) is under challenge in court, and a draft second amendment proposes broader scope. Monitor it.

**What this means in practice (takedown without reading content):**

1. Every blob and thread has an ID.
2. An authority sends a link or ID to the published grievance email.
3. An operator deletes that encrypted item using the admin tool within the deadline and replies.
4. The request and action are recorded.

You need a monitored contact, a named Indian grievance officer, and a documented process before launch.

### 9.4 Other Indian requirements

| Law or rule | Relevance |
|---|---|
| DPDP Act 2023 and DPDP Rules 2025 | Main obligations (notice, consent, security safeguards, breach reporting within 72 hours, children's data) start 13 May 2027. Build for them now. IP and device identifiers are personal data. |
| CERT-In Directions (28 April 2022) | 6-hour incident reporting; 180-day rolling log retention in India; point-of-contact registration |
| DoT SIM-binding directions (issued 28 November 2025) | Aimed at messaging apps tied to phone numbers; reported implementation deadline 31 December 2026. A no-phone-number app may fall outside the literal text, but this runs against policy direction. Get a written lawyer opinion. |
| Telecommunications Act 2023 and Telecom Cyber Security Rules | Basis for the SIM-binding directions |
| IT Act s.69 (interception and decryption orders) | Ask a lawyer how it applies to an E2EE service that cannot decrypt |
| Official Secrets Act | Another reason not to market for classified material |
| POCSO and mandatory-reporting duties | Relevant to reports of child sexual abuse material; the amended rules also expect periodic user notifications about reporting duties |

Context: regulators are tightening anonymous and encrypted apps. India temporarily restricted Telegram in June 2026, and legal commentary cites anonymity plus encryption as a concern.

### 9.5 Export compliance

- Apps that use encryption count as exports of encryption software under US rules, and both stores ask about it.
- Using standard, open-source cryptography and publishing open-source code generally eases requirements, but classification (for example ECCN 5D992 for mass-market versus 5D002) and any BIS notification or self-classification report must be confirmed by an adviser.

### 9.6 Required documents before launch

- Privacy policy (what is collected, retention, logging)
- Terms of use with prohibited-content definitions
- Grievance and takedown policy, with the named officer and contact
- Security policy (`SECURITY.md`) and responsible-disclosure channel
- Transparency report template
- Written legal opinion on SIM binding, intermediary status and peer-to-peer mode

---

## 10. Moderation and takedown process

| Step | Action | Owner | Target time |
|---|---|---|---|
| 1 | Receive report or legal order at grievance email | Grievance officer | Monitored continuously |
| 2 | Verify authority and identify the blob or thread ID | Operator | Within 1 hour |
| 3 | Delete ciphertext via admin tool | Operator | Within the legal deadline (3 hours for government or court orders, 2 hours for certain intimate imagery) |
| 4 | Record request, action and time | Operator | Immediately |
| 5 | Reply to the requester | Grievance officer | Same day |
| 6 | Include in transparency report | Founder | Periodic |

For user reports: the report flow shares decrypted content with the reviewer only when the reporter chooses to send it. Child-safety material is handled under mandatory reporting duties with legal guidance.

---

## 11. Roadmap from zero

Estimates assume a small team or a dedicated solo developer plus part-time legal and design support. Adjust to your reality.

### Phase 0: Foundations (weeks 0-4)

- Register an Indian company; open a Play Console organisation account; complete developer verification
- Create the public repository, licence and `SECURITY.md`
- Draft privacy policy, terms of use, prohibited-content list, grievance and takedown policy
- Set up the grievance email and name the grievance officer
- Book an Indian technology lawyer for the written opinion (SIM binding, intermediary status, P2P mode)
- Choose the final name; check trademarks and domains

### Phase 1: Protocol and core (weeks 3-8)

- Write the protocol spec with test vectors
- Build the Rust core: drop encryption and decryption, passphrase wrapping, metadata stripping
- Spike thread key exchange (Noise versus libsignal); pick one
- Command-line test client; automated test suite

### Phase 2: Server (weeks 6-12)

- Relay server with blob storage, mailboxes, TTL deletion, burn-after-view, delete-by-ID
- Proof-of-work verification, rate limits, size caps
- Admin CLI for takedown
- Log policy implemented as documented
- Deploy to infrastructure in India; load and abuse testing

### Phase 3: Android MVP (weeks 8-16)

- Compose UI: send, receive, thread, handle display, expiry timers
- QR scan and generation
- Report and block flows; terms acceptance; contact link
- `FLAG_SECURE`, recents hiding, Hindi and English
- Test on low-end devices and weak networks; resumable uploads

### Phase 4: Beta, audit and launch prep (weeks 16-24)

- Closed beta with journalists, lawyers and NGOs
- Independent security audit of core, protocol and server; fix findings and publish the report
- Play Console listing, Data Safety form, content rating, store screenshots
- Reproducible build verification; F-Droid listing
- Launch on Google Play and F-Droid (Android only)

### Phase 5: Nearby mode (weeks 24-32)

- Hotspot-based pairing and transfer between Android phones
- Cross-brand testing
- Lawyer review of P2P-specific obligations

### Phase 6: Desktop (weeks 30-42)

- Tauri app with the shared core, online mode
- Code signing and notarization
- Optional web receiver
- Optional PC-to-phone Nearby over a shared network

### Phase 7: Explore (after traction)

- iOS and App Store compliance
- Tor access, UnifiedPush, anonymous rate-limit tokens
- LoRa integration for short text, multi-hop relay only if demand and security design justify it

---

## 12. Risks and mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Abuse (scams, harassment, illegal content) | High | PoW, rate limits, short expiry, reporting, takedown, narrow positioning, transparency |
| Store removal or rejection | High | File-transfer positioning, mandatory moderation features, terms, no discovery or random matching |
| Regulatory action in India | High | Company registration, grievance officer, lawyer opinion, documented process, honest claims |
| Crypto implementation mistakes | High | Audited libraries only, test vectors, independent audit |
| Server compromise | Medium | Ciphertext only, minimal metadata, hardening, incident process (6-hour reporting) |
| Overpromising | Medium | Disciplined wording; explicit limits in app and listing |
| Nearby-mode fragmentation across phones | Medium | Dedicated testing budget; clear device compatibility list |
| Cost of storage and bandwidth | Medium | Size caps, 12-hour TTL, PoW |
| Single point of failure (solo founder) | Medium | Documentation, open source, second maintainer |

---

## 13. Open questions to resolve early

1. Final name, trademark and domain.
2. Noise (`snow`) versus libsignal for threads; licence choice (AGPL implications).
3. File size cap and hosting cost model.
4. Free versus paid model (if any); no ads, no trackers.
5. Exact log fields and retention period, confirmed with a lawyer.
6. Whether Nearby mode (P2P, no server) changes intermediary obligations.
7. Whether to support recipient-less "public link" drops, or require each share to create a thread.
8. Whether burn-after-view is the default or opt-in.
9. Who is the named grievance officer and how are orders monitored out of hours?

---

## 14. Sources consulted (research snapshot, November 2026)

- Google Play Console Help: User Generated Content policy and moderation requirements
- Apple App Store Review Guidelines 1.2 (as quoted in MediaNama, June 2026, on Vibely's removal and anonymous-chat apps)
- Android developer verification rollout (Help Net Security, 19 June 2026; Android developer documentation; commentary dated 7 November 2026)
- IT (Intermediary Guidelines and Digital Media Ethics Code) Amendment Rules 2026 (Mondaq, iPleaders, Bar and Bench, Remfry and Sagar)
- DPDP Act 2023 and DPDP Rules 2025 phased commencement (Mondaq, AZB, Hogan Lovells, TaxGuru)
- CERT-In Directions of 28 April 2022 (Legal500, Mondaq, PSA Legal)
- DoT SIM-binding directions for messaging apps (Outlook Business, Inc42, India Briefing, Privacy Savvy, JSA via IBA)
- Internet Freedom Foundation statement on traceability under Rule 4(2)
- Telegram restriction in India, June 2026 (Mondaq, The Tax Corp)
- App Store and Play export compliance guidance (Apple developer documentation; Cossack Labs; industry write-ups)
- Privacy Guides and related lists on secure file sharing (OnionShare, Send forks, Proton Drive, Magic Wormhole, SecureDrop)

Re-verify every date and rule before relying on it.
