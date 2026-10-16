# Receipt-aware transaction status — P0 draft v2

Implements the receipt-recovery contract needed by SPEC §9.3 and acceptance T022
(lost response) and T023 (expired retry). This adds a client-side envelope for
recovering a full committed publication receipt. It does not implement a service,
persistence, retention, authorization, or the fault-injection acceptance scenarios.

## Explicit versioning

This profile is specifically for `update-refs`. Its request is an object with
exactly `schema_version: 2` and `lookup`, where lookup is the existing version-1
[transaction-status request](TRANSACTION-STATUS.md). The reply has exactly
`schema_version: 2`, `status` (the existing version-1 reply), and optional `receipt`
(the [publication receipt](PUBLICATION-RECEIPTS.md)). The nested records retain
their own version 1. The outer version 2 explicitly opts into receipt recovery;
existing v1 readers are unchanged and reject this envelope rather than silently
ignoring a receipt. A future transport must negotiate/support v2 explicitly, never
silently downgrade this lookup to outcome-only v1.

`ReceiptStatusRequest::new` derives the lookup from a validated original
`UpdateRefsRequest` and a caller-provided query ID. Query validation recomputes the
original publication digest and requires matching repository, transaction and
digest. The reply must match all four lookup binding fields, including query ID.
The original request is local client context, not resent as part of the query.
A server must separately authorize and resolve the lookup in its receipt store;
these APIs are for a client that still possesses its original request.

## Outcome and receipt consistency

| Nested status/outcome | Receipt rule | Client guidance |
| --- | --- | --- |
| known/committed | Full validated receipt required | Complete |
| known/committed_materialization_pending | Full validated receipt required | Recover committed work |
| known/prepared | Receipt forbidden | Query again |
| known/indeterminate | Receipt forbidden | Reconcile |
| known/conflicted | Receipt forbidden | Resolve conflict |
| known/rejected or not_started | Receipt forbidden | Require a new intentional request |
| unknown/missing or expired | Receipt and result forbidden | Reconcile |
| digest_mismatch | Receipt, result and unknown reason forbidden | Resolve ID/content mismatch |

The nested v1 rules still apply, including exit-code/outcome consistency, original
transaction ID, recovery instructions, and result restrictions for unknown and
mismatch responses. A known result's operation ID, when present, must equal the
original intent's operation ID. Only the two committed outcomes may carry a
repository generation in this publication profile. For committed outcomes it
must equal the receipt's repository generation. The receipt is checked against
the entire original request and independently supplied session durability.

In this v2 profile `result.refs` must always be empty. Full ref state comes solely
from the receipt. The older result's singular optional target cannot represent
multiple branch alternatives; projecting it would lose information or create two
conflicting sources of truth. No alternatives are discarded. A result remains a
summary of outcome, operation, generation and recovery instructions.

Absent receipt may be omitted or null; it is omitted on encoding. A committed
reply with a missing, null, malformed, mismatched, incomplete or downgraded receipt
is invalid and cannot produce completion advice. Interruption does not change a
committed outcome into an uncertain or failed publication. Unknown after expiry
never permits automatically rerunning the mutation. No guidance variant grants
permission to repeat the original mutation.

## Bounds and trust

The entire JSON request/reply is bounded to 1 MiB, not each nested payload
separately. Unknown fields, duplicate keys and trailing data fail closed. A server
must reserve envelope space before publication/receipt storage, or explicitly fail
to deliver an oversized receipt; it must not silently truncate or drop the receipt
and report completion. There is no chunked receipt transport in this profile.

`from_json`, `to_json` and `advice` validate the full context. Raw serde decoding
is not validation. `PublicationDurability` must come from the authenticated
session's agreement, not from the received receipt. A coherent receipt is a claim
by the authorized service; no cryptographic proof, actual storage durability,
principal isolation or 90-day retention guarantee is established here.

## Verification

44 shared cases are checked by Rust and an independent Python checker. Query
acceptance is asserted separately to prevent reply mismatch from masking broken
query validation. Cases cover valid outcomes, missing/forbidden receipts, identity
and generation mismatches, unagreed durability, malformed versions/fields,
duplicate keys and trailing data. The Python checker uses a fixed valid original
request; independent request hashing is outside its scope. Rust additionally
checks branch-alternative preservation, full-envelope size, original request
revalidation and advice after a lost committed response or receipt expiry.

Run `cargo run --locked --offline --example receipt_status` for synthetic
lost-response recovery and expired-result guidance. No mutation executes.
