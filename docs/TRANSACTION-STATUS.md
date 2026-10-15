# Transaction-status lookup — P0 draft

Derived from SPEC §§9.3 and 13.2, especially acceptance T022/T023. These JSON
payloads let a client query a previous mutation without repeating it. They are
bounded to 1 MiB, require schema version 1, and reject unknown/duplicate fields.
Optional fields may be omitted or null and are omitted when encoding absent.
This defines an outcome lookup, not durable storage or a complete publication
receipt (old/new refs, policy outcome and durability receipt remain separate work).

## Request

Required: `schema_version`, `request_id`, `repository_id`, `transaction_id`,
`request_digest`. The three IDs are 32 lowercase hexadecimal digits. `request_id`
identifies the lookup; `transaction_id` identifies the original mutation. The
original mutation's result `request_id` must equal this transaction ID.

`request_digest` has spelling `b3:` followed by 64 lowercase hex digits. It is
an algorithm-qualified digest of the original canonical mutation request, not an
object ID or a digest of this status query. This module checks spelling and
binding only; the draft [update-refs contract](UPDATE-REFS.md) defines canonicalization and
digest computation. Durable mutation execution remains unimplemented.

## Reply

Required: `schema_version`, echoed `request_id`, `repository_id`, `transaction_id`,
`request_digest`, and `status`. Optional: `result`, `unknown_reason`.
All identity/digest fields must match the lookup exactly.

| Status | Required payload | Forbidden payload |
| --- | --- | --- |
| `known` | `result`: validated mutation-result v1 for the original transaction | `unknown_reason` |
| `unknown` | `unknown_reason`: `missing` or `expired` | `result` |
| `digest_mismatch` | none | `result`, `unknown_reason` |

A known result follows all existing outcome/exit-code/effect rules. Prepared or
indeterminate results do not become committed simply because they are stored.
Unknown means the service cannot establish an outcome, including after receipt
expiry; it does not mean the mutation never ran. Digest mismatch means the
transaction ID belongs to different content; it never permits re-execution and
does not disclose that other request's result or digest. The echoed digest is
always the digest queried by the client.

The service must authorize repository/principal access before looking up any
transaction. IDs, digests, expiry reasons and returned results are claims from
that authenticated service, not cryptographic proof. Transaction namespace,
principal binding and persisted digest comparison remain backend responsibilities.
The contract cannot verify that a server's claimed outcome actually happened.

## Client guidance

The checked helper derives one action and never returns permission to rerun:

| State/outcome | Guidance |
| --- | --- |
| committed | Complete: consume the existing result |
| committed_materialization_pending | Recover committed work; do not repeat the mutation |
| conflicted | Resolve the reported conflict |
| prepared | Query status again; do not repeat the mutation |
| indeterminate, unknown/missing, unknown/expired | Reconcile exact authoritative refs and recovery instructions |
| rejected, not_started | Require a new intentional request before another attempt |
| digest_mismatch | Resolve ID/content mismatch; never reuse that ID for different content |

A query ID does not replace the original transaction ID. A backend must retain
idempotency receipts at least 90 days under SPEC §9.3, and must not reinterpret
an expired retry as a fresh mutation. This module implements neither retention
nor a clock, so it cannot certify that guarantee. No network dispatch or mutation
execution is performed by these types.

`transaction-status.schema.json` describes structure, with cross-field rules here.
Rust and an independent Python checker share accepted/rejected examples. Run the
local lost-response/expired-result example with
`cargo run --locked --offline --example transaction_status`.
