# Evidence input/environment profiles — P0 draft

SPEC §11.2 requires exact source, dependency/submodule, toolchain and environment
identity. These profiles fill the previously opaque type-9 manifest slots. They
are canonical UTF-8 JSON bytes inside ordinary type-1 blobs, at most 1 MiB each.
The format labels are `kai/evidence-inputs/v1` and `kai/evidence-environment/v1`.
Unknown labels fail context verification; archival object decoding still preserves
unknown formats. Large-blob profiles are not supported by this bounded v1 profile.

Canonical JSON uses the field declaration order listed below, compact separators,
UTF-8 text and the Rust serializer's minimal JSON escaping. Optional absent fields
are omitted, never null. Parsing re-encodes and requires exact byte equality; no
alternate whitespace/key-order/escaping can give the same logical profile two IDs.
Unknown/duplicate fields, noncanonical bytes and out-of-range versions are rejected.
This profile is experimental, not a general-purpose JSON canonicalization standard.

## Inputs: field order

`schema_version` (1), `revision` (native ID), `tree` (native ID), optional
`dirty_overlay_digest` (`b3:` plus 64 lowercase hex), `dependencies`, `submodules`,
optional `review`.
Dependencies are `{name, digest}` records sorted by nonempty name, with raw-content
BLAKE3 digests (`b3:<hex>`); include lockfiles and all declared external build inputs.
Submodules are `{path_hex, algorithm, object_id}` records strictly ordered by decoded
raw path bytes. Paths are lowercase hex of canonical source paths, preserving
non-UTF-8 names. Algorithm is `sha1` or `sha256`; IDs have exactly 40/64 lowercase
hex digits. The list must equal every Gitlink in the captured tree, including nested
directories. External submodule contents are not fetched or claimed available.
Review is `{identifier, target_revision, policy, ?base_revision}`, all IDs native;
identifier is nonempty UTF-8. It is required for review evidence and absent for
execution evidence; identifier must equal the evidence's single review target.
Review target/base/policy IDs are identity claims, not implicit retention pins or
proof those objects are available/trusted; acceptance must explicitly select and
verify them. An overlay digest records declared overlay identity, not proof the
runner used it.

## Environment: field order

`schema_version` (1), `platform` (nonempty identity/version), optional
`container_digest` (`sha256:<64 lowercase hex>`), `toolchains`, `variables`,
`limitations`.
Toolchains are `{name, version}` records, nonempty strings, strictly name-sorted.
Variables are `{name, kind, value}` records, strictly name-sorted; name must be
nonempty and contain neither NUL nor `=`. Kind is `public` or
`opaque_secret_version`; values are strings, with a nonempty opaque token required
for the latter. The producer must not put secret values or guessed/hashes of secrets
in these records. No automatic environment capture occurs. The codec cannot detect
a secret mislabeled as public. An opaque token is not resolved or authenticated.
Limitations are sorted unique nonempty statements; each must also appear verbatim
in the evidence's limitations, so a non-reproducible context is not hidden.

`verify_evidence_context` validates the full required native evidence closure,
checks the declared profile blob IDs and canonical bytes, binds revision/tree and
review kind/target, and compares submodule entries to all captured-tree Gitlinks.
Reads and path traversal use explicit object, byte and edge limits; readers must
supply immutable bounded objects under a lease. It never reads host environment,
executes commands, fetches external data or grants trust. Missing/wrong profiles,
source disagreement, hidden limitations or incomplete Gitlink lists fail closed.
Changing a manifest produces a different blob/evidence identity; no implicit reuse
through change IDs, matching timestamps or a claimed pass is implemented.
Actual runner receipts, dependency completeness, review/policy trust, result
applicability rules and reproducibility still require acceptance-layer verification.
