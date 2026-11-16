# Ember Transparency Report Template

> **Status: DRAFT — Periodic Transparency Disclosure Framework**

This template defines the statistical reporting metrics published periodically (semi-annually) by [Operator legal name, pending incorporation] in accordance with Rule 4(1)(d) of the Information Technology (Intermediary Guidelines and Digital Media Ethics Code) Rules 2021.

---

## Reporting Period: [e.g. 1 January 2027 – 30 June 2027]

### 1. Summary of Requests and Actions Taken

| Metric | Count | Description |
|---|---|---|
| **Total User Abuse Reports Received** | 0 | Reports submitted via in-app "Report" flow |
| **User Reports Actioned (Deleted)** | 0 | Drops or mailboxes removed following moderation review |
| **Non-Consensual Intimate Imagery (NCII) Requests** | 0 | Urgent removal requests under Rule 3(2)(b) |
| **NCII Turnaround Time** | < 2 hours | Average compliance time from notice to deletion |
| **Government & Court Takedown Orders Received** | 0 | Formal legal directions under Section 69A or court orders |
| **Government Orders Complied With** | 0 | Technical takedowns executed via `ember-admin` |
| **Copyright Infringement Notices Received** | 0 | DMCA / Indian Copyright Act notices |
| **Emergency Assistance Requests** | 0 | Life-safety law enforcement inquiries |

---

### 2. Breakdown by Action Category

```json
{
  "period": "2027-H1",
  "reports": {
    "received": 0,
    "actioned": 0,
    "dismissed_unsubstantiated": 0,
    "average_review_hours": 1.2
  },
  "takedowns": {
    "court_orders": 0,
    "statutory_government_orders": 0,
    "ncii_rule_3_2_b": 0,
    "csam_pocso": 0
  },
  "operational_metrics": {
    "total_ephemeral_drops_relayed": 0,
    "expired_by_time_limit_percent": 0.0,
    "burned_after_view_percent": 0.0
  }
}
```

---

### 3. Disclosures Regarding Personal Data

* **User Identifiers Disclosed:** **0 (Zero)**. Ember does not create, maintain, or hold user account profiles, names, or phone numbers.
* **Encryption Keys Disclosed:** **0 (Zero)**. Ember does not possess user decryption keys.
* **Content Plaintexts Disclosed:** Disclosed solely in circumstances where an in-app reporter explicitly submitted decrypted material in an abuse report, and disclosure was mandated by formal court process.
