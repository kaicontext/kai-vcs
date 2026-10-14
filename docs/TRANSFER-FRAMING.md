# Native object transfer frames — P0 draft

Derived from SPEC §9.2. This registry defines version-1 object frames before
implementation. It is separate from the immutable object envelope and from
backup containers. HTTP and SSH adapters must carry the same bytes. This is an
experimental, uncompressed profile; authentication, session/request framing,
resume tokens, quarantine and publication remain unimplemented.

## Layout

Every frame has a 95-byte header followed by one complete canonical object
(including its existing 23-byte object envelope). Integers are unsigned big-endian.

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 12 | Literal `KAI-FRAME` + two zero bytes + version byte 1 |
| 12 | 2 | Native object type |
| 14 | 8 | Canonical object byte length |
| 22 | 1 | Compression identifier: 0 means none |
| 23 | 8 | Transmitted payload length |
| 31 | 32 | Native BLAKE3 object digest |
| 63 | 32 | Frame checksum |
| 95 | transmitted length | Complete canonical object bytes |

The checksum is BLAKE3 over ASCII `KAI-FRAME-CHECKSUM`, one zero byte, header
bytes 0–62, then the transmitted payload. It binds type, lengths, version,
compression and object digest. It detects corruption, not malicious authorship;
an authenticated transport remains required. The object digest is independently
verified over the canonical object bytes, and both declared types must agree.

Only compression 0 is implemented, and both lengths must match. Unknown versions,
compression, or object types fail closed. Canonical decoding validates the object
schema; a frame does not prove its referenced closure is present. Transfer
verification must separately validate a manifest and required closure before any
promotion or ref update. Receiving bytes never makes them authoritative.

## Limits and stream behavior

Object limits count the entire canonical object envelope. Frame limits count the
entire header and payload. The local hard cap is 64 MiB + 23 bytes per canonical
object and 95 additional bytes per frame. Object-specific limits still apply.
Callers use the negotiated minima, bounded by these caps; advertising a frame
limit equal to the object limit leaves 95 fewer usable bytes for objects.
The receiver also requires explicit maximum frame count and total wire bytes.
Zero limits are invalid. Length arithmetic is checked before allocation or reads.

A stream concatenates zero or more frames. EOF before the next header is clean;
EOF within a header or payload is truncation. There is no padding or trailing
whitespace. Repeated object IDs, even byte-identical duplicates, are rejected
within one stream. Deduplication across separate streams belongs to the transfer
session and verified object store, not this decoder. Resume does not reset session
quotas merely because this low-level decoder opens another stream.

The streaming decoder reads at most one bounded payload at a time, retains only
a bounded set of seen IDs, and stops permanently after any error. It counts every
header and payload against the total wire budget. It does not scan for a new magic
after corruption. Transport adapters must enforce timeouts/cancellation and total
session quotas; the synchronous reader cannot interrupt a blocked underlying read.

The independent reader implements these rules without depending on the production
crate. Golden frame vectors are generated from its independent encoder. Object
format vectors continue to define canonical object bytes and IDs.

## Try the standalone verifier

Choose an unused output filename. The example emits a submap-backed repository's
objects, and the independent executable validates every frame before reporting
success. It does not create a repository or publish refs.

```sh
cargo run --locked --offline --example transfer_stream -- /tmp/kai-transfer.kai-frames
cargo run --locked --offline -p kai-object-reference --bin kai-transfer-reader -- /tmp/kai-transfer.kai-frames
```

`transfer_frame::FrameLimits::negotiated` accepts protocol 1,
`native-v1-blake3`, and compression `none`, applying negotiated size minima and
explicit caller-supplied stream quotas. Other selections fail before reading.
The format codec alone does not authorize unknown negotiated feature semantics;
a future session adapter must enforce every negotiated required feature before
using this primitive. An empty stream is valid framing, not a complete transfer.

Tests cover independently reproduced vectors, every truncation and single-byte
corruption of a sample frame, rechecksummed incorrect hashes/types, malformed
objects, short/interrupted reads, duplicate objects, hostile length declarations,
negotiated limits, cumulative quotas and permanent failure state. The CLI is also
run with empty PATH and isolated HOME, and must print no success for a bad suffix.
