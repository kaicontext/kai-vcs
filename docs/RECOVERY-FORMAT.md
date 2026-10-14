# Recovery object registry — P0 draft

Contract: SPEC.md §§4.5–4.9, 7.5, 8.3, 15.1–15.3. This extends FORMAT.md before
implementing these codecs. It does not freeze v1 or add repository transactions.
Framing, native references, identity maps, shortest CBOR, integer key order,
resource bounds and explicit rejection of unknown fields follow FORMAT.md.

Optional fields are omitted. Explicit null is forbidden. Required empty arrays
are valid only where stated. Every source path is nonempty relative raw bytes,
with valid name components (no empty, dot, dot-dot or NUL components). Path lists
and path-to-reference manifests must already be strictly sorted by raw bytes;
codecs refuse ambiguous order instead of reordering aligned alternatives.

## Conflict (type 5)

| Key | Field | Encoding |
| --- | --- | --- |
| 0 | kind | content=1, binary=2, delete/modify=3, kind/mode=4, rename=5, multiple-bases=6, add/add=7, symlink=8, gitlink=9 |
| 1 | paths | nonempty sorted byte-string array |
| 2 | bases | ordered array of versions; empty only when no base exists |
| 3 | sides | ordered array of at least two versions |
| 4 | algorithm version | nonempty opaque bytes |
| 5 | policy version | nonempty opaque bytes |
| 6 | rename candidates | optional nonempty sorted array `[identity_16_bytes, source_path, target_path]` |
| 7 | resolution provenance | optional nonempty native-reference array; retained metadata |

A version is `{0: label_bytes, 1: states, ?2: source_revision}`. Labels are opaque
nonempty bytes, not identities or authorization. Source revisions are required
closure when supplied. A states array has exactly one value per conflict path,
in the same order: `[0]` is absent, `[1, kind, target]` is present. Present kinds
and target namespace restrictions match tree entries. Native content references
are required, including every base and side. Gitlinks remain external.
Repeated/equal alternatives are retained; no deduplication or parser inference.
Rename candidates sort by `(identity, source, target)`; both paths must be in
the conflict's path set, source and target must differ, and duplicates are invalid.

A revision/checkpoint manifest must bind every path declared by each conflict to
that conflict ID, with no extra bindings. Each unresolved path must be absent
from the ordinary tree. An ordinary file at an ancestor also conflicts with the
manifest and is rejected. A missing ancestor directory is allowed; unresolved
paths do not force invalid empty directories into a canonical tree. This check
is part of closure verification, not mere decoding. Plain source export must
continue to refuse unresolved revisions; no export is added in this increment.

## Checkpoint (type 8)

| Key | Field | Encoding |
| --- | --- | --- |
| 0 | workspace ID | 16 bytes |
| 1 | tree | native tree reference, required |
| 2 | selected/staged tree | native tree reference, required |
| 3 | capture class | observed=1, barrier-consistent=2 |
| 4 | actor/time | identity map |
| 5 | inclusion manifest | sorted array `[path, category]`, untracked=1 / ignored=2; may be empty |
| 6 | base revision | optional native revision reference, required when present |
| 7 | conflicts | optional nonempty path/conflict manifest for the working tree |
| 8 | predecessor | optional checkpoint reference, retained metadata (not hard ancestry) |
| 9 | collaboration barrier | optional type-17 reference, required when present |
| 10 | selected conflicts | optional nonempty manifest for the selected tree |

Separate selected conflicts preserve staging independently of working content.
Paths in the inclusion manifest record an explicit capture choice; they do not
change privacy. Checkpoints and predecessor history remain private by default.
Barrier-consistent capture means cooperating writers or a filesystem snapshot;
it does not necessarily imply live collaboration. A collaboration-barrier ref
requires capture class 2. Type 17 stays unsupported until its schema is defined,
so required-closure verification currently refuses checkpoints referencing it.

## Operation (type 7)

| Key | Field | Encoding |
| --- | --- | --- |
| 0 | operation ID | 16 bytes |
| 1 | preceding operations | ordered unique native references; may be empty |
| 2 | pre-view | native type-6 reference |
| 3 | post-view | native type-6 reference |
| 4 | action | nonempty opaque bytes |
| 5 | actor | identity map |
| 6 | started seconds | signed 64-bit |
| 7 | finished seconds | signed 64-bit |
| 8 | affected workspaces | sorted unique 16-byte IDs; may be empty |
| 9 | transaction intent ID | 16 bytes |
| 10 | external receipts | optional nonempty array of opaque locator byte strings |

Predecessors and pre/post views are retained-metadata references. They are not
hard source ancestry. External receipt locators imply neither availability nor
successful external effects. Wall-clock reversal is representable; it must not
invent ordering or shorten retention. A decoded operation is only a prepared
candidate until `{generation,current_operation,current_view}` commits in the
authoritative database. Current views and unexpired recovery views must be
explicitly pinned by the retention registry. No DB authority is implemented here.

## Annotated tag (type 15)

`{0: target_revision, 1: message_bytes, 2: tagger_identity, ?3: git_envelope}`.
Target revision and optional Git envelope are required closure. Lightweight tags
remain refs. Detached signatures are attestations, never in-place mutations.

## Change event (type 4)

`{0: kind, 1: change_ids, 2: prior_revisions, 3: resulting_revisions,
4: actor_identity, 5: reason_bytes, ?6: operation}`.

Kinds: create=1, rewrite=2, split=3, squash=4, abandon=5, revive=6,
cherry-pick=7. Change IDs are a nonempty sorted unique array of 16-byte logical
labels. Revision arrays preserve meaningful order and reject duplicates. Create
has no prior and at least one result; rewrite/split/squash/cherry-pick have prior
and result; abandon has prior and no result; revive has at least one result.
Fine-grained split/squash topology and change-ID conservation are future history
operation checks against actual revisions, not inferred by this codec.
Prior/results and the optional operation are retained metadata: event labels do
not introduce hard revision→event→revision retention cycles.

## Retention boundary

`Object::references` exposes reference classes. Required-closure verification
traverses only required edges. A caller checking retained metadata must select
and pin those roots explicitly. Verifying an audit operation alone does not
certify that its expired undo views are available. View codecs, retention policy,
and durable pins remain later P0/P1 work; no default policy expiry is implemented.
