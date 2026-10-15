# Backup and transfer manifests — P0 draft

Derived from SPEC §§4.9, 9.7, 15.3–15.4 and DELIVERY P0/P1. These are portable
canonical metadata records, not SQLite copies, durable backups or a network protocol.
The standard envelope and deterministic CBOR rules apply. Verification is read-only.

## Shared contents record

All seven integer-keyed fields are required:

| Key | Value |
| --- | --- |
| 0 | Completeness: standalone=1, incremental=2 |
| 1 | Selected typed roots: `[native-ref, type-code]`, strictly digest-sorted |
| 2 | Packaged inventory entries, strictly digest-sorted |
| 3 | Exact prerequisite inventory entries, strictly digest-sorted |
| 4 | Required feature byte labels, nonempty, strictly byte-sorted |
| 5 | Optional feature byte labels, nonempty, strictly byte-sorted |
| 6 | External scope=1: native closure only; no recursive Gitlink/LFS payload promise |

Inventory entry: `[native-ref, type-code, framed-byte-length, interpretation]`.
Type is a nonzero u16; length is 23..67108887 bytes (envelope plus structured limit).
Interpretation is required/interpreted=1 or selected opaque metadata=2. The native
ID is the BLAKE3 checksum of the complete framed object, not a second independent
checksum field. Frame version, type and length must match. Supported interpreted
objects must pass their schema; unknown types may only be opaque inventory data.
Opaque objects cannot satisfy typed roots or required interpreted closure. Their
bytes are still read and hash-checked, never silently dropped. Any signature objects
are selected as ordinary attestation roots/inventory; there is no self-signature
cycle. A detached signature over the manifest is transported separately.

Inventory and prerequisites are disjoint. Every root must appear with matching type
and interpreted status in one of the two lists. Standalone has no prerequisites;
incremental has at least one and remains explicitly incremental even after they
are supplied. Prerequisites name exact content IDs, types and sizes, not mutable
paths, timestamps or assumed peer state. A packaged missing object cannot fall back
to a prerequisite provider. Required and optional feature lists are disjoint. The
verifier requires callers to supply supported feature labels and refuses unknown
required features before reading object payloads. Format/hash support is implicit
in the native v1 envelope; negotiation and capability registry are still P0 work.

Every interpreted inventory/prerequisite object is verified as a closure root;
selected metadata therefore keeps its required source closure while archival edges
retain their existing semantics. Every closure read must be explicitly inventoried.
A provider must supply immutable, allocation-bounded data under a retention lease.
Object/edge/byte budgets cover the inventory and closure verification, including
rereads. No bytes are installed and no refs or committed generations are changed.

## Transfer manifest (type 19)

Required fields: 0 repository ID bytes16, 1 shared contents record. At least one
selected root is required. This is a content transfer description, not a resumable
session token, authorization grant, durable receipt or visible-ref update.

## Backup manifest (type 16)

Required fields: 0 repository ID bytes16, 1 committed generation uint,
2 current view native reference (type 6), 4 shared contents record.
Optional field 3 current operation native reference (type 7). Its absence represents
an initial repository at generation zero; later generations require an operation.
The current view and optional operation must be inventoried as interpreted objects.
Selected extra roots include promised undo/pre-state views and retention metadata;
the current view already carries the portable refs, tombstones and workspace roots.
Verification checks current operation post-view equals the declared current view.
This records an asserted consistent snapshot; only P1's root transaction and backup
lease can establish that it was actually committed. No newest-timestamp recovery.

`verify_package` returns `StandaloneVerified` or `IncrementalVerified` only after
all object bytes, declared features, required closure, and backup linkage pass.
Neither is a durable-backup acknowledgment, full Git-mapping validation or external
payload completeness claim. The draft [source bundle container](BUNDLE-FORMAT.md)
now supports bounded independent verification, source tar export and portable
[repository recovery](RECOVERY-EXPORT.md). Live database installation, server
authorization/configuration, encryption-key recovery, resumable
transport, durable leases, restore writes, and clean-host qualification remain
subsequent work. A decoded standalone
claim alone must never be displayed as a verified complete backup.
