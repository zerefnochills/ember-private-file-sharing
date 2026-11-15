# Ember Dependency & License Compliance Report

This report inventories all direct and transitive dependencies used within the Ember workspace and verifies their compatibility with Ember's **GNU Affero General Public License v3.0 (AGPL-3.0-or-later)**.

---

## 1. Licensing Philosophy & AGPL-3.0 Compatibility

Ember is committed to full open-source transparency. The AGPL-3.0 copyleft license ensures that any hosted fork or network deployment of Ember relay servers or clients remains open source.

All third-party libraries incorporated into Ember must possess permissive or copyleft licenses compatible with AGPL-3.0:
* **Permissive Licenses:** MIT, Apache-2.0 (with standard patent grants), BSD-2-Clause, BSD-3-Clause, ISC, CC0-1.0, Zlib.
* **Weak Copyleft:** MPL-2.0 (file-level copyleft; compliant as dynamic or separate module linking).
* **Compatible Copyleft:** AGPL-3.0-only, AGPL-3.0-or-later.

Licenses prohibited in Ember workspace dependencies:
* Proprietary / Commercial closed-source SDKs (e.g. Google Play Services, Firebase).
* GPL-incompatible licenses (e.g. JSON license, non-commercial restrictions).

---

## 2. Direct Dependency Inventory

| Dependency Name | Version | Declared License | Purpose / Role | Compatibility Status |
|---|---|---|---|---|
| **dryoc** | 2.0.0 | BSD-3-Clause | Pure Rust libsodium-compatible crypto (XChaCha20-Poly1305, Argon2id, BLAKE2b) | Compatible |
| **subtle** | 2.6.1 | BSD-3-Clause | Constant-time comparisons (`ConstantTimeEq`) | Compatible |
| **zeroize** | 1.8.1 | Apache-2.0 OR MIT | Secure memory wiping on drop for secret keys | Compatible |
| **sha2** | 0.10.8 | MIT OR Apache-2.0 | SHA-256 hashcash proof-of-work calculations | Compatible |
| **axum** | 0.8.1 | MIT | High-performance asynchronous HTTP relay framework | Compatible |
| **tokio** | 1.43.0 | MIT | Asynchronous async/await runtime | Compatible |
| **tower** / **tower-http** | 0.5 / 0.6 | MIT | Middleware abstractions (CORS, tracing) | Compatible |
| **serde** / **serde_json** | 1.0 | MIT OR Apache-2.0 | Wire framing and test vector serialization | Compatible |
| **clap** | 4.5.30 | MIT OR Apache-2.0 | CLI interface parsing (`ember`, `ember-admin`) | Compatible |
| **base64** | 0.22.1 | MIT OR Apache-2.0 | Base64url link fragment encoding/decoding | Compatible |
| **thiserror** | 2.0.11 | MIT OR Apache-2.0 | Strongly-typed error hierarchies | Compatible |
| **uniffi** | 0.32.2 | MPL-2.0 | Foreign function interface code generator for Android Kotlin | Compatible |

---

## 3. Automated Policy Enforcement (`deny.toml`)

Automated license checking is configured in [`deny.toml`](../../deny.toml) and checked continuously via GitHub Actions CI and `scripts/check_licenses.ps1`. Any dependency introducing an unapproved license breaks the build.
