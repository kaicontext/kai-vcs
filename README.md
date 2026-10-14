# kai-vcs

The Rust implementation of the native Kai version control specification in
[`kai-vcs-spec`](../kai-vcs-spec/README.md). Work follows its P0–P10 dependency
gates. This is an experimental preservation kernel, not a replacement CLI yet.

## Implemented P0 increments

- Type/domain-separated BLAKE3 IDs and strict native framing.
- Canonical deterministic CBOR trees with raw filenames, executable files,
  symlinks, directories, and algorithm-qualified Gitlinks.
- Causal revisions with ordered parents, logical change IDs, opaque authors and
  messages, explicit optional provenance, imported envelopes, and conflict refs.
- Raw chunks and large-blob manifests with verified ordered assembly.
- Canonical evidence input/environment profiles with bounded source/context
  verification, exact Gitlink inventories and visible reproducibility limitations;
  see [evidence profiles](docs/EVIDENCE-PROFILES.md).
- Bounded JSON capability negotiation and mutation outcomes with explicit exit
  codes, interruption handling and exact decimal-string generations. See
  [interface contract](docs/INTERFACE-CONTRACT.md); no mutation CLI is implemented.
- Collaboration-barrier records with explicit replica frontiers/exclusions and
  checkpoint binding; declarative policy records without installation authority.
- Backup/transfer manifests with exact inventories and incremental prerequisites,
  portable backup roots, feature checks and bounded read-only package verification.
  Durable backup creation, transport and restore are not implemented.
- Git envelopes preserving raw blob/tree/commit/tag bytes and checking SHA-1 or
  SHA-256 IDs; independent extraction with `--git-raw`.
- Evidence, detached attestation records and analysis manifests, with exact source
  bindings, explicit uncertainty and independent proposed signing-payload vectors.
  Signature verification and policy trust are not implemented.
- Repository views with typed refs, deletion generations, divergent draft heads,
  workspace roots, shelves, and explicit retention pins; see [view registry](docs/VIEW-FORMAT.md).
- Persistent conflict alternatives, independent working/staged checkpoint
  conflicts, operation audit records, change events and annotated tags.
- Iterative, bounded verification of native required closure, with required and
  retained metadata references distinguished.
- Conflict/tree consistency checks that refuse missing rename bindings,
  invented resolved files and paths shadowed by ordinary file ancestors.
- An independent Rust reader/encoder, published vectors, malformed-input and
  differential tests, and a small independent transaction/retention oracle.

The reader has **no dependency on the production crate**. It can inspect an
individual canonical object and extract raw blob/chunk bytes. The new
`kai-bundle-reader` independently verifies bounded standalone source-history
bundles and exports resolved snapshots as tar archives. It also recovers complete
portable backup state, including refs, workspaces, conflicts and metadata. Live
database installation and clean-host T012 qualification remain open. See
[BUNDLE-FORMAT.md](docs/BUNDLE-FORMAT.md) for supported types and limits.

```sh
cargo test --workspace --locked # Includes tests against installed upstream Git
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo run -p kai-object-reference --bin kai-object-reader -- /path/to/object
cargo run -p kai-object-reference --bin kai-object-reader -- /path/to/blob --raw
cargo run -p kai-object-reference --bin kai-object-reader -- /path/to/git-envelope --git-raw
```

Reproduce the checked-in vectors independently:

```sh
cargo run -p kai-object-reference --example vectors > /tmp/kai-vectors.tsv
diff tests/fixtures/objects.tsv /tmp/kai-vectors.tsv
cargo run -p kai-object-reference --example vectors -- --signing-payloads > /tmp/kai-signing.tsv
diff tests/fixtures/attestation-payloads.tsv /tmp/kai-signing.tsv
```

## Watch the ASCII demo

```sh
python3 tools/demo/ascii_movie.py
```

A short terminal movie runs the real Rust sample: bundle creation, independent
verification, exact archive restoration, then rejection of a deliberately corrupted
copy. It uses a fresh temporary directory and retains the restored files and command
log. Requires Python 3, Cargo with cached dependencies, and a capable POSIX tar tool.
Use `--fast` to skip animation, `--record /tmp/kai-demo.cast` to save a recording,
or `--replay /tmp/kai-demo.cast` to watch that recording without rerunning Rust.

## Try a complete source bundle

Run from the repository root. The example creates synthetic source, not a capture
of your working checkout. It prints the selected tree ID. The reader verifies all
bundled source objects before creating a new archive; it refuses existing outputs.

