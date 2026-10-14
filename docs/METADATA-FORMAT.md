# Advisory metadata — P0 draft

Registry for SPEC §§4.8–4.9, 11 and 14. This is an experimental encoding
increment, not signature verification, evidence acceptance, or format freeze.
Common framing, deterministic CBOR and the 64 MiB structured limit apply.
Required empty arrays are encoded; optional fields are omitted, never null.
Byte strings are opaque unless explicitly constrained below.

## Evidence (type 9)

| Key | Value |
| --- | --- |
| 0 | Native revision reference (type 3) |
| 1 | Native tree reference (type 2) |
| 2 | Source state: resolved=1, conflicted=2, unknown=3 |
| 3 | Evidence kind: execution=1, review=2 |
| 4 | Ordered nonempty byte-string arguments; first is nonempty. For review, one opaque nonempty review target identifier |
| 5 | Repository-relative cwd bytes; empty means root, otherwise canonical source path |
| 6 | Input manifest: `[nonempty format/version bytes, native blob/large-blob ref]` |
| 7 | Environment manifest: same tuple |
| 8 | Producer identity (common identity schema); attribution only |
| 9 | Claimed trust: unknown=1, self-reported=2, runner-produced=3 |
| 10 | Result: pass=1, fail=2, cancelled=3, timed-out=4, unavailable=5, unknown=6 |
| 11, 12 | Start and finish signed i64 Unix seconds; clock reversal is representable |
| 13 | Artifacts: `[name bytes, native blob/large-blob ref]` pairs, strictly name-sorted, nonempty names |
| 14 | Scope: `{0: covered byte-string labels, 1: excluded labels, 2: selection algorithm/version bytes, 3: fallback reason bytes}`; label arrays strictly byte-sorted, no empty labels |
| 15 | Ordered nonempty limitation statements (array may be empty) |
| 16 (optional) | Signed i64 exit status; execution only |

Review target/base/policy, dependency/submodule identities and dirty-overlay digest
belong in the declared input manifest. Toolchain versions, relevant non-secret
inputs, platform/container identity and runner-issued opaque secret-version tokens
belong in the declared environment manifest. The encoding binds exact bytes and
format identifiers, but interpretation/completeness of these versioned manifests
remains an acceptance-layer deliverable. A decoder never claims a manifest is
complete, reproducible or safe to execute, and never reads the host environment.
Do not populate these blobs with plaintext secrets or low-entropy secret hashes.
The bounded v1 input/environment profiles and explicit context verifier are now
defined in [EVIDENCE-PROFILES.md](EVIDENCE-PROFILES.md). Generic object decoding
continues to preserve other profile identifiers without interpreting them.

Revision/tree and input/environment manifests are required closure. Closure checks
that the tree matches the revision and that a resolved/conflicted claim agrees
with its conflict manifest; unknown makes no claim. Artifacts are retained-metadata
edges: expiry does not erase the statement. Callers must check artifact availability
separately. Pass and runner-produced are producer claims, never a verified runner
receipt. Parsing a record cannot grant trust or applicability to changed inputs.

## Attestation (type 10)

Required keys: 0 signature format bytes (nonempty), 1 positive format version uint,
2 domain bytes (nonempty), 3 subject type uint (1–19), 4 native subject reference,
5 purpose bytes (nonempty), 6 signer/key identifier bytes (nonempty),
7 claimed identity (common identity schema), 8 signature bytes (nonempty).
Optional keys 9 issuance and 10 expiry are signed i64 Unix seconds. If both are
present expiry must not precede issuance. Unknown signature formats can be decoded
and preserved but cannot be treated as verified.

`signing_payload()` is `KAI-ATTESTATION\0` followed by canonical CBOR of this map
with key 8 omitted. Thus format/version, domain, algorithm-qualified typed subject,
purpose, key identity, claimed identity, and optional times are all signed. This is
a proposed v1 signing contract for later SSH integration/interoperability review;
no signer or cryptographic verifier is supplied by this increment. Expiry and trust
are evaluated by policy, not by the decoder's wall clock. Subject is a required
edge; detached attestations are not linked back from the signed object.

## Analysis manifest (type 11)

Required keys: 0 tree reference (type 2), 1 analyzer/version bytes (nonempty),
2 resolver/version bytes (nonempty), 3 dependency digest, 4 build-context digest,
5 platform identity bytes (nonempty), 6 completion (complete=1, partial=2, failed=3),
7 covered labels, 8 unsupported labels, 9 parser-failure labels, 10 unresolved-edge
labels, 11 result references (blob/large-blob), 12 nonempty completion reason.
Digests are `[1, bytes32]`, algorithm 1=BLAKE3 over caller-defined versioned input
bytes. Labels identify paths/symbols/regions and are opaque nonempty bytes, strictly
sorted. Result references are strictly digest-sorted. Complete requires no recorded
unsupported regions, parser failures or unresolved edges; empty coverage/results
never assert zero impact. Partial/failed records can preserve any available results.

Tree is required closure. Result artifacts are retained-metadata edges and may
expire. No source object requires analysis to finish. Analyzer execution, manifest
completeness, applicability, impact selection, signature verification, trust policy,
and durable metadata retention remain later work.
