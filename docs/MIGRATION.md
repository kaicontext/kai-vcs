# Rust migration tracking

Implementation contract: `kai-vcs-spec/SPEC.md`, `ACCEPTANCE.md`, and
`DELIVERY.md`. Rust replaces the delivery plan's tentative Go language choice;
all invariants, phase dependencies, and release evidence requirements remain.
The request to convert everything applies to the native VCS responsibilities
below; UI and analysis can call the Rust API without owning source history.

## Status

P0 is in progress. No phase exit or production stability claim is made. The
current contributor implements the preservation-kernel increment; independent
storage and interoperability reviewers are still unassigned. No invented
review approval is recorded. Format freeze waits for those reviews and the
remaining P0 deliverables.

| Phase | Rust work | State |
| --- | --- | --- |
| P0 | Schemas, vectors, independent reader, state oracle, capabilities, outcomes, fault interfaces | Nineteen object types, recovery semantics, closure checks and reference model implemented; gate open |
| P1 | Immutable storage, platform barriers, SQLite WAL/FULL root transactions, leases, backup/restore | Pending P0 |
| P2 | Exact tracked capture/staging, checkpoints, journaled safe materialization | Pending P1 |
| P3 | Revision DAG, refs/generations, changes, undo/revert, log/blame/bisect | Pending P1 |
| P4 | Conservative merge, durable conflicts, history reshaping | Pending P2/P3 |
| P5 | Native transfer, quarantine, receipts, server ref authority | Pending P1/P3 |
| P6 | Git envelopes/import/export, authority modes, bridge receipts | Pending P2/P3 |
| P7 | Stable API and CLI, evidence/policy, desktop/agent integration | Pending P4/P5/P6 |
| P8 | Resumable side-by-side migration, scale, maintenance | Pending P7 |
| P9 | Independent review, security, complete qualification | Pending P8 |
| P10 | 60-day sole-primary pilot and release authorization | Pending P9 |

## Go-to-Rust responsibility map

| Existing responsibility | Rust destination |
| --- | --- |
| `kai-core/cas` and source object identity | `id`, `codec`, object types; new namespace |
| `kai-engine/graph` source object storage | future `storage`; graph/search stays derived |
| `kai-engine/snapshot` | future `workspace`; old snapshots are migration input |
| `kai-engine/ref`, workspace history | future `repository`, `history`; transactional generations |
| `kai-core/merge`, workspace integrate/rebase/cherrypick | future `merge`, `history`; semantics advisory until qualified |
| `kai-engine/remote`, server ref/ingest writers | future `transport`, one repository transaction authority |
| server custom Git transport and bridge commands | future `gitbridge`; maintained upstream Git adapter per ADR-001 |
| CLI/desktop save, ship, close, recover | shared versioned Rust `api`; adapters stop reconstructing authority |
| graph garbage collection of source content | future `retention`; typed roots and leases |
| CRDT snapshot publication | optional collaboration barrier; outside native save authority |

## P0 evidence mapping

| Acceptance IDs | Current evidence | Still required |
| --- | --- | --- |
| T003 | Input ordering/raw-byte tree identity tests | Real path/native/Git capture differential |
| T004 | A→B→A causal revision identity with equal timestamps | Production log/revert/bisect |
| T005 | Thirty-four golden vectors for nineteen object types, separate generic-CBOR reader, exhaustive truncation and single-byte mutation corpus | Remaining schemas, full F01–F07, fuzz qualification |
| T006 | Claimed-ID mismatch rejection | Every disk/server writer, immutable installation |
| T007/T011 | Typed closure, missing/wrong-type objects, 2,000-deep history, limits | Publication atomicity, partial/shallow protocol, cycle fixtures |
| T010 | 16 MiB threshold, 4 MiB chunks, ordered full-content verification | Bounded capture/restore/transfer of 10 GiB, full qualification |
| T017–T019/T022–T025 | Independent in-memory oracle for generations, batch updates, receipts, pins/leases | Production DB/process/concurrency/crash tests |
| T012–T016 | Single-object blob/chunk/Git-body extraction only | Complete independent extraction and durability harness |
| T034/T046 | Checkpoint working/staged conflicts, every ordered alternative including missing values, complete rename-path bindings | Production staging, bundle/clone/restart and resolution |
| T040/T087–T091 | Explicit permanent source edges versus retained audit/checkpoint edges; independent oracle | Durable retention pins, expiry events, GC barriers and operation recovery |

