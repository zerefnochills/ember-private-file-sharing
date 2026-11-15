# M8 implementation review

Review date: 2026-11-15  
Scope: read-only review of the committed M8 server, access logging, moderation, and transparency implementation. These findings are not fixes; server route and test files are being edited in the parallel Antigravity work.

## Findings

### [P1] Takedown IDs can escape the storage directory

`POST /v1/admin/takedown` only checks that `target_id` is non-empty before passing it to storage. The filesystem backend constructs paths with `root.join(id)` and `.mailboxes.join(id)`. A caller with the admin bearer token can submit a path traversal target, causing recursive deletion outside the intended blob/mailbox directory. Validate IDs against the protocol's generated-ID format before any storage call, and also make filesystem storage reject invalid IDs as a defense in depth. Add tests for traversal strings on Windows and Unix path forms.

References: `server/src/routes.rs:1000`, `server/src/storage/fs.rs:33`.

### [P1] Takedown reports success after storage failures

The takedown handler discards errors from blob deletion, tombstone creation, and mailbox deletion, then records `action_taken: "deleted"`. This can return success and write an audit entry even though the requested deletion or tombstone failed. Propagate these failures and only record a successful action after the required deletion state is durable; make the response distinguish “not found” from “deleted” if both are allowed by policy.

Reference: `server/src/routes.rs:1018`.

### [P1] Reviewer and takedown roles share one credential

Both `POST /v1/admin/takedown` and `GET /v1/admin/reports` call the same `authenticate_admin` function, backed by `EMBER_ADMIN_TOKEN`. Therefore the takedown operator credential also grants access to report plaintext and link keys. This conflicts with the role separation promised in the privacy and takedown documents. Introduce distinct reviewer and takedown credentials or scoped authorization, and test that each role is denied the other role's endpoint.

References: `server/src/state.rs:26`, `server/src/routes.rs:913`, `docs/privacy-and-logging.md:59`.

### [P2] Production access logs are neither emitted nor retained

The middleware appends entries to an in-memory `Vec`; the server executable does not configure a durable sink or emit those entries through tracing. The vector is also unbounded. As written, there is no production access log retention to match the proposed policy, and a long-running process can accumulate entries indefinitely. Choose a bounded/durable sink consistent with the approved privacy policy, or revise the claims and tests to describe the actual behavior.

References: `server/src/access_log.rs:81`, `server/src/main.rs:30`.

### [P2] Transparency totals disappear as reports expire

Transparency totals are derived from currently stored reports, while the sweeper deletes reports after 30 days. Published totals therefore decrease and are not permanent aggregates as documented. Persist aggregate counters separately from report content, increment them atomically on report submission, and test that report deletion does not reduce public totals.

References: `server/src/storage/fs.rs:489`, `docs/privacy-and-logging.md:19`.

### [P2] Report retention starts at submission, not resolution

The privacy policy says reports are kept 30 days after resolution, but records expire 30 days after creation and the API has no report-resolution operation or `resolved_at` field. Either implement the documented resolution-based retention with a maximum cap, or amend the proposal to state a fixed 30-day-from-submission period before release review.

References: `server/src/routes.rs:955`, `docs/privacy-and-logging.md:18`.

## Test status

The focused access-log and M8 integration tests pass, and the full Rust workspace gate passes. Those results show the current tests do not cover the findings above; they do not clear these findings. The web receiver's test vectors also pass (14/14), and its build succeeds.
