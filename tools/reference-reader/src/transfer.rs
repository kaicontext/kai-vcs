//! Independent implementation of TRANSFER-FRAMING.md, without the kernel crate.
use std::{collections::BTreeSet, io::Read};
const HEADER: usize = 95;
const OBJECT_CAP: u64 = 67_108_887;
#[derive(Clone, Copy)]
pub struct Limits {
    pub object: u64,
    pub frame: u64,
    pub count: usize,
    pub total: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            object: OBJECT_CAP,
            frame: OBJECT_CAP + HEADER as u64,
            count: 100_000,
            total: 1_073_741_824,
        }
    }
}
impl Limits {
    fn check(&self) -> Result<(), String> {
        if !(23..=OBJECT_CAP).contains(&self.object)
            || !(118..=OBJECT_CAP + 95).contains(&self.frame)
            || self.count == 0
            || self.total == 0
        {
            return Err("invalid stream limits".into());
        }
        Ok(())
    }
}
fn sum(header: &[u8], payload: &[u8]) -> blake3::Hash {
    let mut state = blake3::Hasher::new();
    for part in [b"KAI-FRAME-CHECKSUM\0".as_slice(), header, payload] {
        state.update(part);
    }
    state.finalize()
}
pub fn encode_frame(object: &[u8], limits: Limits) -> Result<Vec<u8>, String> {
    limits.check()?;
    if object.len() as u64 > limits.object
        || object.len() as u64 + 95 > limits.frame
        || object.len() as u64 + 95 > limits.total
    {
        return Err("frame exceeds limit".into());
    }
    let record = super::decode(object)?;
    let mut h = [0u8; HEADER];
    h[..12].copy_from_slice(b"KAI-FRAME\0\0\x01");
    h[12..14].copy_from_slice(&record.kind.to_be_bytes());
    h[14..22].copy_from_slice(&(object.len() as u64).to_be_bytes());
    h[23..31].copy_from_slice(&(object.len() as u64).to_be_bytes());
    h[31..63].copy_from_slice(blake3::hash(object).as_bytes());
    let check = sum(&h[..63], object);
    h[63..].copy_from_slice(check.as_bytes());
    Ok([h.as_slice(), object].concat())
}
pub struct Stream<R> {
    reader: R,
    remaining: Limits,
    seen: BTreeSet<[u8; 32]>,
    closed: bool,
}
impl<R: Read> Stream<R> {
    pub fn new(reader: R, limits: Limits) -> Result<Self, String> {
        limits.check()?;
        Ok(Self {
            reader,
            remaining: limits,
            seen: BTreeSet::new(),
            closed: false,
        })
    }
    pub fn next_object(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.closed {
            return Err("stream closed".into());
        }
        let result = self.receive();
        self.closed = !matches!(&result, Ok(Some(_)));
        result
    }
    fn receive(&mut self) -> Result<Option<Vec<u8>>, String> {
        let mut h = [0u8; HEADER];
        loop {
            match self.reader.read(&mut h[..1]) {
                Ok(0) => return Ok(None),
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        if self.remaining.count == 0 || self.remaining.total < 95 {
            return Err("stream quota exceeded".into());
        }
        self.reader
            .read_exact(&mut h[1..])
            .map_err(|e| format!("truncated header: {e}"))?;
        if h[..12] != *b"KAI-FRAME\0\0\x01" || h[22] != 0 {
            return Err("unsupported frame profile".into());
        }
        let kind = u16::from_be_bytes([h[12], h[13]]);
        if !(1..=19).contains(&kind) {
            return Err("unknown frame type".into());
        }
        let n = u64::from_be_bytes(h[14..22].try_into().unwrap());
        if h[14..22] != h[23..31] {
            return Err("length disagreement".into());
        }
        if n < 23 || n > self.remaining.object {
            return Err("object length limit".into());
        }
        let size = n.checked_add(95).ok_or("frame length overflow")?;
        if size > self.remaining.frame || size > self.remaining.total {
            return Err("frame/stream length limit".into());
        }
        let id: [u8; 32] = h[31..63].try_into().unwrap();
        if self.seen.contains(&id) {
            return Err("duplicate object frame".into());
        }
        let mut object = vec![0; usize::try_from(n).map_err(|_| "allocation length")?];
        self.reader
            .read_exact(&mut object)
            .map_err(|e| format!("truncated payload: {e}"))?;
        if sum(&h[..63], &object).as_bytes() != &h[63..95] {
            return Err("frame checksum failed".into());
        }
        if blake3::hash(&object).as_bytes() != &id {
            return Err("object ID failed".into());
        }
        if super::decode(&object)?.kind != kind {
            return Err("object type failed".into());
        }
        self.seen.insert(id);
        self.remaining.count -= 1;
        self.remaining.total -= size;
        Ok(Some(object))
    }
}