The oracle explores 137,257 states through six actions of save/undo/GC/retention/
lease changes. It models intended semantics; it cannot establish disk durability.
The fixed mutation corpus is a regression tool, not a statistical safety proof.

### First-increment check record — 2026-10-14

Working-tree implementation on base commit
`f6c8dd25a02345bc7a96f65de3af7a103de9260e`; changes are not committed.
Host: macOS 26.3 (25D2125), arm64. Rust 1.99.0
(`b940084d7`, 2026-09-28), Cargo 1.99.0 (`5f94df478`).

- `cargo test --workspace --locked --offline`: 39 tests passed, no failures.
  Includes 11 preserved blob tests, 20 object/conformance tests, 7 oracle tests,
  and 1 separate-process independent-reader test.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Independent vector regeneration compared byte-for-byte with the checked-in
  fixture: passed. Fixture SHA-256:
  `8ef3defb5165cc3546beb8891d274ba037098459a987714c7cb770bf85779034`.
- Generated-byte fixture seed: `0x4b41495f50305f01`; mutation masks are
  `01`, `20`, `80`, `ff` at each byte of each of the nine golden vectors.

CI configuration covers Linux/macOS/Windows using the same pinned Rust version,
but remote CI has not run. These are developer checks of codecs and models;
filesystem durability, crash testing, release binaries, and cross-platform
certification are not covered by this record.

### Recovery-increment check record — 2026-10-14

Same base commit, local OS/toolchain and uncommitted working tree as above.
Implemented conflict (5), checkpoint (8), operation (7), change-event (4), and
annotated-tag (15) codecs in both Rust implementations. The published registry
is [RECOVERY-FORMAT.md](RECOVERY-FORMAT.md), with [recovery.cddl](recovery.cddl).

Closure checks now verify every alternative, reject empty directory alternatives,
require exact conflict/rename manifest bindings, reject unresolved paths that
also exist in ordinary trees, and reject file ancestors hiding unresolved paths.
Working-tree and staged conflicts are checked independently. Operation links and
checkpoint predecessors remain retained metadata; they do not silently become
permanent revision ancestry. No ref mutation or retention expiry is implemented.

- **53 tests passed**: 39 existing tests (now including expanded vectors) and
  14 recovery tests. [Test output](checks/p0-recovery-tests.log).
- Clippy with `-D warnings` passed. [Output](checks/p0-recovery-clippy.log).
- Formatting, independent vector regeneration and whitespace checks passed.
  The original nine vectors are byte-for-byte unchanged.
  [Output](checks/p0-recovery-format-vectors.log).
- Current fifteen-vector fixture SHA-256:
  `8c13ff1bdb18ef7fd243679373942a8cb914f7f83c9e38ed21dd245333cf751b`.
- [Source fingerprint](checks/p0-recovery-sources.sha256) records the exact source,
  tests, schemas and dependency files used for these checks. Verify from the
  repository root with `shasum -a 256 -c docs/checks/p0-recovery-sources.sha256`.

These are P0 developer checks, not independent human approval, release evidence
for storage durability, or a passed P0 exit gate. CI/platform certification has
not run. No source data, Git state, Go clients, or existing repositories migrate.

## Repository-view increment — 2026-10-14

Implemented type 6 in the production codec and independent reader after defining
[VIEW-FORMAT.md](VIEW-FORMAT.md) and `view.cddl`. The initial representation uses
canonical inline maps with explicit size limits; no sharded submap format is
advertised. Views preserve ref generation tombstones, explicit local branch
alternatives, divergent change heads, workspace bases/checkpoints, shelves and
typed retained-root selections. Ref names are case-sensitive NFC UTF-8, validated
without renaming. Both implementations use the locked Unicode normalization
primitive; their framing/schema/ordering logic remains separate.

