# Update-refs request — P0 draft

Implements the request/digest portion of SPEC §§4.7 and 9.3. This experimental
wire profile makes encoding choices where the spec gives semantic requirements.
It does not implement a server, compare-and-swap, authorization, durable receipts,
retention, or publication. These are required before executing requests.

## Wire representation

The JSON envelope has exactly `intent` and `request_digest`. All fields in the
intent and each update below are required; null is not accepted. Unknown and
duplicate fields are rejected. JSON input/output is bounded to 1 MiB; constructed
intent JSON is limited to 1 MiB minus 128 bytes to leave room for the envelope.

Intent fields, in digest order:

1. `schema_version`: integer 1.
2. `transaction_id`: 32 lowercase hex digits, identifying this intentional mutation.
3. `repository_id`: 32 lowercase hex digits.
4. `expected_namespace_generation`: canonical decimal u64 string.
5. `operation_id`: 32 lowercase hex digits, as in mutation-result v1.
6. `reason`: nonempty UTF-8, at most 4096 bytes, no control characters.
7. `updates`: 1–1000 entries, strictly sorted by UTF-8 ref name, no repeated names.
8. `required_closure_manifest`: canonical `k1:b3:` object ID.
9. `policy_input_ids`: sorted unique canonical object IDs (may be empty).
10. `client_capabilities`: sorted unique ASCII tokens (may be empty). Each is
    1–128 bytes using lowercase letters, digits, hyphen, dot or underscore.

Each update has `name`, `expected_generation`, `expected_old_targets`, and
`new_targets`. Names use the existing NFC native ref grammar. Generations are
canonical decimal u64 strings, not JSON numbers. Target arrays are sorted unique
canonical object IDs. Empty new targets mean deletion; multiple targets are
explicit alternatives, allowed only under `branches/`. This array representation
extends the spec's singular target notation to preserve existing branch conflicts.

Generation zero with empty old targets means never-created absence at the pinned
namespace generation. Positive generation with empty old targets means a retained
tombstone. Both can be created/recreated, but deleting an already absent/deleted
ref is rejected. Nonempty old targets require positive generation. All updates
share the namespace generation, operation ID and reason. These draft choices bind
the creation precondition and audit intent required by §4.7 into the digest.

The server must compare the actual namespace/ref generations and entire old target
sets, check target/manifest object types and verified closure, policy, ancestry,
permissions, and capability support. Syntax acceptance proves none of those facts.
A counter that cannot advance must fail closed; it must never wrap or reset.

## Canonical digest preimage

`request_digest` is `b3:` plus the 64 lowercase hex digits of unkeyed BLAKE3 over
the following bytes. It excludes only the digest itself (to avoid self-reference).
It includes all intent fields; transport headers and JSON formatting are excluded.
This is a domain-separated request digest, not a native object identifier.

- Begin with ASCII `kai.update-refs.request.v1` followed by one NUL byte.
- Append schema_version as unsigned big-endian 16 bits.
- A string is its UTF-8 byte length as unsigned big-endian 64 bits, then its bytes.
- A string array is its count as unsigned big-endian 64 bits, then each string.
- Append transaction ID, repository ID, namespace generation, operation ID, reason
  as strings in that order (generations and IDs retain their canonical spelling).
- Append updates count as unsigned big-endian 64 bits. For each update append its
  name and expected generation as strings, then old-target and new-target arrays.
- Append closure-manifest ID as a string, then policy-ID and capability arrays.

Arrays must already be sorted; the encoder rejects rather than reorders. Text is
not normalized during encoding. Ref names must already be NFC. Reason text is
preserved exactly. Whitespace, JSON key order and equivalent JSON escapes do not
change the digest; any change to a valid intent field does.

`UpdateRefsIntent::into_request` computes the digest. `UpdateRefsRequest` parses
and verifies it, and `status_query` constructs a lookup bound to that same
transaction, repository and digest. Reusing a transaction ID for different content
must be refused by a future persistent server. A digest is not authentication.

## Verification and demo

`tests/fixtures/update-refs.json` publishes a Unicode, multi-ref replacement and
deletion intent plus exact canonical bytes. The independent Python encoder checks
those bytes; Rust tests check matching encoding, roundtrips, field binding,
creation/tombstones/alternatives, bounds, and rejection of malformed requests.
The Python encoder does not independently validate intents or implement BLAKE3.

Try `cargo run --locked --offline --example update_refs`. It prints the request
and status-query binding, then demonstrates rejection after changing the intent.
No publication occurs. Complete publication receipts and durable execution remain
separate increments.
