//! Experimental uncompressed native wire frames; see docs/TRANSFER-FRAMING.md.
use crate::{Object, ObjectId, interface::Negotiated};
use std::{collections::HashSet, io::Read};
pub const HEADER_BYTES: usize = 95;
pub const MAX_OBJECT_BYTES: u64 = 64 * 1024 * 1024 + 23;
const MAGIC: &[u8; 12] = b"KAI-FRAME\0\0\x01";
const DOMAIN: &[u8] = b"KAI-FRAME-CHECKSUM\0";

#[derive(Clone, Copy, Debug)]
pub struct FrameLimits {
    pub max_object_bytes: u64,
    pub max_frame_bytes: u64,
    pub max_frames: usize,
    pub max_stream_bytes: u64,
}
impl Default for FrameLimits {
    fn default() -> Self {
        Self {
            max_object_bytes: MAX_OBJECT_BYTES,
            max_frame_bytes: MAX_OBJECT_BYTES + HEADER_BYTES as u64,
            max_frames: 100_000,
            max_stream_bytes: 1024 * 1024 * 1024,
        }
    }
}
impl FrameLimits {
    /// Bind this implementation to the supported negotiated wire profile. Offers
    /// and negotiation are assertions, not authentication or durability evidence.
    pub fn negotiated(
        session: &Negotiated,
        max_frames: usize,
        max_stream_bytes: u64,
    ) -> Result<Self, String> {
        if session.protocol_version != 1
            || session.object_format != "native-v1-blake3"
            || session.compression != "none"
        {
            return Err("unsupported transfer frame profile".into());
        }
        let limits = Self {
            max_object_bytes: u64::from(session.max_object_bytes).min(MAX_OBJECT_BYTES),
            max_frame_bytes: u64::from(session.max_frame_bytes)
                .min(MAX_OBJECT_BYTES + HEADER_BYTES as u64),
            max_frames,
            max_stream_bytes,
        };
        limits.validate()?;
        Ok(limits)
    }
    fn validate(&self) -> Result<(), String> {
        if self.max_object_bytes < 23
            || self.max_object_bytes > MAX_OBJECT_BYTES
            || self.max_frame_bytes < HEADER_BYTES as u64 + 23
            || self.max_frame_bytes > MAX_OBJECT_BYTES + HEADER_BYTES as u64
            || self.max_frames == 0
            || self.max_stream_bytes == 0
        {
            return Err("invalid transfer frame limits".into());
        }
        Ok(())
    }
    fn frame_size(&self, length: u64) -> Result<u64, String> {
        let total = length
            .checked_add(HEADER_BYTES as u64)
            .ok_or("frame length overflow")?;
        if length < 23 || length > self.max_object_bytes || total > self.max_frame_bytes {
            return Err("transfer frame size limit".into());
        }
        Ok(total)
    }
}
fn checksum(header: &[u8], object: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(DOMAIN);
    h.update(header);
    h.update(object);
    *h.finalize().as_bytes()
}
/// Encode a single validated object. Stream duplicate/count/byte accounting is
/// performed by the receiving FrameReader, not this stateless encoder.
pub fn encode_frame(object: &[u8], limits: FrameLimits) -> Result<Vec<u8>, String> {
    limits.validate()?;
    let length = object.len() as u64;
    let total = limits.frame_size(length)?;
    if total > limits.max_stream_bytes {
        return Err("transfer stream byte limit".into());
    }
    let decoded = Object::decode(object)?;
    let id = ObjectId::hash(object);
    let mut out = Vec::with_capacity(total as usize);
    out.extend(MAGIC);
    out.extend(decoded.type_code().to_be_bytes());
    out.extend(length.to_be_bytes());
    out.push(0);
    out.extend(length.to_be_bytes());
    out.extend(id.digest());
    out.extend(checksum(&out, object));
    out.extend(object);
    Ok(out)
}
#[derive(Debug)]
pub struct ObjectFrame {
    pub id: ObjectId,
    pub type_code: u16,
    pub canonical: Vec<u8>,
}
/// A bounded single-stream verifier. Successful frames still require quarantine
/// and manifest/closure verification before becoming visible repository state.
pub struct FrameReader<R> {
    input: R,
    limits: FrameLimits,
    seen: HashSet<ObjectId>,
    wire_bytes: u64,
    ended: bool,
}
impl<R: Read> FrameReader<R> {
    pub fn new(input: R, limits: FrameLimits) -> Result<Self, String> {
        limits.validate()?;
        Ok(Self {
            input,
            limits,
            seen: HashSet::new(),
            wire_bytes: 0,
            ended: false,
        })
    }
    pub fn wire_bytes(&self) -> u64 {
        self.wire_bytes
    }
    pub fn next_frame(&mut self) -> Result<Option<ObjectFrame>, String> {
        if self.ended {
            return Err("transfer stream is closed".into());
        }
        let result = self.read_next();
        if !matches!(&result, Ok(Some(_))) {
            self.ended = true;
        }
        result
    }
    fn read_next(&mut self) -> Result<Option<ObjectFrame>, String> {
        let mut header = [0u8; HEADER_BYTES];
        loop {
            match self.input.read(&mut header[..1]) {
                Ok(0) => return Ok(None),
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        if self.seen.len() >= self.limits.max_frames {
            return Err("transfer frame count limit".into());
        }
        let after_header = self
            .wire_bytes
            .checked_add(HEADER_BYTES as u64)
            .ok_or("stream length overflow")?;
        if after_header > self.limits.max_stream_bytes {
            return Err("transfer stream byte limit".into());
        }
        self.input
            .read_exact(&mut header[1..])
            .map_err(|e| format!("frame header: {e}"))?;
        if &header[..12] != MAGIC || header[22] != 0 {
            return Err("unsupported frame version or compression".into());
        }
        let kind = u16::from_be_bytes(header[12..14].try_into().unwrap());
        if !(1..=19).contains(&kind) {
            return Err("unsupported frame object type".into());
        }
        let canonical = u64::from_be_bytes(header[14..22].try_into().unwrap());
        let transmitted = u64::from_be_bytes(header[23..31].try_into().unwrap());
        if canonical != transmitted {
            return Err("uncompressed frame length mismatch".into());
        }
        let size = self.limits.frame_size(canonical)?;
        let total = self
            .wire_bytes
            .checked_add(size)
            .ok_or("stream length overflow")?;
        if total > self.limits.max_stream_bytes {
            return Err("transfer stream byte limit".into());
        }
        let id = ObjectId::from_digest(header[31..63].try_into().unwrap());
        if self.seen.contains(&id) {
            return Err("duplicate transfer object".into());
        }
        let mut bytes =
            vec![0; usize::try_from(canonical).map_err(|_| "frame allocation overflow")?];
        self.input
            .read_exact(&mut bytes)
            .map_err(|e| format!("frame payload: {e}"))?;
        if checksum(&header[..63], &bytes) != header[63..95] {
            return Err("transfer frame checksum mismatch".into());
        }
        let decoded = Object::verify(id, &bytes)?;
        if decoded.type_code() != kind {
            return Err("transfer frame object type mismatch".into());
        }
        self.seen.insert(id);
        self.wire_bytes = total;
        Ok(Some(ObjectFrame {
            id,
            type_code: kind,
            canonical: bytes,
        }))
    }
}
