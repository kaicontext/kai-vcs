//! Independent generic-CBOR implementation. Intentionally does not depend on
//! kai-vcs or import its codecs, IDs, constants, schemas, or validation helpers.
use std::collections::BTreeMap;
mod control;
mod git_envelope;
mod metadata;
mod package;
mod recovery;
mod view;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    UInt(u64),
    Negative(u64),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Map(BTreeMap<u64, Value>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    Raw(Vec<u8>),
    Structured(Value),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub kind: u16,
    pub payload: Payload,
}

fn number(out: &mut Vec<u8>, major: u8, n: u64) {
    if n < 24 {
        out.push(major * 32 + n as u8);
        return;
    }
    let width = if n <= 255 {
        1
    } else if n <= 65535 {
        2
    } else if n <= u32::MAX as u64 {
        4
    } else {
        8
    };
    out.push(
        major * 32
            + match width {
                1 => 24,
                2 => 25,
                4 => 26,
                _ => 27,
            },
    );
    out.extend_from_slice(&n.to_be_bytes()[8 - width..]);
}

fn emit(v: &Value, out: &mut Vec<u8>, depth: usize) -> Result<(), String> {
    if depth > 64 || out.len() > 64 * 1024 * 1024 {
        return Err("reference encoder resource limit".into());
    }
    match v {
        Value::UInt(n) => number(out, 0, *n),
        Value::Negative(n) => number(out, 1, *n),
        Value::Bytes(bytes) => {
            number(out, 2, bytes.len() as u64);
            out.extend_from_slice(bytes);
        }
        Value::Array(values) => {
            number(out, 4, values.len() as u64);
            for value in values {
                emit(value, out, depth + 1)?;
            }
        }
        Value::Map(values) => {
            number(out, 5, values.len() as u64);
            for (key, value) in values {
                number(out, 0, *key);
                emit(value, out, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn parse(
    data: &[u8],
    position: &mut usize,
    depth: usize,
    budget: &mut usize,
) -> Result<Value, String> {
    *budget = budget
        .checked_sub(1)
        .ok_or("reference decoder item limit")?;
    if depth > 64 {
        return Err("reference decoder depth limit".into());
    }
    let tag = *data.get(*position).ok_or("truncated CBOR")?;
    *position += 1;
    let info = tag % 32;
    let n = if info < 24 {
        u64::from(info)
    } else {
        let (width, minimum) = match info {
            24 => (1, 24),
            25 => (2, 256),
            26 => (4, 65536),
            27 => (8, 4294967296),
            _ => return Err("invalid CBOR length".into()),
        };
        let end = position.checked_add(width).ok_or("overflow")?;
        let part = data.get(*position..end).ok_or("truncated CBOR number")?;
        *position = end;
        let mut buffer = [0u8; 8];
        buffer[8 - width..].copy_from_slice(part);
        let n = u64::from_be_bytes(buffer);
        if n < minimum {
            return Err("non-minimal CBOR number".into());
        }
        n
    };
    match tag / 32 {
        0 => Ok(Value::UInt(n)),
        1 => Ok(Value::Negative(n)),
        2 => {
            let size = usize::try_from(n).map_err(|_| "byte string overflow")?;
            let end = position.checked_add(size).ok_or("byte string overflow")?;
            let value = data
                .get(*position..end)
                .ok_or("truncated byte string")?
                .to_vec();
            *position = end;
            Ok(Value::Bytes(value))
        }
        4 | 5 => {
            if n > (data.len() - *position) as u64 {
                return Err("impossible collection size".into());
            }
            if tag / 32 == 4 {
                let mut values = Vec::new();
                for _ in 0..n {
                    values.push(parse(data, position, depth + 1, budget)?);
                }
                Ok(Value::Array(values))
            } else {
                let mut values = BTreeMap::new();
                let mut last = None;
                for _ in 0..n {
                    let Value::UInt(key) = parse(data, position, depth + 1, budget)? else {
                        return Err("noninteger map key".into());
                    };
                    if last.is_some_and(|p| key <= p) {
                        return Err("unordered/duplicate map key".into());
                    }
                    last = Some(key);
                    values.insert(key, parse(data, position, depth + 1, budget)?);
                }
                Ok(Value::Map(values))
            }
        }
        _ => Err("CBOR type outside schema profile".into()),
    }
}

fn bytes(v: &Value) -> Result<&[u8], String> {
    if let Value::Bytes(v) = v {
        Ok(v)
    } else {
        Err("expected bytes".into())
    }
}
fn array(v: &Value) -> Result<&[Value], String> {
    if let Value::Array(v) = v {
        Ok(v)
    } else {
        Err("expected array".into())
    }
}
fn uint(v: &Value) -> Result<u64, String> {
    if let Value::UInt(v) = v {
        Ok(*v)
    } else {
        Err("expected unsigned integer".into())
    }
}
fn signed(v: &Value) -> Result<i64, String> {
    match v {
        Value::UInt(n) => i64::try_from(*n).map_err(|_| "integer overflow".into()),
        Value::Negative(n) => i64::try_from(*n)
            .map(|n| !n)
            .map_err(|_| "integer overflow".into()),
        _ => Err("expected integer".into()),
    }
}
fn map<'a>(
    v: &'a Value,
    required: &[u64],
    optional: &[u64],
) -> Result<&'a BTreeMap<u64, Value>, String> {
    let Value::Map(values) = v else {
        return Err("expected map".into());
    };
    if required.iter().any(|key| !values.contains_key(key))
        || values
            .keys()
            .any(|k| !required.contains(k) && !optional.contains(k))
    {
        return Err("missing/unknown schema key".into());
    }
    Ok(values)
}
fn native(v: &Value) -> Result<(), String> {
    let refs = array(v)?;
    if refs.len() != 3
        || uint(&refs[0])? != 1
        || uint(&refs[1])? != 1
        || bytes(&refs[2])?.len() != 32
    {
        return Err("invalid native reference".into());
    }
    Ok(())
}
fn name(v: &[u8]) -> bool {
    !v.is_empty() && v != b"." && v != b".." && !v.contains(&0) && !v.contains(&b'/')
}
fn identity(v: &Value) -> Result<(), String> {
    let m = map(v, &[0, 1, 2, 3], &[])?;
    bytes(&m[&0])?;
    bytes(&m[&1])?;
    signed(&m[&2])?;
    if !(-1439..=1439).contains(&signed(&m[&3])?) {
        return Err("invalid timezone".into());
    }
    Ok(())
}

fn validate(kind: u16, value: &Value) -> Result<(), String> {
    match kind {
        17 | 18 => control::validate(kind, value)?,
        16 | 19 => package::validate(kind, value)?,
        12 => git_envelope::validate(value)?,
        6 => view::validate(value)?,
        9..=11 => metadata::validate(kind, value)?,
        4 | 5 | 7 | 8 | 15 => recovery::validate(kind, value)?,
        2 => {
            let mut previous: Option<&[u8]> = None;
            for entry in array(value)? {
                let m = map(entry, &[0, 1, 2], &[])?;
                let raw_name = bytes(&m[&0])?;
                if !name(raw_name) || previous.is_some_and(|p| p >= raw_name) {
                    return Err("invalid/unsorted tree name".into());
                }
                previous = Some(raw_name);
                match uint(&m[&1])? {
                    1..=4 => native(&m[&2])?,
                    5 => {
                        let r = array(&m[&2])?;
                        if r.len() != 3 || uint(&r[0])? != 0 {
                            return Err("invalid Git reference".into());
                        }
                        let len = match uint(&r[1])? {
                            1 => 20,
                            2 => 32,
                            _ => return Err("invalid Git algorithm".into()),
                        };
                        if bytes(&r[2])?.len() != len {
                            return Err("invalid Git digest".into());
                        }
                    }
                    _ => return Err("unknown tree kind".into()),
                }
            }
        }
        3 => {
            let m = map(value, &[0, 1, 2, 3, 4, 5], &[6, 7, 8])?;
            native(&m[&0])?;
            let parents = array(&m[&1])?;
            let mut seen = std::collections::HashSet::new();
            for p in parents {
                native(p)?;
                if !seen.insert(bytes(&array(p)?[2])?) {
                    return Err("duplicate parent".into());
                }
            }
            if bytes(&m[&2])?.len() != 16 {
                return Err("invalid change ID".into());
            }
            identity(&m[&3])?;
            identity(&m[&4])?;
            bytes(&m[&5])?;
            if let Some(p) = m.get(&6) {
                if array(p)?.is_empty() {
                    return Err("empty optional provenance".into());
                }
                for p in array(p)? {
                    native(p)?;
                }
            }
            if let Some(g) = m.get(&7) {
                native(g)?;
            }
            if let Some(c) = m.get(&8) {
                let paths = array(c)?;
                if paths.is_empty() {
                    return Err("empty optional conflicts".into());
                }
                let mut prev: Option<&[u8]> = None;
                for p in paths {
                    let pair = array(p)?;
                    if pair.len() != 2 {
                        return Err("invalid conflict entry".into());
                    }
                    let path = bytes(&pair[0])?;
                    if !path.split(|b| *b == b'/').all(name) || prev.is_some_and(|p| p >= path) {
                        return Err("invalid conflict path".into());
                    }
                    prev = Some(path);
                    native(&pair[1])?;
                }
            }
        }
        14 => {
            let m = map(value, &[0, 1, 2], &[])?;
            let len = uint(&m[&0])?;
            if len <= 16 * 1024 * 1024 || bytes(&m[&1])?.len() != 32 {
                return Err("invalid large blob length/digest".into());
            }
            let chunks = array(&m[&2])?;
            if chunks.len() as u64 != len.div_ceil(4 * 1024 * 1024) {
                return Err("invalid chunk count".into());
            }
            for c in chunks {
                native(c)?;
            }
        }
        _ => return Err("unsupported structured object type".into()),
    }
    Ok(())
}

pub fn encode(record: &Record) -> Result<Vec<u8>, String> {
    let payload = match &record.payload {
        Payload::Raw(v) => {
            let valid = match record.kind {
                1 => v.len() <= 16 * 1024 * 1024,
                13 => !v.is_empty() && v.len() <= 4 * 1024 * 1024,
                _ => false,
            };
            if !valid {
                return Err("invalid raw type/length".into());
            }
            v.clone()
        }
        Payload::Structured(v) => {
            validate(record.kind, v)?;
            let mut out = Vec::new();
            emit(v, &mut out, 0)?;
            out
        }
    };
    if payload.len() > 64 * 1024 * 1024 {
        return Err("payload exceeds limit".into());
    }
    let mut bytes = b"KAI-OBJECT\0".to_vec();
    bytes.extend_from_slice(&[0, 1]);
    bytes.extend_from_slice(&record.kind.to_be_bytes());
    bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

pub fn decode(data: &[u8]) -> Result<Record, String> {
    if data.len() < 23 || &data[..13] != b"KAI-OBJECT\0\0\x01" {
        return Err("invalid envelope".into());
    }
    let kind = u16::from_be_bytes([data[13], data[14]]);
    let length = u64::from_be_bytes(data[15..23].try_into().map_err(|_| "bad envelope length")?);
    if length > 64 * 1024 * 1024 || length != (data.len() - 23) as u64 {
        return Err("invalid envelope length".into());
    }
    let payload = &data[23..];
    let decoded = match kind {
        1 if length <= 16 * 1024 * 1024 => Payload::Raw(payload.to_vec()),
        13 if (1..=4 * 1024 * 1024).contains(&length) => Payload::Raw(payload.to_vec()),
        2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 14 | 15 | 16 | 17 | 18 | 19 => {
            let mut cursor = 0;
            let value = parse(payload, &mut cursor, 0, &mut 1_000_000)?;
            if cursor != payload.len() {
                return Err("trailing CBOR".into());
            }
            validate(kind, &value)?;
            Payload::Structured(value)
        }
        _ => return Err("unsupported object type/length".into()),
    };
    Ok(Record {
        kind,
        payload: decoded,
    })
}

pub fn id(data: &[u8]) -> Result<String, String> {
    decode(data)?;
    Ok(format!("k1:b3:{}", blake3::hash(data).to_hex()))
}

/// Reconstruct the proposed signing input independently; does not verify a signature.
pub fn attestation_signing_payload(input: &[u8]) -> Result<Vec<u8>, String> {
    let record = decode(input)?;
    if record.kind != 10 {
        return Err("expected attestation".into());
    }
    let Payload::Structured(Value::Map(mut fields)) = record.payload else {
        return Err("expected attestation map".into());
    };
    fields.remove(&8);
    let mut out = b"KAI-ATTESTATION\0".to_vec();
    emit(&Value::Map(fields), &mut out, 0)?;
    Ok(out)
}

pub mod bundle;

mod repository;
