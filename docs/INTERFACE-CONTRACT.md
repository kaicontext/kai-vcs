# Capability and mutation-result contracts — P0 draft

Derived from SPEC §§9.1, 9.4, 13.2. JSON schema version 1 is a proposed API
contract, separate from content-addressed object CBOR. No CLI mutation commands,
network sessions, durable receipts or authority are implemented here. The
`interface` module provides bounded parsing, validation, encoding and negotiation.
Structural schemas: `capabilities.schema.json` and `mutation-result.schema.json`.
Cross-field and canonical ordering rules below require semantic validation as well.
The shared accepted/rejected corpus is `tests/fixtures/interface.json`; the separate
stdlib-only checker is `tools/interface-reference/check.py`.
No current runtime capabilities or durability guarantees are advertised by default.
The [native object-frame profile](TRANSFER-FRAMING.md) consumes negotiated limits
for protocol 1, `native-v1-blake3`, and compression `none`. Object limits include
the canonical envelope; frame limits include the additional 95-byte wire header.
Session authentication and request dispatch remain unimplemented.

JSON records are at most 1 MiB, use UTF-8, reject unknown/duplicate fields, and
contain exactly one value (trailing whitespace is allowed). Optional fields may be
omitted or null; encoders omit absent values. IDs use canonical native IDs or
32 lowercase hex digits for request/operation UUID bytes. Generations are canonical
unsigned decimal strings through u64::MAX, avoiding JavaScript precision loss.

## Capability offer

Required fields: schema_version=1; protocol_versions (sorted unique positive u16);
object_formats (sorted unique strings, e.g. `native-v1-blake3`); supported_features
and required_features (sorted unique labels); max_object_bytes and max_frame_bytes
(positive u32, frame >= object); compression (sorted unique nonempty labels);
signature_formats (sorted unique labels); ref_transactions, partial, shallow
(booleans); durability (`none`, `single-node-durable`, `replicated-durable`);
optional replication_policy (native policy object ID).
Protocol/object-format/compression lists must be nonempty. Feature/codec labels
use lowercase ASCII letters, digits, `-`, `_`, `.`, `/`; unknown labels are preserved.
An offer's required features must be supported locally. Replicated durability
requires a replication policy ID; other classes omit it. This is an asserted
capability, not evidence that storage meets the class.

Negotiation requires explicit transfer selection: complete, partial, or shallow.
Both peers must support requested partial/shallow behavior. A complete request
always stays complete even when peers support partial modes. Negotiation rejects
unsupported required features, protocol/format/compression mismatch, requested
ref transactions without mutual support, and requested signature formats absent
from either peer. It picks the highest common protocol, first lexically common
format/compression, intersects features/signature formats, and takes minimum size
limits. A requested minimum durability is checked against the receiver; a
replicated request must name the exact expected receiver replication-policy ID.
Unknown mandatory features never become an implicit downgrade. Caller-provided
offers and request policy must come from authenticated sessions/trusted local config.

## Mutation result

Required fields: schema_version=1, request_id, outcome, exit_code, refs, recovery,
external_receipts, interrupted. Optional operation_id and generation.
Ref entries have name (native NFC grammar), positive generation decimal string,
and optional target native ID (absent/null denotes deletion). Entries are strictly
name-sorted. Recovery entries are nonempty human-readable strings.
Receipts have system, receipt_id (nonempty strings), and state
(`complete`, `pending`, `failed`, `unknown`); (system,receipt_id) pairs are unique.
Receipts are locators/claims, not authenticated proofs.

| Outcome | Exit | Required constraints |
| --- | --- | --- |
| not_started | 1,2,4,5,6,7,8 | No operation/generation/refs/receipts |
| prepared | 1 | Operation and recovery; no resulting generation/refs/receipts |
| committed | 0 | Operation and generation; receipts all complete |
| committed_materialization_pending | 9 | Operation, generation and recovery |
| conflicted | 3 | Operation and recovery; generation optional |
| rejected | 1,2,4,5,6,7,8 | No resulting generation/refs/receipts |
| indeterminate | 10 | Operation and recovery; no asserted resulting generation/refs |

A ref result always requires a resulting generation. Conflicted receipts, if any,
must be complete; failed external effects use the pending or indeterminate outcome.
Codes: 1 operational failure, 2 usage/unsupported, 3 conflict, 4 dirty/collision,
5 stale lease/retry, 6 incomplete, 7 policy/auth, 8 integrity, 9 committed with
pending materialization/external repair, 10 indeterminate. A prepared candidate
is never success. Interruption preserves the known outcome and its code, including
0 or 9 after commit; it never invents a rollback or blindly changes success to 130.
Indeterminate retries must query operation/request status before another mutation.
Generation/receipt truth, idempotency storage and transaction status remain P1/P5.

## Negotiation exchange (version 1)

SPEC §§9.1–9.2 negotiation now has transport-neutral JSON request and reply
records. Both use the existing 1 MiB bound, duplicate/unknown-field rejection,
UUID spelling, optional/null rules and label ordering. These are typed payloads;
HTTP/SSH dispatch, authenticated hello and session lifecycle remain future work.

A request contains exactly `schema_version: 1`, `request_id`, `repository_id`,
and `policy`. Both IDs are 32 lowercase hexadecimal digits. `policy` requires
`selection` (`complete`, `partial`, `shallow`), `require_ref_transactions`
(boolean), `required_signatures` (ordered unique labels), and `minimum_durability`;
`replication_policy` is optional. Replicated minimum durability requires its exact
native policy ID, and other minima forbid it. No transfer selection defaults.

A successful reply contains exactly `schema_version: 1`, echoed `request_id`,
echoed `repository_id`, and `negotiated`. The latter contains every field of the
negotiation result described above: protocol_version, object_format, compression,
features, signature_formats, max_object_bytes, max_frame_bytes, selection,
ref_transactions, receiver_durability, and optional replication_policy.

The receiver constructs a reply by applying the published negotiation rules.
The client accepts a reply only if it equals the result recomputed from the
original request and both trusted capability offers, including every ID and
selected limit. Matching IDs alone is insufficient. Changed capabilities require
a fresh exchange; a previous result cannot authorize a downgrade or different
repository. A rejected negotiation produces an error, never a success reply.
The request ID is a correlation identifier, not a replay-proof nonce or durable
transaction ID. Authentication, offer freshness and repository authorization
belong to the session adapter. No ref or object mutation occurs here.

Structural definitions are in `negotiation.schema.json`; semantic reply checking
requires the request and offers. The shared corpus `tests/fixtures/negotiation.json`
is checked by Rust and the independent Python negotiation implementation.

Try the local request/reply and frame-limit handoff with
`cargo run --locked --offline --example negotiation`. It performs no network I/O.
