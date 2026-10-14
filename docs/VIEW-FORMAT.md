# Repository views — P0 draft

Derived from SPEC §§4.7, 4.9 and 7.1. This experimental registry precedes the
codec; it is not a format freeze. A view is type 6 using the common envelope.

## Inline field registry (discriminator 0)

All six integer-keyed fields are required, even when their arrays are empty.

| Key | Value |
| --- | --- |
| 0 | Namespace generation, unsigned u64; zero is the initial generation |
| 1 | Ref entries, sorted by unsigned UTF-8 name bytes |
| 2 | Draft entries, sorted by 16-byte change ID |
| 3 | Workspace entries, sorted by 16-byte workspace ID |
| 4 | Explicit retained roots, sorted by (type code, digest) |
| 5 | Encoding discriminator, unsigned 0 for inline maps |

Inline maps are arrays with strictly increasing keys; duplicate keys are invalid.
The structured-object size limit applies to every inline view. Existing inline
encodings and IDs are unchanged.

A ref entry is `[name_bytes, generation, state]`. Generations are positive u64.
State is `[0]` for a deletion tombstone, `[1, native_ref]` for one target, or
`[2, [native_ref, native_ref, ...]]` for explicit draft conflict alternatives.
Alternatives contain at least two distinct references sorted by digest, never
ordered by timestamps. Only local branches permit alternatives. Ref names are
UTF-8 NFC, case-sensitive, with no control characters, backslashes, empty, `.`
or `..` components. Valid namespaces are `branches/<name>`, `tags/<name>`,
`remotes/<remote-id>/branches/<name>`, and `shelves/<name>`; suffixes may contain
multiple components. Unknown/internal namespaces are rejected. Input is validated,
never silently normalized. Draft/workspace roots use the typed tables below.
Branches and remote branches target revisions (3), tags target revisions (3) or
annotated tags (15), shelves target checkpoints (8). Tombstones have no edges.
Generation advancement and expected-old comparisons belong to root transactions;
a single immutable view cannot prove a transition or commit status.

A draft entry is `[change_id_bytes16, [native_ref, ...]]`, with one or more
strictly digest-sorted revision heads. Closure validation verifies every head's
change ID. Divergent heads are all retained; no winner is inferred.

A workspace entry is `{0: workspace_bytes16, ?1: base_revision, ?2: checkpoint}`.
Both optional pointers may be absent for an unborn workspace. A checkpoint
includes the selected/staged tree. Closure validation checks its workspace ID;
the workspace's current base may differ from the checkpoint's captured base.

A retained-root entry is `[type_code, native_ref]`, where type is 1–19. These are
explicit pins selected into this view, not automatic traversal of archival edges.
Their edges, like live refs/drafts/workspaces, are required from the view: the
selected objects and their required source closure must exist. Their outgoing
retained-metadata edges remain subject to separate selection (e.g. an operation
pin does not recursively pin all predecessor operations). A view cannot promise
a pin to a currently unsupported object type: closure verification fails until
that codec is implemented. Expiry is an explicit new view/root transaction, never
an implicit decoder clock decision. Durable leases and retention policy remain P1.

The current view is a hard repository root. Operation pre/post view links remain
retained-metadata edges, so an expired historical view does not invalidate a
retained audit header. Import/transfer must choose their retained-root selection
explicitly; private workspace state is not automatically public.

## Partitioned field registry (discriminator 1)

A partitioned view is also type 6, with exactly three required integer keys:

| Key | Value |
| --- | --- |
| 0 | Namespace generation, unsigned u64 |
| 1 | Array of `[bucket, native_ref]` pairs, strictly increasing by bucket |
| 5 | Encoding discriminator, unsigned 1 |

Buckets are integers 0–255. There are at most 256 pairs. Empty buckets are
omitted; an empty partitioned view has an empty array. Keys 2–4 are forbidden
on this representation, and other discriminators are rejected.

For each entry, the bucket is the first byte of BLAKE3 over the concatenation of
ASCII `KAI-VIEW-BUCKET`, one zero byte, one section byte, and the entry key:

| Section byte | Entry key bytes |
| --- | --- |
| 1 | Ref name, its validated NFC UTF-8 bytes |
| 2 | Change ID, exactly 16 bytes |
| 3 | Workspace ID, exactly 16 bytes |
| 4 | Retained type, unsigned 16-bit big-endian, followed by its 32-byte native digest |

Each bucket targets an **inline type-6 view** with namespace generation zero,
at least one entry, and every entry hashing to that bucket. The inline ordering,
ref generations, target types, and cross-object draft/workspace bindings apply
unchanged. Submaps cannot themselves be partitioned. A submap is an ordinary
inline view; its role is imposed by the incoming bucket edge, so no new object
type or context-dependent object ID is introduced.

Root-to-submap edges are required closure. Both verifiers reject missing or
wrong-type objects, nested roots, nonzero submap generations, empty submaps,
and misplaced entries. Strict bucket ordering and deterministic entry assignment
prevent duplicate keys across submaps. Hashes are checked before interpreting
records. Object, byte and edge budgets include the submaps; each inline object's
bucket classification is computed once even if several roots share it.

The logical view is the union of the submap tables with the root's namespace
generation. Inline and partitioned encodings of equal logical state intentionally
have different IDs. Selecting a representation is explicit; encoders do not switch
at an implicit size threshold. Partitioning the same state always gives the same
root and submap bytes. Updating one entry changes only its bucket's identity and
the root identity; unchanged submaps remain reusable. Namespace-generation-only
changes reuse every submap. A bucket exceeding the 64 MiB structured limit is
rejected; recursive partitioning or an alternative split algorithm would require
a new representation discriminator. This is a bounded P0 layout, not unlimited
scalability or a durable incremental update implementation.

`View::partition` prepares the root and submaps from a validated, sorted view.
The standalone recovery index links each root bucket to its submap object; the
submap's object section lists its refs, drafts, workspaces and pins. Recovery
exports all exact canonical objects and the byte-identical bundle.

## Try a partitioned backup

From the repository, choose an unused output filename:

```sh
cargo run --locked --offline --example repository_bundle -- /tmp/kai-submaps.kai-bundle --submaps
cargo run --locked --offline -p kai-object-reference --bin kai-bundle-reader -- /tmp/kai-submaps.kai-bundle --index
cargo run --locked --offline -p kai-object-reference --bin kai-bundle-reader -- /tmp/kai-submaps.kai-bundle --recover /tmp/kai-submaps.tar
```

The fixture includes branch alternatives, a tombstone, draft heads, a workspace
checkpoint and a retained historical view. The clean-host qualification runs
this representation with only the copied independent reader and bundle.
This remains an experimental format pending independent review before freeze.
