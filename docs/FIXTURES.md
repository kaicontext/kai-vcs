# Reproducible acceptance inputs and process faults

The `kai-fixtures` Rust tool generates P0 inputs for F01–F07 in the acceptance
specification. It requires upstream Git with SHA-256 repository support, but
neither the native kernel nor the independent reader depends on Git.

Run from the repository root, choosing an output directory that does not exist:

```sh
cargo run --locked --offline -p kai-fixtures --bin kai-fixtures -- /tmp/kai-fixtures
cargo build --workspace --bins --locked --offline
python3 tools/qualification/faults.py \
  --child target/debug/kai-fault-child \
  --reader target/debug/kai-bundle-reader \
  --bundle /tmp/kai-fixtures/F03/state-a.kai-bundle \
  --report /tmp/kai-fault-report.json
```

On Windows use `python`, add `.exe` to binary paths, and choose a suitable output
path. Offline commands require previously downloaded Cargo dependencies.

## Fixed expectations

Seed `0x4b41492d50302d31` is fixed. The checked-in
`tests/fixtures/acceptance/manifest.json` records artifact lengths/SHA-256,
native tree/content IDs, raw path bytes, file kinds, Git IDs, and expected
rejections. The integration test generates twice in different directories and
compares both against that manifest. Changes to expected bytes require reviewing
the manifest diff. Generation refuses an existing destination.

Native bundles are checked with the kernel's closure verifier and the independent
reader. Native IDs must agree between the implementations. Git raw object bodies
are read back through upstream Git. Git configuration is isolated from inherited
Git environment variables and global/system configuration. Physical Git index
stat data, compressed packs, and repository configuration are not golden byte
artifacts; their logical object/index expectations are recorded instead.

| Fixture | Generated inputs | Qualification limit |
| --- | --- | --- |
| F01 | Empty, binary, invalid UTF-8, newline variants, whitespace, BOM, 1 MiB line, NUL, marker-like text | Native encoding and bundle validation |
| F02 | Modes, relative/dangling/absolute/looping symlinks, raw names, case/Unicode collisions, reserved names, long paths, file/directory transition | Encoded paths; no claim all can be materialized on every OS |
| F03 | A→B→A, duplicate trees/messages/times, clock skew, merges, octopus/crisscross, disconnected history, rewrite event | Object closure; no mutable history engine |
| F04 | SHA-1/SHA-256 Git repos, signed/unsigned envelope shapes, unusual bytes/headers, tags, notes/custom refs, gitlinks, alternate stores, shallow/replace/graft metadata | Signature armor is synthetic, not a valid cryptographic signature; full import/trust deferred |
| F05 | Staged/unstaged overlap, partial changes, ignored/untracked collision targets, sparse entries, intent-to-add, dirty submodule, detached HEAD, active merge | Real Git state; native workspace capture deferred |
| F06 | Bad envelopes, CBOR, lengths and paths; deterministic race/cycle/type/pack/authentication traces | Available codec/path rejection executes; runtime traces explicitly remain pending |
| F07 | Generated merge triples and pinned MIT Kai source; comments, wrappers, imports, directives, top-level edits, mode/rename/delete, binary and unsupported syntax | Inputs only; actual merge engine and outcomes deferred to P4 |

The real-source corpus includes its license and pinned provenance in
`tools/fixtures/corpus/`; `.gitattributes` disables checkout line-ending conversion
for this exact-byte corpus. Fixture completeness must be reviewed again as each
runtime capability arrives; this is not a claim that all acceptance gates pass.

## Fault interfaces and executed checks

`tools/fixtures/src/faults.rs` exposes a hook receiving the boundary and completed
byte count. Its writer adapter observes actual writes/flushes; controlled writers
inject short writes, an interrupted write, zero progress, storage-full,
permission-denied, and generic I/O errors. Tests assert retries or the exact
preserved prefix as appropriate.

The `kai-fault-child` process wraps the real independent recovery archive writer.
At each boundary it waits for a one-byte parent permit. The Python parent first
records a successful trace, then launches and kills a separate process at every
boundary. This is deterministic, without timing-based kill guesses. A fresh
reader process must refuse to overwrite the partial file without changing it,
and recover byte-identical complete output to a new path. The input bundle must
remain unchanged. Only the after-response boundary may have emitted `DONE`.
The small F03 state-A bundle currently exercises 50 boundaries.

Reports include platform, source commit, binary/input hashes, seed, elapsed time,
and per-boundary assertions. Reports record pass/fail status; failures preserve
completed boundaries, the failing stage/boundary, and error details before the
harness exits unsuccessfully. A supplied `GITHUB_SHA` avoids invoking Git; without
it, unavailable Git metadata is recorded as null. CI uploads available evidence
even on failure and lets the other platform jobs finish.
The tests use temporary directories and delete successful outputs. Reports identify
the host OS but do not certify any particular filesystem or mount configuration.

Rename, directory-sync, root-commit, and lease boundaries are reserved interface
names, not instrumented production operations. File flush is not fsync. Process
kill/restart does **not** establish power-loss durability, atomic root installation,
or exactly-once transport responses. Those require the P1 storage implementation,
platform adapters, and later transport runtime. P0 format review remains open.
