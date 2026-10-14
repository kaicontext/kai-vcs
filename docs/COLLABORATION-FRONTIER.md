# Collaboration frontier profile — P0 draft

SPEC §12.3 requires an exact captured operation frontier, including namespace
operations. This proposed embedded profile uses `kai/collaboration-frontier/v1`
and an ordinary blob of at most 1 MiB. It does not freeze a CRDT wire protocol.

Canonical JSON uses the field order below, compact UTF-8, no unknown/duplicate
fields or trailing whitespace. IDs and byte strings use lowercase hex. Counters
use canonical decimal strings through u64 max, avoiding JSON number truncation.

- `schema_version`: 1.
- `workspace_hex`, `epoch_hex`: 16-byte identities matching the barrier.
- `generation`: barrier generation as decimal string.
- `tree`: native materialized tree ID matching the barrier.
- `protocol_hex`: exact nonempty barrier protocol bytes.
- `namespace`: sorted `replica_hex`, `sequence`, `operations_digest` records.
  Every barrier participant occurs exactly once with the same sequence, including
  zero. The `b3:` digest commits to that replica's protocol-defined operation log
  through the frontier, including file creation, renames and deletion/tombstones.
  It is an identity claim, not a native object reference or verified log replay.
- `entries`: all materialized tree entries, including directories and Gitlinks,
  strictly sorted by raw full path. Each has `path_hex`, `kind` (1–5, the native
  tree kind), `target` (native ID or `sha1:`/`sha256:` Git digest), and `frontier`.
  A frontier contains sorted `replica_hex`, `sequence`, `operations_digest`
  records for protocol-defined file/document operations. These may be sparse or
  empty for untouched entries; each replica must participate in the barrier and
  its sequence must not exceed the namespace/global frontier. Namespace history
  retains deleted paths even though they have no materialized entry.

All paths follow native raw path rules; no Unicode conversion or filesystem access
occurs. Verification checks the barrier's required closure, exact context, full
recursive tree inventory, and frontier bounds. Shared subtrees are expanded at
all paths. Read count, total bytes, entry expansion, and cumulative path bytes are
bounded, including rereads. Providers must supply immutable allocation-bounded
objects under a lease, as for the closure API.

The barrier remains authoritative for explicit excluded replicas/reasons. Excluded
replicas cannot occur in any manifest frontier. Unknown profiles fail before reads.
The generic object decoder/closure checker still accepts opaque manifests; callers
must explicitly invoke the profile verifier when interpreting this profile.

No verifier here proves that a log digest is honest, operations converged, writers
paused, epochs closed, or acknowledged operations were drained. Log serialization,
authenticated acknowledgments, replay/rename/delete semantics, retired-replica
rejoin, successor epochs and compaction proofs require protocol implementation and
T099/T100 qualification. This record is not a live collaboration feature or a
production publication gate. Larger manifests and streaming/submaps remain P0 work.
