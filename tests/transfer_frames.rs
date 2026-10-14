use kai_object_reference::transfer::{Limits, Stream};
use kai_vcs::{
    Blob, Object,
    transfer_frame::{FrameLimits, FrameReader, encode_frame},
};
use std::io::{self, Read};
fn object(data: &[u8]) -> Vec<u8> {
    Object::Blob(Blob {
        data: data.to_vec(),
    })
    .encode()
    .unwrap()
}
fn frame(data: &[u8]) -> Vec<u8> {
    encode_frame(&object(data), FrameLimits::default()).unwrap()
}
fn accept(bytes: &[u8]) -> bool {
    let mut r = FrameReader::new(bytes, FrameLimits::default()).unwrap();
    loop {
        match r.next_frame() {
            Ok(Some(_)) => {}
            Ok(None) => return true,
            Err(_) => return false,
        }
    }
}
fn independent(bytes: &[u8]) -> bool {
    let mut r = Stream::new(bytes, Limits::default()).unwrap();
    loop {
        match r.next_object() {
            Ok(Some(_)) => {}
            Ok(None) => return true,
            Err(_) => return false,
        }
    }
}
fn reseal(bytes: &mut [u8]) {
    let mut h = blake3::Hasher::new();
    h.update(b"KAI-FRAME-CHECKSUM\0");
    h.update(&bytes[..63]);
    h.update(&bytes[95..]);
    bytes[63..95].copy_from_slice(h.finalize().as_bytes());
}
#[test]
fn golden_frames_are_identical_in_both_implementations() {
    let mut count = 0;
    for row in include_str!("fixtures/transfer-frames.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let (name, hex) = row.split_once('\t').unwrap();
        let bytes: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let canonical = &bytes[95..];
        assert_eq!(
            encode_frame(canonical, FrameLimits::default()).unwrap(),
            bytes,
            "{name}"
        );
        assert_eq!(
            kai_object_reference::transfer::encode_frame(canonical, Limits::default()).unwrap(),
            bytes,
            "{name}"
        );
        let mut r = FrameReader::new(bytes.as_slice(), FrameLimits::default()).unwrap();
        let decoded = r.next_frame().unwrap().unwrap();
        assert_eq!(decoded.canonical, canonical);
        assert_eq!(decoded.id, kai_vcs::ObjectId::hash(canonical));
        assert_eq!(
            decoded.type_code,
            Object::decode(canonical).unwrap().type_code()
        );
        assert_eq!(r.wire_bytes(), bytes.len() as u64);
        assert!(r.next_frame().unwrap().is_none());
        assert!(independent(&bytes));
        count += 1;
    }
    assert_eq!(count, 4);
}
#[test]
fn rejects_every_truncation_mutation_duplicate_and_trailing_byte() {
    let bytes = frame(b"hello");
    assert!(accept(&[]) && independent(&[]));
    for end in 1..bytes.len() {
        assert!(!accept(&bytes[..end]), "production truncation {end}");
        assert!(!independent(&bytes[..end]), "independent truncation {end}");
    }
    for pos in 0..bytes.len() {
        let mut bad = bytes.clone();
        bad[pos] ^= 1;
        assert!(!accept(&bad), "production mutation {pos}");
        assert!(!independent(&bad), "independent mutation {pos}");
    }
    let duplicate = [bytes.as_slice(), bytes.as_slice()].concat();
    assert!(!accept(&duplicate) && !independent(&duplicate));
    let trailing = [bytes.as_slice(), b"\0"].concat();
    assert!(!accept(&trailing) && !independent(&trailing));
    let second = frame(b"other");
    let valid = [bytes.as_slice(), second.as_slice()].concat();
    assert!(accept(&valid) && independent(&valid));
}
#[test]
fn valid_checksum_cannot_hide_wrong_id_type_or_object_schema() {
    let bytes = frame(b"hello");
    for pos in [31, 13, 95, 108] {
        let mut bad = bytes.clone();
        bad[pos] ^= 1;
        reseal(&mut bad);
        assert!(!accept(&bad) && !independent(&bad), "resealed {pos}");
    }
    // Even a recomputed object digest and checksum cannot authorize an invalid
    // canonical object (type zero inside its envelope).
    let mut bad = bytes;
    bad[108..110].fill(0);
    let hash = blake3::hash(&bad[95..]);
    bad[31..63].copy_from_slice(hash.as_bytes());
    reseal(&mut bad);
    assert!(!accept(&bad) && !independent(&bad));
}
struct HeaderOnly {
    bytes: Vec<u8>,
    pos: usize,
}
impl Read for HeaderOnly {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        assert!(self.pos < 95, "decoder tried to read a rejected payload");
        let n = out.len().min(95 - self.pos);
        out[..n].copy_from_slice(&self.bytes[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}
#[test]
fn hostile_lengths_and_quotas_are_rejected_before_payload_reads() {
    for length in [0, 22, 67_108_888, u64::MAX] {
        let mut bytes = frame(b"hello");
        bytes[14..22].copy_from_slice(&length.to_be_bytes());
        bytes[23..31].copy_from_slice(&length.to_be_bytes());
        let a = HeaderOnly {
            bytes: bytes.clone(),
            pos: 0,
        };
        let b = HeaderOnly { bytes, pos: 0 };
        assert!(
            FrameReader::new(a, FrameLimits::default())
                .unwrap()
                .next_frame()
                .is_err()
        );
        assert!(
            Stream::new(b, Limits::default())
                .unwrap()
                .next_object()
                .is_err()
        );
    }
    let bytes = frame(b"hello");
    for case in 0..3 {
        let mut a = FrameLimits::default();
        let mut b = Limits::default();
        match case {
            0 => {
                a.max_object_bytes = 23;
                b.object = 23;
            }
            1 => {
                a.max_frame_bytes = 118;
                b.frame = 118;
            }
            _ => {
                a.max_stream_bytes = bytes.len() as u64 - 1;
                b.total = a.max_stream_bytes;
            }
        }
        assert!(
            FrameReader::new(
                HeaderOnly {
                    bytes: bytes.clone(),
                    pos: 0
                },
                a
            )
            .unwrap()
            .next_frame()
            .is_err()
        );
        assert!(
            Stream::new(
                HeaderOnly {
                    bytes: bytes.clone(),
                    pos: 0
                },
                b
            )
            .unwrap()
            .next_object()
            .is_err()
        );
    }
}
#[test]
fn aggregate_limits_and_terminal_error_state_are_enforced() {
    let bytes = [frame(b"a"), frame(b"b")].concat();
    for case in 0..2 {
        let mut a = FrameLimits::default();
        let mut b = Limits::default();
        if case == 0 {
            a.max_frames = 1;
            b.count = 1;
        } else {
            a.max_stream_bytes = bytes.len() as u64 - 1;
            b.total = a.max_stream_bytes;
        }
        let mut r = FrameReader::new(bytes.as_slice(), a).unwrap();
        let mut s = Stream::new(bytes.as_slice(), b).unwrap();
        assert!(r.next_frame().unwrap().is_some());
        assert!(s.next_object().unwrap().is_some());
        assert!(r.next_frame().is_err());
        assert!(s.next_object().is_err());
        assert!(r.next_frame().is_err());
        assert!(s.next_object().is_err());
    }
    let single = frame(b"a");
    let mut r = FrameReader::new(
        single.as_slice(),
        FrameLimits {
            max_object_bytes: 24,
            max_frame_bytes: 119,
            max_frames: 1,
            max_stream_bytes: 119,
        },
    )
    .unwrap();
    assert!(r.next_frame().unwrap().is_some());
    assert!(r.next_frame().unwrap().is_none());
    assert!(r.next_frame().is_err());
    for case in 0..6 {
        let mut a = FrameLimits::default();
        let mut b = Limits::default();
        match case {
            0 => {
                a.max_object_bytes = 0;
                b.object = 0;
            }
            1 => {
                a.max_frame_bytes = 0;
                b.frame = 0;
            }
            2 => {
                a.max_frames = 0;
                b.count = 0;
            }
            3 => {
                a.max_stream_bytes = 0;
                b.total = 0;
            }
            4 => {
                a.max_object_bytes = u64::MAX;
                b.object = u64::MAX;
            }
            _ => {
                a.max_frame_bytes = u64::MAX;
                b.frame = u64::MAX;
            }
        }
        assert!(FrameReader::new(&[][..], a).is_err());
        assert!(Stream::new(&[][..], b).is_err());
        assert!(encode_frame(&object(b""), a).is_err());
        assert!(kai_object_reference::transfer::encode_frame(&object(b""), b).is_err());
    }
}
struct ShortReads<R> {
    inner: R,
    interrupt: bool,
}
impl<R: Read> Read for ShortReads<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.interrupt = !self.interrupt;
        if self.interrupt {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = out.len().min(3);
        self.inner.read(&mut out[..n])
    }
}
#[test]
fn streaming_handles_short_reads_and_interrupts() {
    let bytes = frame(b"fragmented stream");
    let mut r = FrameReader::new(
        ShortReads {
            inner: bytes.as_slice(),
            interrupt: false,
        },
        FrameLimits::default(),
    )
    .unwrap();
    let mut s = Stream::new(
        ShortReads {
            inner: bytes.as_slice(),
            interrupt: false,
        },
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        r.next_frame().unwrap().unwrap().canonical,
        s.next_object().unwrap().unwrap()
    );
    assert!(r.next_frame().unwrap().is_none());
    assert!(s.next_object().unwrap().is_none());
}
#[test]
fn negotiated_limits_reject_unsupported_profiles_and_include_header_overhead() {
    use kai_vcs::interface::{Durability, Negotiated, TransferSelection};
    let n = Negotiated {
        protocol_version: 1,
        object_format: "native-v1-blake3".into(),
        compression: "none".into(),
        features: vec![],
        signature_formats: vec![],
        max_object_bytes: 1024,
        max_frame_bytes: 1024,
        selection: TransferSelection::Complete,
        ref_transactions: false,
        receiver_durability: Durability::None,
        replication_policy: None,
    };
    let limits = FrameLimits::negotiated(&n, 10, 1024).unwrap();
    assert!(encode_frame(&object(&vec![0; 906]), limits).is_ok()); // 906 + 23 + 95 = 1024
    assert!(encode_frame(&object(&vec![0; 907]), limits).is_err());
    for case in 0..3 {
        let mut bad = n.clone();
        match case {
            0 => bad.protocol_version = 2,
            1 => bad.object_format = "other".into(),
            _ => bad.compression = "zstd".into(),
        }
        assert!(FrameLimits::negotiated(&bad, 10, 1024).is_err());
    }
}
