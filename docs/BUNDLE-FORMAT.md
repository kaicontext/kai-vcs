# Bundle container and independent archive extraction — P0 draft

Implements bounded portable bundles from SPEC §§9.7, 10.6 and R18/R27.
[Repository recovery export](RECOVERY-EXPORT.md) preserves native repository state;
this remains experimental and does not install a live database.

Container bytes, in order:

1. `KAI-BUNDLE\0` (11 bytes), version `u16be(1)`.
2. Manifest framed length `u64be`, then one canonical type-16 backup or type-19 transfer manifest.
3. Each inventory object in manifest digest order: framed length `u64be`, then
   exactly that object's bytes. No count, padding, compression or trailing bytes.

The manifest supplies roots, checksums, lengths, kinds, interpretation and native
closure scope. The reader requires standalone completeness, no prerequisites and
no required feature labels. Interpreted inventory supports all 19 registered types,
including views, checkpoints, conflicts and advisory metadata. All interpreted
objects are checked, including unselected inventory. Parent ancestry, typed edges,
repository cross-object bindings, owner/envelope kinds, chunk boundaries and
complete-content digests are verified. Unknown opaque inventory is hash/frame
checked and preserved, but cannot satisfy required edges or roots. Retained
archival pointers may remain absent according to the native retention contract.

Default limits: 256 MiB whole input including framing, 100,000 objects, one million
source edges/expanded paths, 64 MiB cumulative expanded path bytes, depth 256 and
1 GiB total large-file verification work and logical archive content. These are
reader limits, not changes to object
identity or a claim of large-repository qualification. Input memory is bounded;
large files stream their verified chunks to the archive. No network, Git, hooks,
filters, credentials or original database are used.

`kai-bundle-reader BUNDLE` verifies and lists selected root IDs.
`kai-bundle-reader BUNDLE --archive ID OUTPUT.tar` exports an inventoried tree,
resolved revision/tag or resolved checkpoint working tree. Conflicted revisions
and checkpoints fail source export while remaining fully recoverable as objects.
`--index` and `--recover OUTPUT.tar` expose portable repository state. A source
archive contains one
resolved tree, not history. History remains in the bundle. Output is created
exclusively only after complete verification and export preflight. Existing files
are never overwritten. I/O failure may leave a partial new archive, reported as an
error; this is not a durable transaction or backup acknowledgment.

Archives use POSIX ustar headers, POSIX pax UTF-8 path/linkpath records for
non-ASCII valid UTF-8 values, and GNU long-name/long-link extensions for remaining
long values. Raw path/link bytes are retained without normalization. Files use modes 0644/0755; directories 0755 and
symlinks 0777. uid/gid/mtime are deterministic zero. Symlinks are emitted last, so
no later entry traverses an earlier symlink. Targets may be absolute or dangling;
they are stored without access. Gitlinks refuse archive export because an ordinary
filesystem archive cannot preserve their algorithm-qualified identity. Such
bundles can still be verified. Empty root trees export an empty archive; empty
nested directories are invalid native trees.

Preflight rejects metadata aliases (`.git`, `.kai`), case/normalization collisions,
Windows device/ADS/trailing-dot/space aliases and backslashes conservatively.
Non-UTF-8 POSIX names are preserved. This does not promise every archive extractor
or target filesystem can materialize every valid stored name. Restore using a
capable archive tool into a fresh directory; archive emission itself never writes
source paths or follows source symlinks. Root-relative race-resistant checkout,
Windows emulation and full clean-host T012 qualification remain later work.

## Local restoration evidence

The macOS system tar restored the synthetic sample with exact regular/executable
and binary bytes, modes, nesting and symlink target. The Unix integration test
also restores GNU long names and long symlink targets. Non-UTF-8 archive bytes are
checked on every platform; the actual raw-name restore assertion runs on Linux.
The current Mac filesystem rejected a raw 0xff filename, so no raw-name restore
success is claimed there. CI includes Linux/macOS/Windows, but remote results and
clean-host qualification have not been observed for this increment.
