# Repository views — P0 draft

Derived from SPEC §§4.7, 4.9 and 7.1. This experimental registry precedes the
codec; it is not a format freeze. A view is type 6 using the common envelope.

## Field registry

All six integer-keyed fields are required, even when their arrays are empty.

| Key | Value |
| --- | --- |
| 0 | Namespace generation, unsigned u64; zero is the initial generation |
| 1 | Ref entries, sorted by unsigned UTF-8 name bytes |
| 2 | Draft entries, sorted by 16-byte change ID |
| 3 | Workspace entries, sorted by 16-byte workspace ID |
| 4 | Explicit retained roots, sorted by (type code, digest) |
| 5 | Reserved encoding discriminator, unsigned 0 for inline maps |

Inline maps are arrays with strictly increasing keys; duplicate keys are invalid.
There are no content-addressed submaps in this increment. Discriminator values
other than 0 are rejected. Sharded representations and their identity rules must
be registered and reviewed before format freeze; the structured-object size
limit applies to this representation.

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
