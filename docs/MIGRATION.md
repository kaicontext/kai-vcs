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

## P0 implementation and evidence

| Area | Implemented | Still required |
| --- | --- | --- |
| Object formats (T003–T007/T011) | Draft codecs for all 19 types, golden vectors, independent reader, malformed-input tests and typed closure checks | Format review/freeze, capture differentials, immutable installation and publication atomicity |
| Large content (T010) | Chunk boundaries and ordered full-content verification | Bounded 10 GiB capture/restore/transfer qualification |
| Recovery (T012) | Independent source/repository bundles, exact-byte recovery and fresh-runner drills on Linux/macOS/Windows | Broader fixture coverage and platform materialization qualification |
| Failures and transactions (T013–T025) | In-memory transaction oracle, controlled write errors and 50 process-kill/restart boundaries | Durable storage/root transactions, concurrency and power-loss qualification |
| Conflicts and staging (T034/T046) | Checkpoint working/staged conflicts and complete ordered alternatives/rename bindings | Production staging, resolution and restart behavior |
| Retention (T040/T087–T091) | Explicit source versus retained metadata edges and reference model | Durable pins, expiry, GC barriers and operation recovery |
| Contracts and profiles | Capability/outcome validation, evidence context and collaboration frontier verification | Protocol binding, authentication and runtime enforcement |
| F01–F07 | 62 reproducible cases, 78 recorded artifacts, expected byte/kind/ID manifest | Runtime execution as later milestones become available |

The transaction oracle explores 137,257 states through six actions involving
save, undo, GC, retention and leases. It models intended semantics; it does not
establish disk durability. See [FIXTURES.md](FIXTURES.md) for executed versus
pending fixture checks and [RECOVERY-EXPORT.md](RECOVERY-EXPORT.md) for recovery
scope and platform limitations. Detailed format contracts live in the individual
format documents, rather than an implementation diary.

## Validation

At implementation commit `c3f73a9`, local checks passed 144 Rust tests, formatting
and strict Clippy. Both [Rust conformance](https://github.com/kaicontext/kai-vcs/actions/runs/38563690366)
and [independent recovery qualification](https://github.com/kaicontext/kai-vcs/actions/runs/38563690379)
passed on Linux, macOS and Windows. Each platform ran the fixture comparison and
50 process-kill/restart boundaries. These results validate the stated tooling
scope, not P0 completion or production durability.

Execution logs belong to GitHub Actions. Per-platform fixture manifests and fault
reports are uploaded as `fixture-fault-evidence-*`; fresh-host recovery reports
are uploaded as `recovery-report-*`. Artifacts are subject to GitHub retention;
regenerate them with the checked-in workflows and commands in
[FIXTURES.md](FIXTURES.md). Golden expectations remain versioned under
`tests/fixtures/`; historical developer logs and source fingerprints are omitted
from the review tree and remain available in earlier Git commits.

## Next P0 work

The [P0 exit audit](P0-EXIT-AUDIT.md) maps each deliverable to evidence and
separates vector/model results from later runtime acceptance. The
[authority/licensing decision review](decisions/P0-AUTHORITY-LICENSING.md)
records the implementation choice; independent approval remains pending.

1. Review capability/outcome contracts against storage and transport needs;
   define remaining versioned request/session framing.
2. Review evidence/frontier completeness, protocol binding, applicability and
   signed-payload interoperability. Resolve size/streaming and submap choices.
3. Broaden clean-host recovery and runtime fixture execution before closing T012.
4. Connect fault hooks to platform/storage implementations when available;
   review ADRs for native authority and MIT preservation-kernel licensing.
5. Obtain independent storage/interoperability review before freezing format v1.

## Current change contract

The Rust kernel prepares and verifies canonical bytes, IDs, types, closure and
chunk assembly. The independent reader can export immutable source/repository
state; it does not install a P1 database or editable workspace. Existing output
files are refused. No root transaction, production repository migration, policy
trust, signature authentication or live collaboration is implemented.

Formats remain experimental native v1. Process-kill checks do not establish
power-loss durability; rename, directory-sync, root-commit and lease boundaries
await P1 adapters. The existing Go production authority remains unchanged.
