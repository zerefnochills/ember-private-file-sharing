# Ember: What the Server Sees and Keeps

**Status:** Draft v1, milestone M1. **Every retention period below is a proposal pending written advice from an Indian technology lawyer** (CERT-In log rules, DPDP Act obligations, IT Rules). Nothing here is legal advice. Do not publish this as a final privacy policy until reviewed.
**Principle:** collect the minimum needed to run the service and meet legal duties, say exactly what it is, and delete it on schedule. Never claim "no logs".

---

## 1. Data the server MAY hold

| Data | Why | Holds | Proposed retention |
|---|---|---|---|
| Ciphertext blob parts | The service itself | Encrypted bytes only | Until expiry (12 hours at most), burn, or early delete |
| Blob record | Enforce expiry and limits | Random blob ID, created time, expiry time, total size, part count, burn flag, state | Deleted with the blob |
| Delete token hash | Authorise early delete and uploads | Hash only, never the token | Deleted with the blob |
| Claim record (burn blobs) | One-time download | Hash of the claim token, claim expiry | Deleted with the blob |
| Rate-limit counters | Throttle abuse | Short-lived counters keyed by network address (or a truncated form) | Minutes (target 15 minutes), memory only where possible |
| Proof-of-work challenge records | Prevent replay | Challenge ID, expiry | Minutes |
| Access and security log | Operate, secure and investigate incidents; meet CERT-In style duties | See section 2 | **Proposal:** 180 days, stored in India, only if the lawyer confirms CERT-In applies to this service. Otherwise the shortest period the lawyer approves (suggested 30 days or less) |
| Report records (user presses Report) | Moderation | Content and key sent by the reporter at that moment, blob or thread ID, reporter's note, time | **Proposal:** 30 days after the report is resolved, then deleted. Child-safety material follows legal reporting duties and counsel's advice |
| Takedown log | Prove compliance with orders | Order reference, who issued it, blob or thread ID, action taken, time actioned | **Proposal:** retained as long as the lawyer advises (placeholder: 3 years). Contains no user content |
| Transparency report counts | Publish accountability | Aggregate counts only | Permanent (no personal data) |

## 2. Fields in the access and security log

The log is deliberately narrow. Each line may contain only:

- UTC timestamp
- Client network address (IP): the TCP peer address, or the forwarded address only when the peer is a configured trusted reverse proxy. **Personal data under the DPDP Act**
- HTTP method
- Route **template** (for example `/v1/blobs/{id}/parts/{n}`), NOT the actual blob ID
- HTTP status code
- Response size (rounded to a bucket)
- Request duration
- Result of rate-limit and proof-of-work checks (pass or fail, without challenge details)

Because blob IDs are not logged, a log line cannot be tied to a particular drop. Whether this is acceptable for incident response and legal duties is a **question for the lawyer** (open question 5 in the blueprint).

**Implementation status (16 Nov 2026 review):** the access log is currently an in-memory ring of the most recent 10,000 entries and is not persisted. Any retention period promised in this document (for example 180 days) therefore needs a persistence and deletion job, which does not exist yet. Do not publish a retention promise until it does.

## 3. Data the server MUST NOT hold

- Encryption keys, the link secret K, the link fragment, passphrases
- File names, media types, or any plaintext content
- Names, phone numbers, email addresses, contact lists
- Persistent device identifiers, advertising IDs, fingerprints
- Cookies or sessions (there are no accounts)
- Request bodies or full URLs in any log
- Location data

Server-side code MUST NOT write these anywhere, including debug logs, crash dumps, metrics labels, traces and error messages. Third-party analytics and crash-reporting SDKs are not used.

## 4. Rules for code (enforced by tests)

1. A shared logging module is the only way to write logs; it accepts only the fields in section 2.
2. Tests (milestones M3 and M8) send requests containing a fake key, fake fragment, fake passphrase and fake filename, then search all logs, stdout, error output and storage for those strings. The test fails if any appear.
3. Debug logging is off in production and can never include request bodies.
4. Retention is enforced by a scheduled job with its own test and a recorded run result.
5. Logs and backups are stored in India (hosting location is an open item, see `EMBER_BUILD_GUIDE.md` section 8).

## 5. Client-side data

| Data | Where | Notes |
|---|---|---|
| Drafts, received files | App-private storage | Deleted on expiry or by the user; Android backup excluded |
| Thread handle | Memory and app storage until the thread closes | Local only, never used by the server as an identifier |
| Link secret and keys | Memory; hardware-backed storage where applicable | Zeroised after use |
| No account, no profile, no analytics | n/a | The app makes no requests to any host other than the configured Ember server (verified by a proxy test in M11) |

## 6. Who may see what

| Role | May access | May NOT access |
|---|---|---|
| Server operator | Ciphertext, blob records, logs per section 2 | Plaintext, keys, file names |
| Admin tool user (takedown) | Delete by ID, takedown log | Content |
| Reviewer (reports) | Only what the reporter chose to send | Anything else |
| Public | Nothing without the link | n/a |

No hidden monitoring exists or may be added (blueprint D8).

## 7. Legal hooks to confirm with counsel

- CERT-In Directions of 28 April 2022: 180-day rolling logs within India, 6-hour incident reporting, point-of-contact. Does it apply to a minimal service, and which fields?
- DPDP Act and Rules: notice and consent, security safeguards, breach reporting within 72 hours; main duties start 13 May 2027 (re-verify). IP addresses count as personal data.
- IT Rules 2026 amendments: takedown timelines (3 hours for orders, 2 hours for certain intimate imagery), grievance resolution (7 days), safe-harbour conditions.
- Child-safety mandatory reporting (POCSO).
- How peer-to-peer Nearby mode (v1.5) changes obligations.

## 8. Changes to this document

Any new data field, longer retention or new third-party service requires:
1. An ADR in `docs/adr/`,
2. An update to this file and the published privacy policy,
3. A matching test from section 4,
4. Review by the owner and, before release, the lawyer.
