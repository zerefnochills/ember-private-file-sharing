# Ember Grievance Redressal and Takedown Policy

> **Status: DRAFT — Pending Legal Review**
> *This document has been prepared for legal counsel review in accordance with the Information Technology (Intermediary Guidelines and Digital Media Ethics Code) Rules 2021, Rule 3(2) and Section 79 of the Information Technology Act 2000.*

---

## 1. Overview and Intermediary Status

[Operator legal name, pending incorporation] operates the Ember relay infrastructure as an intermediary within the meaning of Section 2(1)(w) of the Information Technology Act 2000. 

As an end-to-end encrypted technical intermediary:
1. We do not initiate the transmission, select the receiver of the transmission, or select or modify the information contained in the transmission.
2. We temporarily store encrypted payloads strictly for the technical purpose of transmission, with a hard upper bound of twelve (12) hours or upon viewer consumption (whichever is earlier).
3. We do not possess the cryptographic keys needed to inspect or decrypt files transmitted across the Service.

---

## 2. Designated Grievance Officer

In accordance with Rule 3(2)(a) of the IT Rules 2021, [Operator legal name, pending incorporation] has appointed a Grievance Officer to address user grievances, notices, and legal orders.

* **Designation:** Grievance Officer, [Operator legal name, pending incorporation]
* **Email:** `grievance@ember.local` *(designated monitored email channel)*
* **Response Address:** *[Physical Office Address in India — Pending Incorporation]*
* **Acknowledgment Window:** Within twenty-four (24) hours of receipt.
* **Resolution Window:** Within fifteen (15) days of receipt (or expedited within statutory emergency windows).

---

## 3. Statutory Takedown Timelines

Ember adheres strictly to the operational timelines mandated by Indian law:

| Category of Request | Governing Legal Authority | Action Required | Statutory Deadline |
|---|---|---|---|
| **Non-Consensual Intimate Imagery (NCII)** | Rule 3(2)(b), IT Rules 2021 | Immediate removal / disable access | **Within 2 hours** of notice |
| **Court Orders & Government Takedown Directions** | Rule 3(1)(d), IT Rules 2021 & Section 69A IT Act | Technical deletion of identified blob or mailbox | **Within 36 hours** (emergency orders: **within 3 hours**) |
| **Child Sexual Exploitation (CSAM)** | POCSO Act 2012 & mandatory intermediary reporting | Immediate technical deletion & law enforcement reporting | **Immediate / Priority** |
| **General User Abuse Reports & Copyright Notices** | Rule 3(2)(a), IT Rules 2021 & Indian Copyright Act 1957 | Moderation review, verification, and technical deletion | Acknowledged in **24 hours**, resolved in **15 days** |

---

## 4. Operational Takedown Procedure

Because Ember stores only opaque ciphertext on its servers, takedowns operate on cryptographic and network identifiers:

1. **Receipt of Notice:** The Grievance Officer receives a notice containing either:
   * A full drop link `https://<host>/d/<blobId>#<key>` or
   * A specific 128-bit hex `blobId` or mailbox identifier.
2. **Identification & Extraction:** The technical operator extracts the resource identifier (`blobId` or mailbox ID).
3. **Admin CLI Execution:** The operator executes `ember-admin takedown`:
   ```bash
   ember-admin takedown --id <blob-or-mailbox-id> --ref <ticket-or-order-reference>
   ```
4. **Immediate Deletion & Tombstone:**
   * The ciphertext files (`part_0.bin`, etc.) and metadata are permanently purged from server storage.
   * A tombstone is recorded so subsequent access attempts receive an immediate `410 Gone`.
   * If a live thread mailbox is targeted, queued messages are discarded and the mailbox is destroyed.
5. **Append-Only Audit Logging:** The action is permanently recorded in `.takedowns.jsonl` with timestamp, operator ID, and formal request reference.
6. **Notice of Action:** The Grievance Officer transmits formal written confirmation of technical compliance to the requesting authority or complainant.

---

## 5. Transparency and Review

* **Audit Retention:** The takedown audit log is maintained for three (3) years to fulfill statutory auditing requirements.
* **Transparency Reporting:** Ember publishes periodic transparency statistics disclosing aggregate counts of takedown requests, legal orders, and user abuse reports processed. Transparency reports never contain personal identifiers or private data.
