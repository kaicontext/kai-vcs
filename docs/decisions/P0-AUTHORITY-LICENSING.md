# P0 decision review: native authority and preservation licensing

**Status:** Proposed for independent approval; records the implemented direction.
**Date:** 2026-10-14.
**Deciders:** Accountable preservation maintainer, independent storage reviewer,
and upstream-Git interoperability reviewer; identities not yet recorded.
**Basis:** SPEC ADR-001–003 (§§3.1–3.3), §18.1, and DELIVERY §4/P0 at spec commit
`ff60132f82b67bd663fc5939983003b5b5511493`.

## Context

Kai must preserve exact source bytes and kinds, causal history, unresolved conflict
alternatives, private checkpoints and recoverable operations without relying on
an analysis cache or hosted service. Existing Go state cannot silently acquire
native v1 identities. Git interoperability remains required, including an exit
path usable without Kai.

The Rust workspace currently prepares and verifies immutable object bytes. It
has no durable root authority yet. The repository already carries an MIT license;
this record neither relicenses other Kai repositories nor establishes ownership
of dependencies or future imported code.

## Decision

Continue with a native immutable object model and a dedicated Git adapter, as
specified by ADR-001. P1 will put committed generations/root pointers in a separate
transactional authority, with disposable indexes physically distinct. Prepared
objects and timestamps must never be treated as proof that a root committed.
Git-authoritative coexistence is an explicit adapter mode, not an uncontrolled
bidirectional mirror with two writers deciding accepted history.

Keep the native kernel and independent reader under the existing MIT license.
Keep published schemas, vectors and recovery tooling available without account,
service or license-server access. The 1.0 self-hosted transfer licensing requirement
remains binding when that component is implemented. The independent reader must
continue to avoid importing production serializers and validation logic.

## Options considered

| Option | Benefit | Cost / conclusion |
| --- | --- | --- |
| Git-only authority plus Kai metadata | Mature storage and immediate interoperability | Native conflicts/checkpoints/operations still need a substantial model; retain as coexistence mode |
| Native objects and transactional roots with Git adapter | Explicit native preservation/recovery semantics and independent offline verification | Highest storage/interoperability qualification burden; selected by the specification and current implementation |
| Existing semantic graph as authority | Smaller initial migration | Couples cache/analysis lifecycle to source retention and ambiguous legacy identity; rejected |
| Service-only preservation implementation | Central deployment control | Prevents independent recovery and vendor exit; inconsistent with SPEC §18.1 |

## Consequences and boundaries

- The native library does not invoke Git. Git-based fixture generation and
  differential tests are adapter/qualification tools; they do not confer a
  runtime Git dependency on native object verification.
- Exact Git envelopes preserve bytes, not signature trust or a completed importer.
  Future adapters must disclose their upstream Git dependency/version and losses.
- MIT is recorded in [LICENSE](../../LICENSE) and the three package manifests.
  This is evidence of the repository's current declaration, not a completed
  dependency-license or code-ownership audit.
- P1 must establish immutable installation, root linearization, retention leases,
  barriers and backup/restore against the oracle. Recovery of object candidates
  alone cannot invent a committed head after database loss.
- Production cutover remains side-by-side and explicit. This record changes no
  existing repository's source authority, stored object IDs or data.

## Review actions

- [ ] Name the accountable owner and independent reviewers.
- [ ] Storage reviewer approves native authority, root/retention semantics and
      independent recovery boundaries against an exact implementation revision.
- [ ] Interoperability reviewer approves Git-authority coexistence, byte-fidelity
      obligations and the planned adapter dependency boundary.
- [ ] Maintainer confirms ownership/licensing scope and schedules dependency review
      before release; future transfer implementation preserves SPEC §18.1 availability.

The implementation author records these choices; no independent approval is
claimed. P0 exit status is tracked in [the audit](../P0-EXIT-AUDIT.md).
