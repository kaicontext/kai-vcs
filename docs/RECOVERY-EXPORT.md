# Independent repository recovery export — P0 draft

SPEC §§4.7–4.9, 9.7, 15.3 and T012 require recovery without the original database
or main application. The independent reader now accepts either type-16 backup or
type-19 transfer manifests in the existing bundle container. It interprets all 19
registered types, preserves selected opaque objects, and verifies typed required
closure. Incremental bundles and unknown required features are still refused.

`kai-bundle-reader BUNDLE --index` prints a descriptive index. It includes backup
repository identity, generation/current-view/current-operation, every included
view's exact refs (including deletion tombstones and divergent alternatives),
draft heads, workspace base/checkpoint pointers, retained roots, checkpoints,
revision trees/conflict bindings, and operation recovery pointers. Bytes are hex
where needed; IDs and counters are exact. This index is derived, not authority.

`kai-bundle-reader BUNDLE --recover OUTPUT.tar` creates a portable recovery archive
with only fixed or content-addressed safe paths:

- `manifest.object`: original framed backup/transfer manifest.
- `objects/<digest>.object`: every inventoried framed object, including opaque data.
- `repository.kai-bundle`: byte-identical standalone bundle, usable by this reader.
- `INDEX.txt`: the descriptive index.

Extract into a fresh directory using a capable tar tool. The recovered bundle can
be verified and used to export resolved revision/checkpoint snapshots with this
reader alone. No SQLite replay, main Kai binary, Git process, hosted service or
network is needed. This restores portable immutable repository state, not a live
P1 database, editable workspace, authorization configuration or secret store.
No generation is installed or chosen by timestamp, and no operation is replayed.

All required edges and cross-object bindings are verified: view draft/change and
workspace/checkpoint identities; checkpoint/barrier workspace and tree; evidence
revision/tree/conflict state; backup current-operation post-view; Git owner kinds;
conflict path completeness and absence from ordinary source trees; large-file
assembly. Retained-metadata edges are not silently promoted to required closure.
Missing expired archival pointers remain explicit in the index and remain in the
original object bytes. Every packaged object is retained, even when unreachable
from current refs. Nested package records contribute typed roots; their inventory
is a declaration, not an assertion that all historical package bytes are present.

Unresolved revisions and checkpoints verify and recover with their base/side
alternatives intact. `--archive ID OUTPUT.tar` accepts any inventoried interpreted
tree/revision/tag/checkpoint, but refuses a conflicted revision/checkpoint (working
or staged conflicts). It exports checkpoint working trees; staged tree IDs remain
available in the index for explicit tree export. Exporting a raw tree is exactly
that tree, not a claim that an enclosing conflicted revision is resolved.

Limits from BUNDLE-FORMAT.md still apply. Recovery output is streamed, including
reconstruction of the bundle; index generation has a 64 MiB limit. Existing output
paths are refused. Verification/preflight precedes creation. Write failures may
leave a partial new archive; no durable acknowledgment is claimed. Embedded
profiles, signature trust, policy enforcement and CRDT protocol claims remain
separate from opaque metadata preservation and native closure verification.

Clean-directory and standalone-binary drills provide local evidence. A genuinely
clean-host/platform qualification and independent review remain P0 acceptance work.

## Automated independent recovery qualification

The `Independent recovery qualification` workflow builds deterministic fixtures
and the standalone reader on Linux, macOS and Windows, then passes only those
artifacts and a Python standard-library harness to separate fresh runner jobs.
Recovery jobs do not check out the repository or install Rust. Reader subprocesses
run with empty PATH and isolated HOME, without Cargo, Git or external helpers.
System tar and Python inspect/restore output independently. Artifact checksums
protect transport; semantic checks compare exact objects, bundle framing, expected
source bytes, refs/conflict state and file/archive metadata.

The harness covers missing/incremental/corrupt/truncated/trailing-data packages,
reserved/metadata/case/normalization path aliases, raw/control/long/Unicode names,
existing partial outputs, full portable metadata recovery and unresolved-source
refusal. Rust fault injection checks failed/zero/short writes and flush failures
at multiple byte boundaries; every failed export returns an error and any bytes
written are exactly a prefix of the expected archive. This is not a process-kill,
filesystem-crash, power-loss or durable-store certification.

Each OS uploads `recovery-report-<os>` with passed cases and explicit limitations:
Windows checks executable/symlink metadata in the archive without certifying
native materialization; raw POSIX name restoration is tested on Linux, while
other platforms verify exact archive bytes. Control-character names are restored
on POSIX only. These omissions are not reported as successful materialization.
The workflow exercises isolated recovery on fresh hosted runners, not all F01–F07
fixtures, every filesystem or the full T012/P0 acceptance gate.

To reproduce locally with the installed stable toolchain (Python 3, Cargo and tar):

```sh
python3 tools/qualification/prepare.py /tmp/kai-recovery-inputs --toolchain stable
python3 /tmp/kai-recovery-inputs/recover.py
```

The destination must not already exist. Default preparation pins Rust 1.99.0 as in
CI. The harness retains temporary files and writes `recovery-report.json` beside
its downloaded artifacts, including errors if a step fails.

The initial Windows run exposed ambiguous Unicode encoding in legacy tar headers.
Source export now emits POSIX pax `path`/`linkpath` records for valid non-ASCII
UTF-8 values. The records use byte-counted lengths, preserve embedded newlines and
normalization, and leave non-UTF-8 raw names unchanged. This follows libarchive's
[documented pax encoding](https://github.com/libarchive/libarchive/blob/master/libarchive/tar.5).
The Unicode materialization assertion remains mandatory on every runner.