```sh
demo_dir=$(mktemp -d)
root_id=$(cargo run --locked --offline --example source_bundle -- "$demo_dir/source.kai-bundle")
cargo run --locked --offline -p kai-object-reference --bin kai-bundle-reader -- "$demo_dir/source.kai-bundle"
cargo run --locked --offline -p kai-object-reference --bin kai-bundle-reader -- "$demo_dir/source.kai-bundle" --archive "$root_id" "$demo_dir/source.tar"
mkdir "$demo_dir/restored"
tar -xf "$demo_dir/source.tar" -C "$demo_dir/restored"
ls -l "$demo_dir/restored"
```

The restored example has `hello.txt`, executable `run.sh`, `data/bytes.bin`, and
`hello-link` pointing to `hello.txt`. Archive export runs no saved scripts or hooks.
Restoration requires a tar tool/filesystem capable of the saved kinds and names.
Gitlinks and conflicted snapshots refuse source export. Their native objects can
be recovered intact with `--recover`; incremental bundles remain unsupported.
This is portable recovery, not live repository initialization or durable backup.

## Try repository recovery

This second synthetic fixture includes branches, divergent drafts, a deleted ref,
a workspace checkpoint, staged content, a delete/modify conflict and recovery
pointers. It is validated by both the production and independent implementations.

```sh
recovery_dir=$(mktemp -d)
cargo run --locked --offline --example repository_bundle -- "$recovery_dir/repository.kai-bundle"
cargo run --locked --offline -p kai-object-reference --bin kai-bundle-reader -- "$recovery_dir/repository.kai-bundle" --index
cargo run --locked --offline -p kai-object-reference --bin kai-bundle-reader -- "$recovery_dir/repository.kai-bundle" --recover "$recovery_dir/recovery.tar"
mkdir "$recovery_dir/recovered"
tar -xf "$recovery_dir/recovery.tar" -C "$recovery_dir/recovered"
```

The recovered directory contains the exact manifest, every inventoried object,
a readable `INDEX.txt`, and a byte-identical bundle. Use the example's resolved
revision ID with `--archive ID OUTPUT.tar` to export that snapshot. Unresolved
revisions/checkpoints preserve all alternatives but refuse plain source export.
See [recovery export](docs/RECOVERY-EXPORT.md) for bounds and authority semantics.

## Compatibility

The local work on framed blob IDs and its regression tests is retained. Blob
encoding now returns a `Result` and IDs are typed `ObjectId` values, consistently
with other object types. Existing valid framed blob bytes/IDs do not change.
The prototype `KV` tree encoding is superseded and explicitly rejected. No
legacy `.kai`, Git repository, production CLI, or stored history is migrated.

Read [FORMAT.md](docs/FORMAT.md), [RECOVERY-FORMAT.md](docs/RECOVERY-FORMAT.md),
[VIEW-FORMAT.md](docs/VIEW-FORMAT.md), [METADATA-FORMAT.md](docs/METADATA-FORMAT.md),
[GIT-ENVELOPE-FORMAT.md](docs/GIT-ENVELOPE-FORMAT.md),
[PACKAGE-FORMAT.md](docs/PACKAGE-FORMAT.md),
[CONTROL-FORMAT.md](docs/CONTROL-FORMAT.md), and their CDDL files for the implemented registry.
[MIGRATION.md](docs/MIGRATION.md) tracks all remaining
conversion work and acceptance gaps. The native kernel will not depend on Git,
the semantic graph, a model, a UI, or a hosted service. Git-specific work stays
in the adapter required by the specification.

## Current limits

P0 is **open**. All 19 object types have draft codecs. Embedded manifest profiles,
capability/outcome review, clean-host extraction qualification, platform fault interfaces,
full fixture generators, and independent reviewer approval are still required.
There is no durable object store, metadata transaction, commit, checkout, remote,
or migration authority in this increment. Pure object verification does not
acknowledge a saved revision or certify power-loss durability. All P1–P10 release
gates, including the 60-day sole-primary pilot, remain outstanding.

License: MIT, including the independent reader.

The draft [collaboration frontier profile](docs/COLLABORATION-FRONTIER.md) binds
replica and per-file frontiers to a complete materialized tree inventory. Its
read-only verifier checks immutable declarations; live writer coordination and
CRDT operation replay remain unimplemented.
