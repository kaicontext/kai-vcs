# Native object registry — P0 draft

Contract: `kai-vcs-spec/SPEC.md` §§4.1–4.4 and 4.9. This registry is the first
implemented subset of P0, **not a format freeze or a completed P0 gate**.
The [recovery registry](RECOVERY-FORMAT.md) extends it with change events,
conflicts, operations, checkpoints and annotated tags from §§4.5–4.8.
All names and bytes here are independent of Go. The user's Rust implementation
choice supersedes DELIVERY.md's tentative Go-module choice; behavior is unchanged.

## Envelope and identifiers

`KAI-OBJECT\0 || u16be(1) || u16be(type) || u64be(payload length) || payload`

IDs hash the entire envelope using BLAKE3-256 and display as exactly
`k1:b3:<64 lowercase hex>`. No short IDs, bare hashes, uppercase hex, Unicode
normalization, trailing bytes, or legacy reinterpretation. Existing framed blob
bytes and IDs remain unchanged. The prototype `KV` tree format is rejected.

Native binary references are `[1, 1, digest_bytes]`: native format 1,
BLAKE3 algorithm 1, exactly 32 digest bytes. External Git references are
`[0, algorithm, digest_bytes]`: external namespace 0, SHA-1=1 (20 bytes) or
SHA-256=2 (32 bytes). External Git references only appear in gitlink entries.

Structured objects use deterministic CBOR: definite lengths, shortest integers,
ascending integer map keys, no floats/tags/undefined/duplicate keys. Only declared
fields are allowed. Optional fields are omitted; explicit null is not equivalent
and is rejected. Empty optional collections are omitted (not encoded as empty).
Names/messages/identities are bytes, not text. See `schemas.cddl`.

## Type registry

| Code | Type | Codec status |
| --- | --- | --- |
| 1 | blob | implemented, raw bytes, 0–16 MiB |
| 2 | tree | implemented |
| 3 | revision | implemented |
| 4 | change-event | implemented; recovery registry |
| 5 | conflict | implemented; recovery registry |
| 6 | view | implemented; [view registry](VIEW-FORMAT.md) |
| 7 | operation | implemented; recovery registry |
| 8 | checkpoint | implemented; recovery registry |
| 9 | evidence | implemented; [metadata registry](METADATA-FORMAT.md) |
| 10 | attestation | implemented; [metadata registry](METADATA-FORMAT.md) |
| 11 | analysis-manifest | implemented; [metadata registry](METADATA-FORMAT.md) |
| 12 | git-envelope | implemented; [Git envelope registry](GIT-ENVELOPE-FORMAT.md) |
| 13 | chunk | implemented, raw bytes, 1 byte–4 MiB |
| 14 | large-blob | implemented |
| 15 | tag | implemented; recovery registry |
| 16 | backup-manifest | implemented; [package registry](PACKAGE-FORMAT.md) |
| 17 | collaboration-barrier | implemented; [control registry](CONTROL-FORMAT.md) |
| 18 | policy | implemented; [control registry](CONTROL-FORMAT.md) |
| 19 | transfer-manifest | implemented; [package registry](PACKAGE-FORMAT.md) |

All 19 object types now have draft codecs. Embedded manifest profiles and
capability/outcome contracts remain unfinished; this does not freeze format v1.

## Tree fields

Tree payload is an array sorted by unsigned raw name bytes. Each entry is a map:

| Key | Field | Encoding |
| --- | --- | --- |
| 0 | name | bytes, one path component |
| 1 | kind | regular=1, executable=2, symlink=3, directory=4, gitlink=5 |
| 2 | target | native reference, or external Git reference for gitlink |

Empty names, `.`, `..`, NUL, `/`, and duplicate names are invalid. Non-UTF-8,
case-distinct, and normalization-distinct names remain distinct. Encoding sorts;
decoding refuses unsorted input. A root tree may be empty. A directory pointing
to an empty tree is invalid in closure. Case aliases/reserved OS names may be
stored; a later materializer must preflight them against the target platform.

## Revision fields

| Key | Field | Encoding |
| --- | --- | --- |
| 0 | tree | native reference, required tree closure |
| 1 | parents | ordered native references, required revision closure |
| 2 | change_id | exactly 16 bytes; logical label, no closure edge |
| 3 | author | identity map |
| 4 | committer | identity map |
| 5 | message | opaque bytes |
| 6 | provenance | optional nonempty ordered native references; retained metadata |
| 7 | git_envelope | optional native reference; required when present |
| 8 | conflicts | optional nonempty sorted array of `[path_bytes, native_reference]`; required conflict closure |

Identity maps contain exactly `0: name_bytes`, `1: email_bytes`,
`2: timestamp_seconds` (signed 64-bit), `3: timezone_minutes` (signed, -1439..1439).
Parents retain their order and cannot be duplicated. Ancestry does not depend
on timestamps. Conflict paths consist of nonempty valid name components joined
by `/`; absolute paths, duplicate paths and invalid components are rejected.
Closure verification checks complete conflict bindings, absence of unresolved
paths from ordinary trees, and rejection of shadowing non-directory ancestors.
A syntactically decoded revision alone is not an accepted head.
Required self-links/cycles are invalid. Closure verification rejects
unsupported required targets rather than silently omitting them. Conflict codecs and their
required alternatives are implemented by the recovery registry.

## Large blob fields

| Key | Field | Encoding |
| --- | --- | --- |
| 0 | logical length | unsigned 64-bit, strictly greater than 16 MiB |
| 1 | content digest | 32 bytes, BLAKE3 over raw complete content (not an object ID) |
| 2 | chunks | ordered native chunk references; all required closure |

Chunks are exactly 4 MiB except the last, which is 1–4 MiB. The count must equal
`ceil(logical_length / 4 MiB)`. Full content verification checks every chunk's
framed ID, its position-dependent length, total length, and assembled raw hash.
Repeated chunk IDs are valid. Compression/location never affect identity.

## Bounds, compatibility, and qualification

Current structured payload limit: 64 MiB (implementation bound, not format freeze).
The independent generic reader additionally caps CBOR nesting at 64 and parsed
items at 1,000,000. Its resource refusals are distinct from format invalidity.
Decoders check available bytes before allocating collections and reject lengths
that overflow the host. Source objects beyond declared limits fail explicitly.
Blob/chunk bounds follow the specification. Multi-object file IO remains a later
phase; this crate does not yet claim bounded-memory 10 GiB capture/restore.

There is no authoritative storage, ref mutation, migration, or checkout in this
increment. Codecs produce prepared bytes only; they cannot acknowledge a saved
revision. No linearization point or recovery state is invented for pure encoding.
All future writers must verify canonical bytes, ID and typed closure before
publication, then commit roots under the specified transaction protocol.
