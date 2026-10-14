# Independent repository recovery export — P0 draft

SPEC §§4.7–4.9, 9.7, 15.3 and T012 require recovery without the original database
or main application. The independent reader now accepts either type-16 backup or
type-19 transfer manifests in the existing bundle container. It interprets all 19
registered types, preserves selected opaque objects, and verifies typed required
closure. Incremental bundles and unknown required features are still refused.

`kai-bundle-reader BUNDLE --index` prints a descriptive index. It includes backup
repository identity, generation/current-view/current-operation, every included
view's exact refs (including deletion tombstones and divergent alternatives),
draft heads, workspace base/checkpoint pointers, retained roots, checkpoints,
revision trees/conflict bindings, and operation recovery pointers. Bytes are hex
where needed; IDs and counters are exact. This index is derived, not authority.

`kai-bundle-reader BUNDLE --recover OUTPUT.tar` creates a portable recovery archive
with only fixed or content-addressed safe paths:

- `manifest.object`: original framed backup/transfer manifest.
- `objects/<digest>.object`: every inventoried framed object, including opaque data.
- `repository.kai-bundle`: byte-identical standalone bundle, usable by this reader.
- `INDEX.txt`: the descriptive index.

Extract into a fresh directory using a capable tar tool. The recovered bundle can
be verified and used to export resolved revision/checkpoint snapshots with this
reader alone. No SQLite replay, main Kai binary, Git process, hosted service or
network is needed. This restores portable immutable repository state, not a live
P1 database, editable workspace, authorization configuration or secret store.
No generation is installed or chosen by timestamp, and no operation is replayed.

All required edges and cross-object bindings are verified: view draft/change and
workspace/checkpoint identities; checkpoint/barrier workspace and tree; evidence
revision/tree/conflict state; backup current-operation post-view; Git owner kinds;
conflict path completeness and absence from ordinary source trees; large-file
assembly. Retained-metadata edges are not silently promoted to required closure.
Missing expired archival pointers remain explicit in the index and remain in the
original object bytes. Every packaged object is retained, even when unreachable
from current refs. Nested package records contribute typed roots; their inventory
is a declaration, not an assertion that all historical package bytes are present.

Unresolved revisions and checkpoints verify and recover with their base/side
alternatives intact. `--archive ID OUTPUT.tar` accepts any inventoried interpreted
tree/revision/tag/checkpoint, but refuses a conflicted revision/checkpoint (working
or staged conflicts). It exports checkpoint working trees; staged tree IDs remain
available in the index for explicit tree export. Exporting a raw tree is exactly
that tree, not a claim that an enclosing conflicted revision is resolved.

Limits from BUNDLE-FORMAT.md still apply. Recovery output is streamed, including
reconstruction of the bundle; index generation has a 64 MiB limit. Existing output
paths are refused. Verification/preflight precedes creation. Write failures may
leave a partial new archive; no durable acknowledgment is claimed. Embedded
profiles, signature trust, policy enforcement and CRDT protocol claims remain
separate from opaque metadata preservation and native closure verification.

Clean-directory and standalone-binary drills provide local evidence. A genuinely
clean-host/platform qualification and independent review remain P0 acceptance work.
