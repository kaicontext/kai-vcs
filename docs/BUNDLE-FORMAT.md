# Source bundle container and independent archive extraction — P0 draft

Implements a bounded first slice of SPEC §§9.7, 10.6 and R18/R27. This is an
experimental source-history bundle, not a full repository backup/restore command.

Container bytes, in order:

1. `KAI-BUNDLE\0` (11 bytes), version `u16be(1)`.
2. Manifest framed length `u64be`, then one canonical type-19 transfer manifest.
3. Each inventory object in manifest digest order: framed length `u64be`, then
   exactly that object's bytes. No count, padding, compression or trailing bytes.

The manifest supplies roots, checksums, lengths, kinds, interpretation and native
closure scope. The reader requires standalone completeness, no prerequisites and
no required feature labels. Interpreted inventory supports blobs, trees, resolved
revisions, Git envelopes, chunks, large blobs and annotated tags (1,2,3,12–15).
Selected roots must be trees, resolved revisions or tags. All interpreted objects
are checked, including unselected inventory. Parent ancestry, typed edges, envelope
owner kinds, chunk boundaries and complete-content digests are verified. Unknown
opaque inventory is hash/frame checked and retained in the bundle, but cannot
satisfy any source edge or root. Unsupported interpreted metadata, nested packages,
conflicted revisions and backup manifests fail explicitly; no claim is made that
all native repository state is independently restorable yet.

Default limits: 256 MiB whole input including framing, 100,000 objects, one million
source edges/expanded paths, 64 MiB cumulative expanded path bytes, depth 256 and
1 GiB total large-file verification work and logical archive content. These are reader limits, not changes to object
identity or a claim of large-repository qualification. Input memory is bounded;
large files stream their verified chunks to the archive. No network, Git, hooks,
filters, credentials or original database are used.

`kai-bundle-reader BUNDLE` verifies and lists selected root IDs.
`kai-bundle-reader BUNDLE --archive ROOT OUTPUT.tar` exports a selected tree,
revision or tag. ROOT must be a declared selected root. The archive contains one
resolved tree, not history. History remains in the bundle. Output is created
exclusively only after complete verification and export preflight. Existing files
are never overwritten. I/O failure may leave a partial new archive, reported as an
error; this is not a durable transaction or backup acknowledgment.

Archives use POSIX ustar headers and GNU long-name/long-link extensions as needed;
raw path/link bytes are retained. Files use modes 0644/0755; directories 0755 and
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
