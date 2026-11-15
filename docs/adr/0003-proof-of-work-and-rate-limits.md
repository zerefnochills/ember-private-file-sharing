# ADR 0003: Proof-of-Work and Rate Limits

- Status: accepted
- Date: 2026-11-15
- Deciders: Antigravity (M4)

## Context
Milestone M4 requires cheap throttling of automated abuse and resource exhaustion without user accounts.
The system must:
1. Issue and verify proof-of-work challenges for blob creation (and future thread creation).
2. Allow runtime adjustment of difficulty under attack.
3. Ensure the solver runs in `ember-core` on both native and WebAssembly (WASM) targets.
4. Enforce coarse, privacy-preserving IP rate limits without persistent tracking.

## Decisions

1. **Proof-of-Work Algorithm:**
   - Standard SHA-256 hashcash: `hash = SHA-256(challenge || nonce_be_bytes)`.
   - Target condition: `hash` must have at least `difficulty` leading zero bits.
   - Nonce is a 64-bit unsigned integer (`u64`) encoded as 8 bytes big-endian.
   - Solver uses standard pure Rust `sha2` crate (no C dependencies, native WASM support).
   - Rationale: SHA-256 benefits from hardware acceleration on modern mobile (ARMv8 SHA-2) and desktop architectures, and executes deterministically across all environments with sub-microsecond verification overhead.

2. **Stateless Signed Challenge Token:**
   - Challenge token wire format: Base64url-encoded string containing:
     - 16 bytes: Unique random Challenge ID (`id`)
     - 8 bytes: Unix timestamp of creation (`created_at`)
     - 8 bytes: Unix timestamp of expiry (`expires_at`, 5-minute TTL)
     - 4 bytes: Required difficulty (`difficulty`, big-endian `u32`)
     - 16 bytes: Keyed BLAKE2b/HMAC tag over the preceding fields using an ephemeral server secret.
   - Total length: 52 raw bytes (70 base64url characters).
   - Issuance endpoint: `GET /v1/pow/challenge` returns `{"challenge": "<str>", "difficulty": <u32>, "expires_at": <u64>}`.
   - Rationale: Issuing challenges is stateless and consumes zero server memory, preventing memory exhaustion from unfulfilled challenge requests.

3. **Anti-Replay Mechanism:**
   - The server maintains an in-memory cache of spent challenge IDs with their expiry timestamps.
   - When a solution is verified:
     1. Token signature and timestamp are verified in constant time.
     2. If `now > expires_at`, reject (`400 Bad Request: challenge_expired`).
     3. If `id` exists in spent cache, reject (`400 Bad Request: pow_replayed`).
     4. If SHA-256 hash lacks required leading zeros, reject (`400 Bad Request: invalid_pow`).
     5. Challenge ID is recorded in the spent cache; expired entries are swept every minute.

4. **HTTP Wire Representation:**
   - Clients supply PoW solutions via HTTP request headers:
     - `X-Pow-Challenge: <challenge_string>`
     - `X-Pow-Nonce: <u64_decimal_or_string>`
   - Alternatively, clients may supply `pow: {"challenge": "...", "nonce": ...}` in JSON request bodies.
   - `POST /v1/blobs` requires valid PoW. Missing or invalid solutions return `400 Bad Request`.

5. **Coarse In-Memory Rate Limiter:**
   - Keyed by client IP (`IpAddr`). IPv6 addresses are masked to `/64` prefix to prevent address-rotation abuse.
   - Sliding window / token bucket per IP stored strictly in memory (default 60 blob creations per 15-minute window).
   - When exceeded, returns HTTP `429 Too Many Requests` with `Retry-After` header.
   - Expired buckets are purged automatically; zero IP addresses or counters are persisted to disk or logs.

## Consequences
- Protects relay resources and storage without requiring user accounts, CAPTCHAs, or third-party identity services.
- Verification is instant on server (<1 µs).
- Solvers in Rust core, CLI, and future WASM web receivers share the exact same verification and solving logic.
