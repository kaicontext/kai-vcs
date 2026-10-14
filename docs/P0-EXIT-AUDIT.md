# P0 exit audit

Audited implementation: `2377fb80a4632b9f2837df0a01599b1011e10665`.
Specification: sibling `kai-vcs-spec` commit
`ff60132f82b67bd663fc5939983003b5b5511493`, DELIVERY §4/P0,
ACCEPTANCE §§1–3, and SPEC §§3–4, 9, 13, 18.1.
This is an implementation self-audit, not independent approval.

**Disposition: P0 remains open.** P0's tooling exists, but format decisions and
independent approvals are incomplete. Passing tests does not authorize format
freeze or starting P1 as a completed dependency gate.

## Deliverable traceability

| P0 deliverable | Reviewable evidence | Disposition |
| --- | --- | --- |
| Complete CDDL/field registries and binary framing | [FORMAT](FORMAT.md), [schemas.cddl](schemas.cddl), recovery/view/metadata/Git/package/control registries | All 19 draft object types covered; schema precision and view encoding decisions remain |
| Canonical vectors in two implementations | [Object tests](../tests/objects.rs), [vectors](../tests/fixtures/objects.tsv), independent encoder/decoder in `tools/reference-reader` | Implemented; malformed, truncation and mutation cases included |
| Capability registry and command outcomes | [Interface contract](INTERFACE-CONTRACT.md), JSON schemas, [interface tests](../tests/interface.rs), independent Python checker | Draft implemented; authenticated session/request framing remains undesigned |
| Typed required/retained closure | [Closure verifier](../src/closure.rs), [package tests](../tests/package.rs), recovery/view tests | Implemented at object/package layer; no durable publication claim |
| Authoritative-state and ref-generation model | [Transaction oracle](../tests/transaction_model.rs) | Seven tests; bounded exploration asserts 137,257 visits; no unexplained modeled outcome, not exhaustive production proof |
| Independent encoder/decoder/extractor | [Reader](../tools/reference-reader/src/lib.rs), [bundle reader](../tools/reference-reader/src/bundle.rs), [recovery workflow](../.github/workflows/recovery.yml) | Implemented without a production-crate dependency; bounded fresh-runner recovery passes |
| F01–F07 generators | [Generator](../tools/fixtures/src/lib.rs), [expected manifest](../tests/fixtures/acceptance/manifest.json), [scope](FIXTURES.md) | Inputs implemented; synthetic signatures and deferred hostile runtime traces limit qualification |
| Fault-injection interfaces | [Hooks](../tools/fixtures/src/faults.rs), [process harness](../tools/qualification/faults.py) | Archive write/flush/response hooks exercised; storage hooks await P1 |
| Authority/licensing ADR review | [Implementation decision review](decisions/P0-AUTHORITY-LICENSING.md) | Published by this increment; independent signoff pending |
| Storage and interoperability reviewer approval | No approval record supplied | Blocking; reviewer identities and reviewed revision must be recorded |

## Exit tests: do not confuse model evidence with runtime acceptance

DELIVERY names “T003–T006 vectors” for P0, while ACCEPTANCE defines wider runtime
procedures under the same IDs. The vector subset can be assessed now; the complete
acceptance procedures cannot honestly be marked passed.

| Exit evidence | Current result | Uncovered portion |
| --- | --- | --- |
| T003 source-independent identity | Canonical raw-name/order tests; fixture generation agrees across directories and CI platforms | Native/Git capture differential, locales and timezones through real capture APIs |
| T004 history event identity | A→B→A revisions and repeated timestamps represented; model preserves causal history | User-facing log/revert/bisect execution in P3 |
| T005 canonical vectors | Production and independent codecs agree; hostile encodings rejected | Independent human schema approval and broader fuzz qualification |
| T006 immutable write | Claimed-ID rejection and oracle installation checks | Every real disk/server writer, quarantine and preservation after failed installation in P1/P5 |
| Save/ref/undo/GC model | Bounded model tests pass; ABA, batch rejection, retries, expiry and leases covered | Correspondence between the model and future storage implementation |

## Decisions and gaps before freeze

1. **View representation:** [VIEW-FORMAT](VIEW-FORMAT.md) only defines inline
   arrays and explicitly defers submaps. SPEC §4.7 calls for v1 submap ordering
   and encoding in P0. Define that representation and its independent vectors,
   or obtain an explicit spec amendment approving inline-only v1 with limits.
   Do not silently treat a reserved discriminator as a defined submap format.
2. **Schema precision:** `identity` timestamps and metadata/operation times use
   CDDL `int`, while codecs enforce signed i64. Tighten the published ranges
   and check all registries against codec limits. Add executable CDDL validation
   to qualification; the current independent reader is not a CDDL validator.
3. **Protocol boundary:** capabilities and outcomes are records, not a complete
   request/session protocol. Define request identity, version framing and
   binding to the future transaction contract before competing callers appear.
   Authentication/runtime receipt storage stays in P1/P5.
4. **Embedded profiles:** input/environment and collaboration frontier profiles
   exist. Review their applicability, protocol/log binding and signed payload
   interoperability; validation of declarations does not authenticate their claims.
5. **Fixture limitations:** F04 armor preserves signature-shaped bytes but is
   not a cryptographically valid signature corpus. F06 race/pack/authentication
   traces specify later runtime inputs. Track their execution under P2/P5/P6,
   without reporting the complete F04/F06 acceptance suites as passed.
6. **Accountability:** name the preservation owner, independent storage reviewer
   and interoperability reviewer. Record each approval against the exact format
   revision and any conditions. Automated PR scores do not supply these signoffs.

The next implementation increment should resolve schema precision with executable
checks, followed by the view-representation decision. P1 object installation,
SQLite root transactions and power-loss testing remain separate work; their
absence does not invalidate the P0 codec tests, but those tests cannot replace them.

## Validation reference

At the audited revision, local validation passed 145 Rust tests, 10 Python tooling
tests, formatting and strict Clippy. [Conformance CI](https://github.com/kaicontext/kai-vcs/actions/runs/38571765544)
and [independent recovery CI](https://github.com/kaicontext/kai-vcs/actions/runs/38571765377)
passed on Linux, macOS and Windows. The POSIX demo regression is skipped on Windows.
Raw-name and mode/symlink materialization limitations remain documented in
[FIXTURES](FIXTURES.md) and [RECOVERY-EXPORT](RECOVERY-EXPORT.md).
Logs and run reports remain CI evidence, not additional checked-in review files.