Closure verification follows all selected roots, validates target types even
when already visited, binds draft heads to change IDs and checkpoints to workspace
IDs, and continues to skip archival operation/checkpoint links. Explicitly pinning
an audit object does not recursively retain its expired history. Transactional
CAS, generation advancement, expiry policy and persistence are still unimplemented.

- **63 tests passed**, including 10 view-specific tests and the expanded
  differential mutation/truncation corpus.
- Both implementations agree on **17 vectors**; the previous 15 are byte-for-byte
  unchanged. Current vector SHA256: `2eb021287b04c7ffcbdb391b3acfc2d448486ad3934fff5fb6da93b85dc28779`.
- Workspace Clippy with warnings denied and formatting checks passed.
- Evidence: `checks/p0-view-tests.log`, `checks/p0-view-clippy.log`,
  `checks/p0-view-format-vectors.log`, and `checks/p0-view-sources.sha256`.
- Checks ran locally on macOS; remote CI and independent reviewer approval have
  not run. P0 is still open. No existing data or repository authority changed.

## Advisory-metadata increment — 2026-10-14

Registered evidence (9), attestation (10) and analysis-manifest (11) before
implementing both codecs; see [METADATA-FORMAT.md](METADATA-FORMAT.md) and
`metadata.cddl`. Evidence binds exact revision/tree and versioned input/environment
manifest bytes. Closure verifies source claims. Artifact expiry preserves the
historical claim; trust classes remain producer claims, not verified execution.
Analysis preserves coverage, exclusions, parser failures and unresolved regions;
a contradictory complete status is rejected. The proposed attestation signing
payload binds typed subject, purpose, domain, signer identity and optional times,
while excluding signature bytes. Unknown signature formats are preserved without
verification. Source capture never requires analysis execution.

- **71 tests passed**, including 8 metadata-specific tests and the expanded
  differential mutation/truncation corpus.
- **20 object vectors** agree in both implementations, plus a separately checked
  attestation signing-payload vector. All previous 17 object vectors are unchanged.
- Object-vector SHA256: `0e7d64ec9f3c4889b4f6333258b85827baaf6990290b2db9359708f38fc4d657`.
- Formatting and workspace Clippy with warnings denied passed locally on macOS.
- Evidence: `checks/p0-metadata-tests.log`, `checks/p0-metadata-clippy.log`,
  `checks/p0-metadata-format-vectors.log`, `checks/p0-metadata-sources.sha256`.

No commands are executed, signatures verified, policies trusted or external state
read by these codecs. Input/environment profile interpretation, applicability,
SSH interoperability, durable storage and the remaining P0 gate work remain open.
Remote CI and independent reviewer approval have not run.

## Git-envelope increment — 2026-10-14

Implemented type 12 after publishing [GIT-ENVELOPE-FORMAT.md](GIT-ENVELOPE-FORMAT.md)
and `git-envelope.cddl`. Both implementations preserve exact uncompressed object
bodies for blob/tree/commit/tag, verify the original algorithm-qualified SHA-1 or
SHA-256 ID, and retain opaque headers, signature blocks, binary bytes and parent
order. The independent reader extracts verified bytes with `--git-raw`; malformed
or corrupted envelopes return integrity failure without stdout. Native revision
and tag envelope links enforce commit/tag kind, and conflicted revisions reject
ordinary Git commit envelopes.

- **79 tests passed**, including 7 envelope tests and a separate-process Git
  extraction test, plus expanded mutation/truncation coverage.
- **28 object vectors** agree between implementations. The prior 20 object vectors
  and attestation signing vector are unchanged.
- Object-vector SHA256: `ab6c82ede5480cf684be31861486bc4f596cd3d32b4fd332cda5b11ac3bd5393`.
- Upstream Git **2.50.1 (Apple Git-155)** independently hashed, stored and extracted
  all four kinds under SHA-1 and SHA-256 in temporary isolated repositories.
