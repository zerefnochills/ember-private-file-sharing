# Ember Moderation and Takedown Operational Procedure

**Status:** Draft v1, Milestone M8  
**Applicability:** Ember Relay Server Operators, Grievance Officer, Reviewers, Administrators

This document outlines the standard operating procedures for executing content takedowns, handling user abuse reports, maintaining the append-only audit log, and generating transparency statistics in compliance with Indian Information Technology (Intermediary Guidelines and Digital Media Ethics Code) Rules, 2021 (as amended 2026), Bharatiya Nyaya Sanhita (BNS), and the Digital Personal Data Protection Act (DPDP), 2023.

---

## 1. Guiding Principles & Architecture

Ember is designed around zero-knowledge end-to-end encrypted file drops and ephemeral threads.
Under this architecture:
1. **The Server Never Holds Plaintext:** The server only stores opaque ciphertext chunks, random 128-bit identifiers, and cryptographically hashed tokens.
2. **Takedown Without Reading Content:** Operators do not need to (and cannot) decrypt stored drops. Deletion is executed strictly against target identifiers (`blob_id` or `mailbox_id`).
3. **No Retaliation / Re-access:** Deleting a blob permanently wipes its ciphertext and records an immediate tombstone so that all subsequent requests return HTTP 410 Gone.
4. **Separation of Concerns:** The append-only takedown log is kept strictly isolated from user drop data and contains zero user content, zero keys, and zero file names.

---

## 2. Statutory Timelines & Triage SLA

Under Rule 3(2)(b) and related provisions of the IT Rules:

| Category | Trigger / Authority | Target Action Deadline | Responsible Role |
|---|---|---|---|
| **Non-Consensual Intimate Imagery (NCII)** | Grievance complaint by victim or authorized representative | **Within 2 hours** from receipt | Grievance Officer & Operator |
| **Court / Government Lawful Orders** | Formal requisition under s.69A IT Act, court order, or authorized agency order | **Within 3 hours** from receipt | Grievance Officer & Operator |
| **Child Sexual Abuse Material (CSAM)** | Mandatory report under POCSO / Cybercrime portal | **Immediate action & statutory reporting** | Grievance Officer |
| **Urgent Complaints** | Imminent harm or severe rights infringement | **Within 36 hours** | Grievance Officer |
| **General Grievances** | User report or third-party complaint | **Within 7 days** | Grievance Officer |

---

## 3. Step-by-Step Takedown Workflow

```
[ Requisition / Grievance Received ]
                 │
                 ▼
     [ Step 1: Authenticate Order ]  (Verify court/govt seal, ticket signature)
                 │
                 ▼
   [ Step 2: Extract Target ID ]    (Parse blob_id or mailbox_id from link)
                 │
                 ▼
 [ Step 3: Execute Admin Takedown ]  (Run `ember-admin takedown`)
                 │
                 ▼
 [ Step 4: Verify Ciphertext Purge ] (Assert 410 Gone; verify tombstone)
                 │
                 ▼
   [ Step 5: Formal Notification ]   (Dispatch compliance confirmation to authority)
```

### Step 1: Verification
Upon receipt of a takedown order or grievance at the published grievance email (`grievance@ember.internal`):
1. The Grievance Officer verifies the legitimacy of the issuing court, law enforcement agency, or reporting victim.
2. Extract the target URL or identifier. A valid link matches:
   `https://<server>/d/<blob_id>#<key>` or `ember://<blob_id>#<key>`.
3. Note that the fragment (`#<key>`) is discarded by the operator—only the `blob_id` is needed for takedown.

### Step 2: Execution via `ember-admin`
The operator issues the deletion using the administrative CLI:
```bash
export EMBER_ADMIN_TOKEN="<operator_bearer_token>"

# For a file drop blob:
ember-admin takedown \
  --server https://relay.ember.internal \
  --target "K7x2m9Lq_8AbC-def12345" \
  --target-type blob \
  --order-ref "COURT-ORD-2026-8812" \
  --operator "op-priya-01" \
  --reason "court_order"

# For an ephemeral thread mailbox:
ember-admin takedown \
  --server https://relay.ember.internal \
  --target "M8y3n0Pr_9XyZ-uvw67890" \
  --target-type mailbox \
  --order-ref "NCII-EMERGENCY-4412" \
  --operator "op-rahul-02" \
  --reason "ncii"
```

### Step 3: Technical Effects of Takedown
When the endpoint `POST /v1/admin/takedown` is invoked:
1. The server confirms the admin bearer token in constant time.
2. All stored ciphertext parts (`part_0.bin`, `part_1.bin`, ...) and metadata records are permanently deleted from storage.
3. For blobs, an immediate tombstone record is generated with a 1-hour minimum TTL (or matching remaining blob TTL) so any ongoing or subsequent client requests immediately receive HTTP 410 Gone.
4. An immutable `TakedownRecord` is appended to the `.takedowns.jsonl` audit log.

### Step 4: Formal Reply
Within the statutory window (same business day or prior to deadline expiration), the Grievance Officer sends a formal written acknowledgement citing the order reference, timestamp of execution, and confirmation that the target ciphertext has been completely purged.

---

## 4. User Abuse Reporting & Reviewer Role

Ember enables voluntary user reporting:
1. **Voluntary Reporter Sharing:** A recipient who receives abusive or prohibited material may explicitly choose to click "Report". The recipient app optionally submits an excerpt of the plaintext content and/or the link key so that the moderator can inspect the reported material.
2. **Reviewer Role Isolation:**
   - Public reporting endpoint (`POST /v1/reports`) accepts reports and returns a report ticket ID.
   - Access to report contents (`GET /v1/admin/reports`) is strictly restricted to authenticated reviewers with the reviewer bearer token.
   - Server operators without the reviewer role never inspect report contents.
3. **Strict 30-Day Retention:** Reports are stored in a dedicated `.reports/` directory with a 30-day expiry timestamp (`created_at + 30 * 86400`). The server's background sweeper automatically purges all reports past their expiration date.

---

## 5. Append-Only Takedown Log Format

The takedown log is stored in `.takedowns.jsonl`. Each record consists of:
```json
{
  "id": "T1x8K3mQ_9AbC-zxy09876",
  "timestamp": 1773789000,
  "target_id": "K7x2m9Lq_8AbC-def12345",
  "target_type": "blob",
  "request_ref": "COURT-ORD-2026-8812",
  "operator": "op-priya-01",
  "reason": "court_order",
  "action_taken": "deleted"
}
```
**Audit Rules:**
- The file is append-only. No records may ever be modified or removed.
- The log contains NO keys, NO link fragments, and NO file names.
- Retention: Maintained for 3 years (as advised by Indian legal counsel).

---

## 6. Transparency Reporting

To ensure public accountability without compromising privacy, the relay server generates aggregate statistics available via `GET /v1/transparency` and the CLI:
```bash
ember-admin transparency --server https://relay.ember.internal
```
Output:
```
=== Ember Relay Transparency Report ===
Total Takedowns Processed: 42
  - court_order: 12
  - ncii: 25
  - illegal_content: 5
Total Reports Received:    89
  - abuse: 45
  - malware: 20
  - harassment: 24
```
**Privacy Guarantee:** Transparency outputs contain only non-identifiable integers and category labels. Zero individual IDs, timestamps, or IP addresses are ever disclosed.
