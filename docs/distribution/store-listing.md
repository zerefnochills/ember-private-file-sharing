# Google Play & App Store Listing

## App Title
**Ember: Private File Drop**

## Short Description (80 characters max)
Send files privately. End-to-end encrypted, account-free, auto-expires in 12h.

## Tagline
Send it once. Only they can open it.

---

## Full Description

Ember is a focused tool for sending files and text drops directly to someone else with end-to-end encryption. No account required. No phone numbers. No contact lists.

Share sensitive documents, high-resolution scans, notes, or credentials with confidence. Once delivered, your share automatically expires.

### Key Capabilities

* **Local End-to-End Encryption:** Your files are encrypted on your device before upload using audited libsodium-compatible cryptography (`crypto_secretstream_xchacha20poly1305`). The decryption key lives exclusively in the link fragment on your device and is never sent to or stored by our relay server.
* **Account-Free & Clean:** No phone number, email address, password, or profile registration is ever requested.
* **Automatic Expiration:** Drops automatically expire and are permanently erased after 12 hours.
* **Burn-After-View:** Choose burn-after-view to permanently remove files the moment the recipient finishes downloading.
* **Metadata Stripping:** Automatically removes GPS location data, camera serial numbers, and author tags from photos and documents before encryption.
* **Short 1:1 Private Threads:** Need to confirm receipt or exchange a follow-up? Start an optional, temporary chat thread protected by forward-secret ratchets. Each thread uses disposable pseudonyms (like `amber-fox-4821`) and disappears when expired or closed.
* **No App Needed for Recipients:** Recipients can open your drop link in any standard modern web browser or scan the QR code to decrypt the file directly.
* **Screen & Snapshot Protection:** Protected with Android `FLAG_SECURE` to prevent screen captures and hide drop contents in recent app switchers.
* **Abuse Controls:** In-app reporting and blocking features enable users to report violations directly.

---

## Google Play Data Safety Declaration

Based strictly on the actual codebase implementation:

### Data Collection & Sharing

| Question | Answer | Details |
|---|---|---|
| **Does your app collect or share user data?** | **No personal data collected** | Ember requires no user accounts, names, emails, phone numbers, or device IDs. |
| **Is data encrypted in transit?** | **Yes** | All data is end-to-end encrypted client-side using XChaCha20-Poly1305 before transmission over HTTPS. |
| **Can users request data deletion?** | **Yes (Automatic & Manual)** | All content automatically self-deletes within 12 hours. Senders can manually delete drops at any time using their delete token, or configure burn-after-view. |
| **Location data?** | **Not collected** | Location permissions are never requested; GPS data in photos is stripped before encryption. |
| **Financial / Health / Contacts info?** | **Not collected** | Zero financial or contact data accessed. |
| **Third-party trackers or SDKs?** | **None** | Zero advertising libraries, zero Google Play Services / Firebase analytics. |

---

## Content Rating & Categorization
* **Category:** Tools / Productivity
* **Content Rating:** Everyone (PEGI 3 / ESRB Everyone) with user-to-user content interactivity disclosures and built-in moderation reporting tools.