- Workspace Clippy (warnings denied), formatting, and vector reproduction passed.
- Evidence: `checks/p0-git-envelope-tests.log`, `checks/p0-git-envelope-clippy.log`,
  `checks/p0-git-envelope-format-vectors.log`, `checks/p0-git-envelope-sources.sha256`.

This does not implement repository import/export, raw Git semantic validation,
collision-safe import, native/Git projection mapping, signature verification, or
streaming envelopes larger than the structured-object limit. An archival envelope
is not proof of valid native history. The Git CLI is used only in conformance tests;
production encoding/decoding and source closure remain independent of Git. Checks
ran locally on macOS; remote CI and independent reviewer approval have not run.
P0 is still open. No existing repository was imported, exported or migrated.

## Backup/transfer-manifest increment — 2026-10-14

Implemented backup (16) and transfer (19) records after registering
[PACKAGE-FORMAT.md](PACKAGE-FORMAT.md) and `package.cddl`. This follows SPEC §9.7
(bundle roots/inventories/checksums/prerequisites) and §15.4 (portable consistent
backup roots and declared retention), within DELIVERY P0's format/model scope.
The new read-only verifier checks every inventory byte, exact prerequisite source,
required features, typed closure and backup operation/post-view linkage. Opaque
optional metadata is hash-checked but cannot satisfy interpreted source closure.
Results distinguish verified standalone contents from verified incremental contents;
a decoded completeness claim is not a verification result or durability receipt.

- **89 tests passed**, including 10 package-specific verification tests and expanded
  independent malformed-input/mutation/truncation coverage.
- **32 object vectors** agree between implementations; all prior 28 object vectors
  and the signing-payload vector are unchanged.
- Object-vector SHA256: `ecd3ef4f800f42e33a3038cb35fdf6e57546320cb0e6ae8b080edd83fba8ad39`.
- Workspace Clippy (warnings denied), formatting and vector reproduction passed.
- Evidence: `checks/p0-package-tests.log`, `checks/p0-package-clippy.log`,
  `checks/p0-package-format-vectors.log`, `checks/p0-package-sources.sha256`.

These are experimental field-registry choices derived from the spec, awaiting P0
review/freeze. No claim is made that a snapshot generation was actually committed:
P1 must acquire the root snapshot and lease and durably create/restore the backup.
No package container, restore writes, network session, replication receipt or
independent full bundle extractor is implemented. Remote CI and independent review
have not run. The remaining object registries are collaboration barriers and policy;
other P0 capabilities, fault harnesses and acceptance gaps remain open.

## Collaboration-barrier/policy increment — 2026-10-14

Registered and implemented types 17/18 in both implementations from SPEC §§11.4,
12.3 and 14, with [CONTROL-FORMAT.md](CONTROL-FORMAT.md) and `control.cddl`.
Barrier records preserve participating frontiers, explicit exclusions, epoch,
generation, materialized tree and namespace-manifest identity. Checkpoint closure
requires matching workspace/tree and existing barrier-consistent capture class.
Policy records preserve exact protected refs, check/reviewer requirements, claimed
principal labels, conservative selective-CI mode, administrative rewrite mode and
required evaluator features. Mandatory spec gates cannot be switched off by fields.
Parsing does not grant trust, install policy or authorize publication.

- **94 tests passed**, including 5 new control-record tests and expanded independent
  mutation/truncation coverage.
- **34 object vectors** agree in both implementations. All prior 32 vectors and
  the attestation signing-payload vector are unchanged.
- Object-vector SHA256: `3563d9fc805db5b5f2113295f7f197880e6e65016678354c362b851b0b275f56`.
- Workspace Clippy (warnings denied), formatting and vector reproduction passed.
- Evidence: `checks/p0-control-tests.log`, `checks/p0-control-clippy.log`,
  `checks/p0-control-format-vectors.log`, `checks/p0-control-sources.sha256`.

All 19 object types now have draft codecs, not completed semantic implementations.
Frontier manifest interpretation, acknowledged writer pause/drain/epoch protocols,
retired-replica rejoin, policy installation/evaluation and authorization remain open.
P0 still requires capability/outcome contracts, complete embedded profiles, the
independent extractor, fixture/fault work and independent review. No live workspace
or trusted server policy changed. Checks ran locally; remote CI and reviewer
approval have not run. No phase gate or format freeze is claimed.

