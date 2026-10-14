# Executable CDDL validation

P0 requires published schemas and independent evidence that their field contracts
match encoded objects. `tests/cddl.rs` reads the seven checked-in CDDL registries
and uses the pinned upstream [cddl 0.10.7 validator](https://docs.rs/cddl/0.10.7/cddl/)
to evaluate CBOR payloads. It also passes each relevant case through the production
codec and the independent Kai reader. The schema is executed directly; no copy of
its rules is embedded in a substitute checker.

Run the focused suite:

```sh
cargo test --locked --test cddl
```

After fetching dependencies, add `--offline` for a network-free run. The existing
`cargo test --workspace --locked` CI step includes this suite on Linux, macOS and
Windows. The validator and `ciborium` fixture manipulation library are exact-pinned
dev dependencies with transitive versions/checksums in Cargo.lock. They do not
become dependencies of the shipped kernel or independent reader. The validator's
supported feature combination includes JSON/AST support internally; only its CBOR
validation API is exercised here.

## Coverage

- All 34 published vectors cover all 19 object type codes. Each must pass the
  CDDL rule selected by its envelope type and both Kai codecs.
- Identity timestamps across revision/change/checkpoint/operation/tag/evidence/
  attestation, operation start/finish, evidence start/finish/exit status, and
  attestation issue/expiry are tested at i64 minimum/maximum and one beyond each.
- Ref generations, policy revisions and attestation versions reject zero; ordinary
  unsigned generations accept zero. Tests include u64 maximum and negative values.
- Timezone offsets, large-blob size threshold, and raw blob/chunk size limits have
  boundary cases. Required-field removal, unknown keys and explicit nulls are
  rejected for all map-shaped golden payloads.
- A non-minimal CBOR array encoding passes structural validation but fails both
  codecs. A large-blob length inconsistent with its chunk count also passes the
  schema and fails both codecs. These tests keep the distinction explicit.

## Scope and compatibility

The schema correction narrows previously broad `int` declarations to signed i64,
makes u64/positive-u64 limits explicit, and documents existing raw/large-content
bounds. The codecs already enforce these bounds. No accepted object bytes, object
IDs, envelope version, or golden vector changes; no migration is required.

CDDL describes payload structure, not the complete acceptance predicate. Envelope
magic/version/length, canonical shortest encoding and ordered keys, byte/path
ordering, cross-field relationships, resource budgets, identity hashes, and
required closure remain codec/verifier responsibilities. Validating one payload
alone cannot establish a saved revision, signature trust, or durability.

The two raw payload kinds are wrapped in a CBOR byte string only inside the test
adapter so their `.size` rules can be evaluated; stored framing remains unchanged.
This is a checked-in regression corpus, not exhaustive schema equivalence, fuzz
qualification, independent human approval, or completion of P0.
