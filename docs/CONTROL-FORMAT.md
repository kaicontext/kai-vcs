# Collaboration barriers and policy — P0 draft

Derived from SPEC §§11.4, 12.3 and 14.2. These immutable records describe claims;
decoding never pauses writers, authenticates participants or installs server policy.
Common envelope/CBOR/size limits apply. Field choices await P0 independent review.

## Collaboration barrier (type 17)

Required keys: 0 workspace bytes16; 1 epoch bytes16; 2 barrier generation uint;
3 materialized tree reference (type 2); 4 protocol/version bytes (nonempty);
5 acknowledged frontier `[replica bytes16, sequence uint]` pairs;
6 excluded replicas `[replica bytes16, nonempty reason bytes]` pairs;
7 namespace/frontier manifest reference (blob/large-blob);
8 manifest format/version bytes (nonempty).
Both replica lists are strictly ID-sorted and disjoint. At least one participating
replica is required. Sequence zero is valid. Every participant must be represented
in the declared frontier manifest; that manifest must describe all participating
files and namespace operations (renames/deletes), not just text operations.
The [collaboration frontier profile](COLLABORATION-FRONTIER.md) adds an explicit
verifier for context, participant frontiers and complete materialized inventory.
Generic object decoding still treats manifests as opaque. Authenticated
acknowledgments, writer pause/epoch closure, operation draining, retired-replica/
rejoin handling and successor epochs remain protocol work.
Tree and manifest are required closure. A checkpoint linking this barrier must be
barrier-consistent and have the same workspace and working tree. Staged content
may differ. No timestamp chooses a frontier; later edits cannot mutate these bytes.
A decoded barrier is not proof of actual capture consistency or CRDT convergence.

## Policy (type 18)

Required keys: 0 repository bytes16; 1 positive policy revision uint;
2 protected ref names (UTF-8 NFC byte strings, strictly byte-sorted);
3 required check labels (nonempty bytes, strictly sorted);
4 required reviewer count uint; 5 allowed principal/key labels (nonempty bytes,
strictly sorted); 6 selective-CI mode (disabled=1, unknown-falls-back-to-full=2,
unknown-rejects=3); 7 accepted-history rewrite mode (deny=1, administrative=2);
8 required evaluator feature labels (nonempty bytes, strictly sorted).
Optional key 9 predecessor policy reference (retained metadata, type 18).
Protected refs must be exact `branches/...` or `tags/...` names under the existing
native ref-name grammar. No glob semantics, implicit default rules or automatic
principal trust are introduced. Empty scope/check/principal arrays are representable;
an empty allowed-principal set cannot grant access. Review counts are declarative,
not evidence of approvals.

Every protected-ref acceptance must still require resolved source, complete closure,
exact generation lease, authorized actor and applicable configured checks/reviews.
These mandatory spec gates are not configurable off in this schema. Administrative
rewrite additionally requires a named action, exact lease, recovery root and audit
receipt. Selective CI is opt-in; unknown analysis must broaden or reject. A trusted
administrator must select the governing object externally. A repository cannot
lower its own gate by including a policy object. Unknown required evaluator features
must fail closed at installation/evaluation; neither is implemented here.
Signatures are detached attestations. Predecessors may expire without invalidating
current policy bytes. Policy evaluation, authorization, revocation and trust remain
separate from canonical parsing and source recovery.