## JSON capability/outcome increment — 2026-10-14

Implemented the proposed JSON v1 contracts from SPEC §§9.1, 9.4 and 13.2 in
`src/interface.rs`, with [INTERFACE-CONTRACT.md](INTERFACE-CONTRACT.md), structural
JSON schemas and a shared accepted/rejected corpus. Capability negotiation checks
mutual protocol/format/compression/signature support, required features, requested
ref transactions, explicit complete/partial/shallow selection and receiver durability
requirements including exact replication-policy identity. It never silently converts
a complete transfer into a partial one or advertises local durability by default.

Mutation results validate exit codes against prepared/committed/pending/conflicted/
rejected/indeterminate outcomes, operation/request IDs, exact decimal generations,
ref updates/tombstones, recovery guidance and external receipt states. Interruptions
retain the known result instead of implying rollback. JSON rejects unknown/duplicate
fields, trailing values, oversized input and contradictory claims.

- **102 Rust tests passed**, including 8 interface-specific tests.
- **21 accepted/rejected JSON vectors** agree with an independent stdlib-only Python
  checker that does not import Rust code. This supplements the unchanged 34 object
  vectors and signing-payload vector; negotiation itself is unit-tested in Rust.
- Workspace Clippy (warnings denied), formatting and whitespace checks passed.
- Evidence: `checks/p0-interface-tests.log`, `checks/p0-interface-clippy.log`,
  `checks/p0-interface-reference.log`, `checks/p0-interface-sources.sha256`.

Structural JSON schemas do not replace cross-field semantic validation. No network
transport, mutation executor, authenticated capability exchange, durable outcome
store, transaction-status service or policy authorization is implemented. Claims
still require backing evidence from future P1/P5 operations. The reader CLI remains
read-only. P0 is open; full manifest profiles, extraction/fault fixtures and independent
review remain outstanding. Checks were local; remote CI/reviewer approval did not run.

## Evidence-profile increment — 2026-10-14

Defined [EVIDENCE-PROFILES.md](EVIDENCE-PROFILES.md) before implementing canonical
input/environment profile blobs and `verify_evidence_context`, following SPEC §11.2.
The verifier first checks required native evidence closure, then profile byte IDs,
canonical encodings, revision/tree bindings, review-target declaration, visible
limitations and the exact Gitlink inventory across all directory paths. Repeated
subtrees at different paths are checked separately; non-UTF-8 names are preserved.
Reads and path expansion are budgeted. No environment, command, secret or external
submodule is fetched/executed automatically.

- **112 Rust tests passed**, including 10 new context/profile tests.
- Two canonical profile examples plus **14 accepted/rejected profile cases** agree
  between Rust and an independent Python checker. The previous 21 interface cases
  still pass. Existing object formats/vectors remain unchanged.
- Workspace Clippy (warnings denied), formatting and whitespace checks passed.
- Evidence: `checks/p0-evidence-profile-tests.log`,
  `checks/p0-evidence-profile-clippy.log`, `checks/p0-evidence-profile-reference.log`,
  `checks/p0-evidence-profile-sources.sha256`.

Context validity does not authenticate a runner, prove dependency completeness,
resolve opaque secret-version tokens or grant review/policy approval. Review policy/
base IDs and overlay/dependency digests remain claims requiring acceptance-layer
verification and explicit retention selection. The collaboration frontier profile
is the next embedded-profile gap. Full extraction, failure harnesses and independent
review remain P0 work. Tests were local; remote CI/reviewer approval did not run.
This increment is local work after the earlier GitHub push; no phase gate is closed.

## Collaboration frontier profile increment — 2026-10-14

Implemented the draft `kai/collaboration-frontier/v1` profile from SPEC §12.3.
It binds workspace, epoch, generation, protocol, exact participant frontier and
complete raw-path materialization inventory to a barrier. Per-entry frontiers
cannot include excluded replicas or exceed the barrier. Protocol-defined log
digests explicitly commit to namespace history, including renames/deletes; they
remain claims, not validated operation histories. Shared subtrees are expanded at
every path, with bounded reads, bytes and path work. Decimal-string counters
preserve the entire u64 range in JSON. Existing native object vectors are unchanged.

