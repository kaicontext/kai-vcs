//! Narrow deterministic CBOR profile for the checked-in schema registry.
use crate::ObjectId;

pub const BLOB_LIMIT: usize = 16 * 1024 * 1024;
pub const CHUNK_SIZE: usize = 4 * 1024 * 1024;
// Alpha resource bound for structured objects; not a format v1 size guarantee.
pub const STRUCTURED_LIMIT: usize = 64 * 1024 * 1024;
pub const HEADER_LEN: usize = 23;

pub fn frame(kind: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(b"KAI-OBJECT\0");
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn unframe(input: &[u8]) -> Result<(u16, &[u8]), String> {
    if input.len() < HEADER_LEN {
        return Err("input too short".into());
    }
    if &input[..11] != b"KAI-OBJECT\0" {
        return Err("invalid magic".into());
    }
    if input[11..13] != 1u16.to_be_bytes() {
        return Err("invalid version value".into());
    }
    let kind = u16::from_be_bytes([input[13], input[14]]);
    let len = u64::from_be_bytes(input[15..23].try_into().map_err(|_| "invalid length")?);
    if len != (input.len() - HEADER_LEN) as u64 {
        return Err("length mismatch".into());
    }
    let limit = match kind {
        1 => BLOB_LIMIT,
        13 => CHUNK_SIZE,
        _ => STRUCTURED_LIMIT,
    };
    if len > limit as u64 {
        return Err("object payload exceeds resource limit".into());
    }
    Ok((kind, &input[HEADER_LEN..]))
}

pub fn head(out: &mut Vec<u8>, major: u8, value: u64) {
    let prefix = major << 5;
    match value {
        0..=23 => out.push(prefix | value as u8),
        24..=255 => out.extend_from_slice(&[prefix | 24, value as u8]),
        256..=65535 => {
            out.push(prefix | 25);
            out.extend_from_slice(&(value as u16).to_be_bytes());
        }
        65536..=4294967295 => {
            out.push(prefix | 26);
            out.extend_from_slice(&(value as u32).to_be_bytes());
        }
        _ => {
            out.push(prefix | 27);
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
}

pub fn signed(out: &mut Vec<u8>, value: i64) {
    if value >= 0 {
        head(out, 0, value as u64);
    } else {
        head(out, 1, (!value) as u64);
    }
}

pub fn bytes(out: &mut Vec<u8>, value: &[u8]) {
    head(out, 2, value.len() as u64);
    out.extend_from_slice(value);
}

pub fn reference(out: &mut Vec<u8>, id: ObjectId) {
    head(out, 4, 3);
    head(out, 0, 1);
    head(out, 0, 1);
    bytes(out, id.digest());
}

pub fn bounded(payload: Vec<u8>, kind: u16) -> Result<Vec<u8>, String> {
    if payload.len() > STRUCTURED_LIMIT {
        return Err("structured object exceeds resource limit".into());
    }
    Ok(frame(kind, &payload))
}

pub struct Decoder<'a> {
    rest: &'a [u8],
}
impl<'a> Decoder<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { rest: input }
    }
    pub fn remaining(&self) -> usize {
        self.rest.len()
    }
    pub fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        if len > self.rest.len() {
            return Err("truncated CBOR".into());
        }
        let (value, rest) = self.rest.split_at(len);
        self.rest = rest;
        Ok(value)
    }
    pub fn head(&mut self, major: u8) -> Result<u64, String> {
        let byte = self.take(1)?[0];
        if byte >> 5 != major {
            return Err("unexpected CBOR type".into());
        }
        let info = byte & 31;
        let (len, min) = match info {
            0..=23 => return Ok(info as u64),
            24 => (1, 24),
            25 => (2, 256),
            26 => (4, 65536),
            27 => (8, 4294967296),
            _ => return Err("indefinite or reserved CBOR encoding".into()),
        };
        let mut value = 0u64;
        for byte in self.take(len)? {
            value = (value << 8) | *byte as u64;
        }
        if value < min {
            return Err("noncanonical CBOR integer/length".into());
        }
        Ok(value)
    }
    pub fn expect(&mut self, major: u8, value: u64) -> Result<(), String> {
        if self.head(major)? != value {
            return Err("unexpected CBOR field/value".into());
        }
        Ok(())
    }
    pub fn bytes(&mut self) -> Result<&'a [u8], String> {
        let len = usize::try_from(self.head(2)?).map_err(|_| "CBOR length overflow")?;
        self.take(len)
    }
    pub fn count(&mut self, major: u8, minimum_size: usize) -> Result<usize, String> {
        let count = usize::try_from(self.head(major)?).map_err(|_| "CBOR count overflow")?;
        if count > self.remaining() / minimum_size {
            return Err("impossible CBOR item count".into());
        }
        Ok(count)
    }
    pub fn signed(&mut self) -> Result<i64, String> {
        let major = self.rest.first().ok_or("truncated CBOR integer")? >> 5;
        if major > 1 {
            return Err("expected signed integer".into());
        }
        let magnitude = i64::try_from(self.head(major)?).map_err(|_| "signed integer overflow")?;
        Ok(if major == 0 { magnitude } else { !magnitude })
    }
    pub fn reference(&mut self) -> Result<ObjectId, String> {
        self.expect(4, 3)?;
        self.expect(0, 1)?;
        self.expect(0, 1)?;
        Ok(ObjectId::from_digest(
            self.bytes()?
                .try_into()
                .map_err(|_| "invalid native digest length")?,
        ))
    }
    pub fn finish(self) -> Result<(), String> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err("trailing CBOR bytes".into())
        }
    }
}
