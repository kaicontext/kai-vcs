# Publication preflight — P0 draft

`PublicationPlan::prepare` implements the pure compare-and-swap planning portion
of SPEC §§5.3, 7.1 and 9.3. It reads an immutable repository view and a validated
update-refs request, then returns a candidate view and prospective receipt. It
performs no writes, commits, locks, network requests or durability acknowledgments.
A plan is never evidence that publication happened.

## Inputs and checks

Inputs are the repository ID, current inline `View`, original `UpdateRefsRequest`,
proposed policy-outcome object ID and expected `PublicationDurability` from the
session. The repository ID must match the request. The view's canonical encoding
is validated and its ID is captured. The original request digest is recomputed.

The current namespace generation must equal the request's expected generation.
All requested refs are checked against that same view before building a candidate:

- A nonexistent name matches only expected generation zero and empty old targets.
- An existing name matches only its exact generation and complete target set.
- A retained deletion tombstone matches an empty target set at its positive
  generation; it cannot be mistaken for a never-created name.
- A target returning to its earlier value does not hide intervening changes,
  because the generation must still match (the ABA case).

Any mismatch rejects the whole plan. Nothing in the caller's view is modified,
even if earlier entries in the batch matched. Every updated ref advances once;
namespace generation advances once for the whole batch. Exhausted u64 counters
reject instead of wrapping. Deletion retains a tombstone. Creation starts at one;
recreation advances the tombstone's generation. Branch alternatives remain explicit.
Unchanged refs, draft heads, workspace roots and retention pins are preserved.

## Candidate and response preflight

The candidate's canonical view encoding is checked before returning a plan.
The prospective receipt is validated against the entire original request and
session durability. The planner also builds and validates a synthetic ordinary
committed v2 status response to check the complete 1 MiB envelope limit. Query IDs
have fixed width; a zero-filled ID is used only for this size calculation.

`committed_status_bytes()` reports the canonical size for the standard committed
response with empty recovery/external-receipt arrays and `interrupted: false`.
It does not reserve arbitrary later recovery text, external side effects or
materialization-pending outcomes. A publisher adding those must separately
preflight their complete response. Pretty-printed JSON can also exceed the limit;
use the bounded canonical serializer. The planner never returns completion advice.

A request fitting the input limit does not necessarily yield a deliverable receipt,
and a receipt fitting alone does not necessarily yield a deliverable status reply.
Both checks happen before a plan is accepted. No field or target is dropped to fit.

## Recheck and remaining commit gates

The returned plan exposes read-only candidate/prospective-receipt references and
the captured pre-view ID. `recheck_snapshot` checks the repository ID and the
entire canonical current view ID. Even a changed workspace root at an unchanged
namespace generation invalidates the plan. A future publisher must call this under
its writer lock/metadata transaction and recompute on failure. Calling it before
acquiring that lock cannot prevent a later race. No installation method exists yet.

This is structural planning, not an authorized publication decision. The supplied
policy-outcome ID is syntax-checked only. A real publisher still must verify:

- Repository/principal authorization and required negotiated capabilities.
- Policy outcome applicability to exact inputs, ref target object types, ancestry
  rules, complete required closure and object durability.
- Leases and roots, durable pre/post views and operation records.
- Serialized recheck and atomic metadata commit with required barriers.
- Persistent transaction/digest idempotency, receipt retention and recovery.

No target object is loaded or policy evaluated by this module. Replicated durability
is a supplied expectation, not a verified replication result. It accepts an inline
View; loading/materializing partitioned view submaps remains caller work. This
increment does not qualify storage or satisfy crash-recovery acceptance tests.

## Tests and demo

Tests cover an all-or-nothing multi-ref preview, preserved unrelated roots, target
and generation mismatch, ABA, creation/recreation, alternatives, deletion,
full-snapshot recheck, wrong repository, malformed inputs, counter exhaustion and
a valid standalone receipt whose complete status envelope is too large.

Run `cargo run --locked --offline --example publication_preflight` to preview a
replacement and deletion, then reject a stale snapshot. No repository is changed.