- **119 Rust tests passed**, including seven new frontier tests covering raw names,
  shared subtrees, Gitlinks, context/participant mismatch, omitted/altered entries,
  future/excluded frontiers, corruption, missing objects and resource limits.
- **18 canonical profile cases** agree with the independent Python checker, now
  included in CI. Earlier 21 interface and 14 evidence-profile cases still pass.
- Workspace Clippy (warnings denied), formatting and whitespace checks passed.
- Evidence: `checks/p0-frontier-tests.log`, `checks/p0-frontier-clippy.log`,
  `checks/p0-frontier-reference.log`, `checks/p0-frontier-sources.sha256`.

This is an experimental profile and explicit read-only verifier. Generic closure
verification still accepts opaque manifests. Authentication, writer pause/drain,
operation-log encoding/replay, namespace convergence, successor epochs, retired
replicas and compaction proofs remain protocol work. No live collaboration or
production publication gate is claimed. Tests were local; P0 remains open.

## Independent source bundle/extractor increment — 2026-10-14

Added a documented versioned standalone transfer container and independent
`kai-bundle-reader`, without linking the production crate. It checks inventory
checksums/lengths, typed source closure, complete parent ancestry, owner/envelope
kind binding, and assembled large-file digest. Selected tree/revision/tag snapshots
export as deterministic tar archives with exact bytes, modes and symlink targets.
All verification and export preflight precede output creation; existing outputs
are refused. A production-encoder example generates a reproducible source bundle.

- **130 Rust tests passed locally**, including ten source-bundle tests and one
  standalone CLI test. Tests cover corruption/truncation/trailing data, missing/
  mistyped/opaque required objects, prerequisites/features, complete ancestry,
  tag selection, paths/aliases, raw and long names, large-blob assembly, logical
  budgets, actual tar restoration and refusal to overwrite existing output.
- Workspace Clippy (warnings denied), formatting and whitespace checks passed.
- Restored the example with system tar; exact binary/text/script bytes, 0644/0755
  modes, nested directory and symlink target matched. Unix long-name/link restore
  passes. Actual non-UTF-8 filename restore is Linux-only: this Mac filesystem
  refused it. Raw-name archive bytes are still verified locally.
- Evidence: `checks/p0-bundle-tests.log`, `checks/p0-bundle-clippy.log`,
  `checks/p0-bundle-restore.log`, `checks/p0-bundle-sources.sha256`.

Scope is bounded standalone **source-history** bundles. Full view/checkpoint/
conflict/metadata/backup extraction is not implemented. Unsupported interpreted
objects and unresolved revisions fail closed; Gitlink bundles verify but cannot
be exported as ordinary filesystem tar snapshots. Source export is not repository
restore, durable backup, or race-resistant checkout. P0 and clean-host T012 remain
open; tests are local and remote CI has not been checked.

## Independent repository recovery increment — 2026-10-14

Extended the independent bundle reader to type-16 backup roots and all 19
registered interpreted object types. Required closure now includes views, drafts,
workspaces, checkpoints, conflict alternatives and advisory metadata. Cross-object
checks cover draft/change IDs, workspace/checkpoint/barrier identity, evidence
source state, complete conflict path bindings and backup operation post-view.
Retained archival pointers may expire; included objects are never silently dropped.

`--index` describes refs (including tombstones/divergence), drafts, workspace and
staged state, conflicts and recovery pointers. `--recover` emits a safe-path tar
containing original manifest/objects, a derived index and byte-identical bundle.
Resolved checkpoint/revision snapshots export independently. Unresolved source
export is refused while its exact alternatives remain recoverable.

- **139 Rust tests passed locally**, including nine repository recovery tests and
  extended CLI recovery/index/no-overwrite/corruption checks.
