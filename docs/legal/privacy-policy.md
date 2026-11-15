# Ember Privacy Policy

> **Status: DRAFT — Pending Legal Review**
> *This document has been prepared for legal counsel review in accordance with the Digital Personal Data Protection Act (DPDP Act) 2023, the Information Technology Act 2000, and the Information Technology (Intermediary Guidelines and Digital Media Ethics Code) Rules 2021.*

---

## 1. Introduction

Ember ("we", "us", or "the Service") is a private, account-free, end-to-end encrypted file and message drop service operated by Ember Technologies.

Our fundamental architectural design is minimal data retention:
* We do **not** require registration, email addresses, phone numbers, or user accounts.
* We do **not** know who you are, who you share with, or what files you transfer.
* All file and text content is encrypted on your local device before transmission. The decryption key exists solely in the link fragment on your device and is **never** transmitted to or stored by our servers.

---

## 2. Information We Process

### 2.1 Information We Never Receive or Store
Because encryption and decryption occur entirely client-side:
* **Plaintext Content:** We cannot read, view, or parse the contents of any file or text drop.
* **File Names & Types:** File metadata is encrypted inside the ciphertext payload before reaching our servers.
* **Decryption Keys:** Cryptographic keys and URL fragments (everything following `#` in a link) remain solely on the sender and recipient devices.
* **Persistent Identifiers:** We do not collect device identifiers (IMEI, Android ID, MAC address, advertising ID) or link usage across sessions.

### 2.2 Ephemeral Data Processed During Relaying
When you upload or download an encrypted drop, our relay server temporarily processes:
1. **Encrypted Blob Payload:** Opaque ciphertext chunks (`crypto_secretstream_xchacha20poly1305`), stored temporarily for up to 12 hours or until burned upon download (whichever occurs first).
2. **Drop Lifecycle Metadata:** Random 128-bit blob identifiers, encrypted part count, creation timestamp, expiry timestamp (maximum 12 hours from creation), and one-way cryptographic hashes of authorization tokens (claim tokens and delete tokens).
3. **Proof-of-Work Nonces:** Stateless SHA-256 hashcash challenges used exclusively to prevent denial-of-service and automated spam. Nonces are checked and cached in-memory for 15 minutes to prevent replay attacks, then discarded.
4. **Ephemeral Thread Mailboxes:** If a private 1:1 chat thread is initiated, two directional mailboxes store opaque ciphertext messages for up to 12 hours. Pseudonymous handles (e.g. `amber-fox-4821`) are generated client-side and are **never** transmitted to the server.

### 2.3 Operational Server Logs
To maintain service reliability and comply with applicable laws (including CERT-In Directions 2022):
* **Log Fields:** Request method, route template (e.g. `/v1/blobs/{id}/parts/{n}` with specific resource IDs stripped), HTTP status code, request duration in milliseconds, coarse response size bucket (`<1KB`, `1KB-10KB`, `10KB-1MB`, `1MB-10MB`, `10MB-50MB`, `>50MB`), and client IP address.
* **IP Address Handling:** IP addresses are recorded in server access logs strictly for rate limiting, DDoS defense, and mandatory rolling retention. In rate-limiting memory tables, IPv6 addresses are truncated to `/64` prefixes and stored in memory only.
* **Zero Secret Invariant:** Our server logging pipeline enforces automated suppression ensuring no cryptographic keys, URL fragments, file names, or plaintexts can ever appear in server logs.

---

## 3. Data Retention and Deletion

* **Drops:** Automatically and permanently deleted from server storage upon expiration (maximum 12 hours from upload) or immediately after successful download if "burn-after-view" is enabled. When a drop is deleted, tombstones are marked and storage blocks are unlinked.
* **Mailbox Messages:** Automatically deleted upon thread closure or after 12 hours from creation.
* **Abuse Reports:** When a user explicitly files an abuse report, the reporter's client provides the decrypted content and decryption key directly to our moderation review queue. Abuse reports are stored in an isolated, encrypted partition and are automatically purged after 30 days.
* **Operational Logs:** Server access logs are retained in India on a rolling 180-day retention schedule as required by CERT-In Directions, after which they are permanently overwritten or deleted.
* **Takedown Audit Log:** Records of administrative takedown actions (timestamp, legal request reference, affected blob/mailbox ID, and operator ID) are maintained in an append-only audit log for 3 years to satisfy statutory compliance obligations.

---

## 4. Purpose and Legal Grounds for Processing (DPDP Act 2023)

We process technical data solely for:
1. **Service Delivery:** Transmitting encrypted payloads at the request of the user.
2. **System Security & Integrity:** Enforcing proof-of-work and sliding-window rate limits to mitigate abuse and denial-of-service attacks.
3. **Legal Compliance:** Fulfilling statutory duties under the IT Act 2000, IT Rules 2021, and CERT-In Directions 2022.

Because Ember requires no user accounts, phone numbers, or registration, we do not engage in behavioral advertising, cross-app tracking, user profiling, or selling of personal data.

---

## 5. User Rights

Under the DPDP Act 2023 and applicable privacy laws, users have rights regarding personal data:
* **Right of Access & Correction:** Because all content is encrypted end-to-end with keys known only to you, and we store no user accounts, we hold no user profiles to query, correct, or export.
* **Right to Erasure:** Senders may delete an uploaded drop prior to expiry using the delete token issued upon upload, or recipients may burn the drop immediately upon viewing.
* **Grievance Redressal:** You may contact our designated Grievance Officer regarding any data protection or privacy concerns.

---

## 6. Security Safeguards

We implement rigorous cryptographic and operational safeguards:
* **End-to-End Cryptography:** Libsodium-compatible primitives (`dryoc`): XChaCha20-Poly1305 secretstream, Argon2id key wrapping, and forward-secret ratchet chains.
* **Memory Safety:** Rust codebase with `#![forbid(unsafe_code)]` in cryptographic and core protocol crates.
* **Automated Stripping:** Pre-encryption stripping of EXIF GPS, device serial numbers, and author metadata on supported image and document formats.
* **No Third-Party Trackers:** Zero third-party SDKs, zero analytics trackers, and zero proprietary Google Play Services or Firebase components.

---

## 7. Contact & Grievance Redressal

In compliance with Rule 3(2) of the Information Technology (Intermediary Guidelines and Digital Media Ethics Code) Rules, 2021:

**Grievance Officer:**
* Name: *[Designated Grievance Officer — Pending Appointment]*
* Designation: Grievance Officer, Ember Technologies
* Email: `grievance@ember.local` *(to be configured prior to production)*
* Physical Address: *[Registered Office Address in India — Pending Incorporation]*

We acknowledge complaints within twenty-four (24) hours and resolve grievances within fifteen (15) days of receipt.
