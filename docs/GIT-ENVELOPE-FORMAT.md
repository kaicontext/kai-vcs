# Git envelopes — P0 draft

Derived from SPEC §§4.8, 10.2–10.4. Type 12 preserves the exact *uncompressed body*
of a Git object plus its declared Git kind and algorithm-qualified ID. It does
not store zlib/pack framing, which is transport-specific and not Git identity.
The native ID hashes the entire canonical Kai frame with BLAKE3 as usual.

Required deterministic-CBOR map fields:

| Key | Value |
| --- | --- |
| 0 | Git kind: blob=1, tree=2, commit=3, tag=4 |
| 1 | External ID: `[0, algorithm, digest]`, SHA-1=1/20 bytes, SHA-256=2/32 bytes |
| 2 | Exact raw body bytes, including all binary headers/messages/signatures |

Verify the external ID over `kind_ascii || SP || decimal_body_length || NUL || body`.
Length is the byte count, in canonical decimal without leading zeroes. Both encode
and decode reject a mismatched ID or malformed algorithm/length. No UTF-8 conversion,
line-ending changes, trailer insertion, signature translation or parent reordering
is permitted. Algorithms remain distinct; conversion is never exact identity.

This initial representation is inline and bounded by the 64 MiB structured-object
limit, including CBOR overhead. Larger envelope storage/streaming remains a P0/P6
extension to be registered before freeze. Every Git kind can be archived, including
internally malformed bodies with a valid hash; hash verification is not a Git
semantic validation result. No native references are inferred from opaque Git
body bytes. Gitlinks and raw Git parent IDs remain in the external namespace.

Revision envelope links must refer to commit envelopes; annotated native tag links
must refer to tag envelopes. Conflicted revisions cannot carry an ordinary imported
Git commit envelope. These closure checks do not prove that native fields match the
Git object's projected tree, parents or identities: the importer must validate those
mappings and raw Git semantics before accepting native history. Collision detection,
SHA-1 collision handling, exact full-repository import/export, symbolic/custom ref
mapping, deterministic change IDs and signature verification remain adapter work.

The independent reader accepts `--git-raw` only for type 12 and writes the exact
verified body to stdout (no status text). `--raw` remains blob/chunk extraction.
This is an object preservation codec and extractor, not an implemented Git importer,
exporter or completed T012/T054–T058 interoperability gate.