- Workspace Clippy (warnings denied), formatting and whitespace checks passed.
- The synthetic backup passed production package verification and independent
  recovery. An isolated-directory drill copied only the reader executable and
  bundle, reset HOME/PATH, removed original input copies, then restored and
  compared the manifest and every object. The recovered bundle and index matched
  exactly; resolved source exported and unresolved source refused before writes.
- Evidence: `checks/p0-repository-recovery-tests.log`,
  `checks/p0-repository-recovery-clippy.log`,
  `checks/p0-repository-recovery-drill.log`,
  `checks/p0-repository-recovery-sources.sha256`.

This recovers portable immutable repository state, not an installed P1 database
or editable workspace. No operation replay, root publication, policy trust or
signature authentication occurs. Full clean-host/platform T012 qualification,
fixture/fault harnesses and independent review remain open. The local isolated
Mac drill is not a substitute for those acceptance gates. New work is local;
the preceding source reader and ASCII demo were pushed as `2113f88`.

## Recovery qualification harness — 2026-10-14

Added a dedicated Linux/macOS/Windows build-and-recovery workflow. A separate
fresh recovery runner receives only the standalone binary, deterministic bundles,
expected fixture identities, transport hashes and a standard-library harness;
there is no checkout or Rust installation. Reader subprocesses use empty PATH and
isolated HOME. Reports preserve passed cases and explicit platform limitations.

Added deterministic portable/raw/control/reserved/colliding path fixtures and
missing/incremental closures. The harness checks exact portable repository/object
and source recovery, aliases, corrupt/truncated/missing frames, trailing bytes,
unresolved state and refusal to overwrite existing partial output. Rust writer
fault injection verifies exact prefixes and errors for short/zero/broken-pipe
writes and flush failures; source and recovery exporters are both exercised.

- **140 Rust tests passed locally**, plus Clippy (warnings denied) and formatting.
- Local macOS harness: **19 passed cases**, with raw-name materialization and
  process-kill/power-loss qualification explicitly outstanding.
- CI workflow: `.github/workflows/recovery.yml`; fresh runner reports are uploaded
  as `recovery-report-ubuntu-latest`, `recovery-report-macos-latest`, and
  `recovery-report-windows-latest`. Consult the run/artifacts for remote outcomes;
  local results alone do not imply remote success.
- Evidence: `checks/p0-recovery-qualification-tests.log`,
  `checks/p0-recovery-qualification-clippy.log`,
  `checks/p0-recovery-qualification-local.json`,
  `checks/p0-recovery-qualification-sources.sha256`.

This covers a bounded hosted-runner recovery drill, not all T012 fixtures or P0
completion. Windows executable/symlink materialization, non-Linux raw filenames,
crash/durability qualification, full F01–F07 generators and independent reviewer
signoff remain gaps. The preceding repository recovery implementation was pushed
as `63f9d24` before this increment.

## Next P0 work

1. Review the proposed capability/outcome contracts against storage and transport
   implementations; define remaining versioned request/session framing.
2. Review evidence/frontier profile completeness and protocol binding, evidence
   applicability and signed-payload interoperability. Resolve remaining
   size/streaming and submap choices before freeze.
3. Qualify independent repository recovery on clean hosts/platforms (T012), with
   broader fixture coverage and failure cases before declaring the gate complete.
4. Add F01–F07 generators with byte/kind manifests and deterministic seeds.
5. Define fault-injection/platform interfaces and record ADR review for the native
   authority boundary and MIT preservation-kernel licensing.
6. Obtain independent storage/interoperability review before freezing format v1.

## First-increment change contract

Requirements: R01–R05, R15, R19–R21, R24, R27, R29–R30 at the object/preparation
layer. Observable behavior: callers receive canonical prepared bytes or an
explicit error; they can verify IDs, types, closure and chunk assembly offline.
Version: experimental native v1 envelope, registered subset only. Linearization:
none—no roots are mutated. Failure/recovery: the caller retains its inputs;
no filesystem state or historical head changes. Compatibility: valid framed
blob identities preserved, prototype tree bytes rejected, Go production state
untouched. Rollout: additive Rust library and separate read-only object tool;
no automatic imports, hooks, accounts, uploads, or cutover.
