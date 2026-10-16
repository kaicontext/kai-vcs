# Publication receipt — P0 draft

Implements the committed receipt shape from SPEC §§9.3–9.4 against the
[update-refs request](UPDATE-REFS.md). This is an experimental transport-neutral
contract, not a durable transaction service or evidence that disk flushes occurred.
A receipt represents a successful, committed, all-or-nothing publication only.
Prepared, rejected, unknown and indeterminate outcomes use the existing outcome
contracts; they must never be represented by this committed receipt type.

## Fields and binding

All fields are required except `durability.replication_policy`:

- `schema_version`: integer 1.
- `transaction_id`, `repository_id`, `request_digest`, `operation_id`: exact
  copies from the original validated request. The request digest is recomputed
  before trusting that binding, even for requests constructed directly in Rust.
- `repository_generation`: canonical decimal u64 string, exactly one greater
  than the request's expected namespace generation.
- `policy_outcome_id`: canonical native object ID for the recorded policy outcome.
- `durability`: object containing `class` and optional `replication_policy`.
- `refs`: one entry for each requested update, in exactly the request's order.
  Each has `name`, `old_generation`, `new_generation`, `old_targets`, `new_targets`.

Every name, old generation, old target set and new target set must match the
corresponding request entry exactly. New ref generation must equal old generation
plus one, including deletion and recreation. Overflow is an error, never wraparound.
Empty target arrays preserve absence/tombstones; multiple branch alternatives
remain explicit. Receipts cannot omit refs, add refs or choose one alternative.

The exact +1 rules are this draft profile's choice for one atomic batch. The spec
requires advancing generations; this profile is stricter and makes one batch's
transition unambiguous. A future backend must implement this rule or version the
contract. Namespace generation changes once for the batch, not once per ref.

## Durability expectations

`PublicationDurability` is supplied independently by the caller from its trusted,
authenticated session and compared exactly with the receipt's claim. Do not derive
that expected value from the receipt itself. Both values are validated:

| Class | Replication policy |
| --- | --- |
| `single-node-durable` | Absent or null; omitted on encoding |
| `replicated-durable` | Required canonical native object ID |
| `none` | Forbidden for a committed publication receipt |

A different class or policy is rejected, including a claimed upgrade not negotiated
for the session. These are declarative claims. Verifying the policy object, its
failure domains, authorization, synchronous barriers and actual durable state is
backend/session work. A replicated claim does not itself prove two independent
hosts have acknowledged the same required closure and metadata.

## Parsing, limits and recovery

JSON is bounded to 1 MiB, with unknown/duplicate fields and trailing data rejected.
Generations are decimal strings, avoiding JSON numeric precision loss. Target and
identity grammar comes from the validated request; policy IDs are checked directly.
Constructed receipts are bounded on validation/encoding too. A future publisher
must ensure the complete receipt fits before committing; a receipt must never be
truncated to fit the wire limit. Struct deserialization alone is not validation:
use `from_json` or `validate` with the original request and session durability.

After a lost response, the server must retain and return the same committed receipt
for the original transaction/digest. This module defines what that receipt must
contain. The [v2 status envelope](RECEIPT-STATUS.md) carries and checks the receipt
without changing outcome-only transaction-status v1. Backend retrieval/retention,
authenticated dispatch and persistent idempotency are still separate work, including
the minimum 90-day retention rule.
No generic MutationResult conversion is offered: its singular ref target cannot
represent every branch alternative without losing information.

## Checks and demo

Rust and an independent Python checker share 47 accepted/rejected vectors covering
binding, complete batches, exact states, generation spelling, durability policy,
missing/unknown/duplicate fields, trailing data and invalid Unicode. The checker
uses the fixed valid update-refs fixture; request hashing/validation is tested
separately. Additional Rust tests cover creation, recreation, branch alternatives,
last representable generations, overflow, stale request digests and size limits.

Run `cargo run --locked --offline --example publication_receipt` for a synthetic
committed receipt and rejection of an incomplete batch. It changes no refs and
makes no durability claim about local storage.
