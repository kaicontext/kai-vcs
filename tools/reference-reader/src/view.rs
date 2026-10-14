//! Independent type-6 schema validation over decoded CBOR.
use super::{Value, array, bytes, map, native, uint};
use unicode_normalization::is_nfc;

fn sorted_bytes<'a>(last: &mut Option<&'a [u8]>, key: &'a [u8]) -> Result<(), String> {
    if last.is_some_and(|p| p >= key) {
        return Err("unsorted or duplicate view key".into());
    }
    *last = Some(key);
    Ok(())
}
fn heads(v: &Value, minimum: usize) -> Result<(), String> {
    let a = array(v)?;
    if a.len() < minimum {
        return Err("too few heads".into());
    }
    let mut last = None;
    for r in a {
        native(r)?;
        sorted_bytes(&mut last, bytes(&array(r)?[2])?)?;
    }
    Ok(())
}
fn fixed(v: &Value) -> Result<&[u8], String> {
    let b = bytes(v)?;
    if b.len() != 16 {
        return Err("expected 16 bytes".into());
    }
    Ok(b)
}
pub(super) fn validate(v: &Value) -> Result<(), String> {
    let header = map(v, &[0, 1, 5], &[2, 3, 4])?;
    if uint(&header[&5])? == 1 {
        let m = map(v, &[0, 1, 5], &[])?;
        uint(&m[&0])?;
        let entries = array(&m[&1])?;
        if entries.len() > 256 {
            return Err("too many submaps".into());
        }
        let mut last = None;
        for entry in entries {
            let pair = array(entry)?;
            if pair.len() != 2 {
                return Err("invalid submap pair".into());
            }
            let bucket = uint(&pair[0])?;
            if bucket > 255 || last.is_some_and(|p| p >= bucket) {
                return Err("invalid submap bucket ordering".into());
            }
            native(&pair[1])?;
            last = Some(bucket);
        }
        return Ok(());
    }
    let m = map(v, &[0, 1, 2, 3, 4, 5], &[])?;
    uint(&m[&0])?;
    if uint(&m[&5])? != 0 {
        return Err("unsupported view representation".into());
    }
    let mut last = None;
    for r in array(&m[&1])? {
        let r = array(r)?;
        if r.len() != 3 {
            return Err("invalid ref tuple".into());
        }
        let b = bytes(&r[0])?;
        sorted_bytes(&mut last, b)?;
        let name = std::str::from_utf8(b).map_err(|_| "invalid UTF-8 ref")?;
        if !is_nfc(name) || name.chars().any(|c| c.is_control() || c == '\\') {
            return Err("invalid ref normalization or character".into());
        }
        let parts: Vec<_> = name.split('/').collect();
        if parts.iter().any(|p| matches!(*p, "" | "." | "..")) {
            return Err("invalid ref component".into());
        }
        let local = match parts.as_slice() {
            ["branches", _, ..] => true,
            ["tags" | "shelves", _, ..] | ["remotes", _, "branches", _, ..] => false,
            _ => return Err("invalid ref namespace".into()),
        };
        if uint(&r[1])? == 0 {
            return Err("zero ref generation".into());
        }
        let state = array(&r[2])?;
        if state.is_empty() {
            return Err("empty ref state".into());
        }
        match (uint(&state[0])?, state.len()) {
            (0, 1) => {}
            (1, 2) => native(&state[1])?,
            (2, 2) if local => heads(&state[1], 2)?,
            _ => return Err("invalid ref state".into()),
        }
    }
    let mut last = None;
    for d in array(&m[&2])? {
        let d = array(d)?;
        if d.len() != 2 {
            return Err("invalid draft tuple".into());
        }
        sorted_bytes(&mut last, fixed(&d[0])?)?;
        heads(&d[1], 1)?;
    }
    let mut last = None;
    for w in array(&m[&3])? {
        let w = map(w, &[0], &[1, 2])?;
        sorted_bytes(&mut last, fixed(&w[&0])?)?;
        for key in [1, 2] {
            if let Some(r) = w.get(&key) {
                native(r)?;
            }
        }
    }
    let mut last = None;
    for p in array(&m[&4])? {
        let p = array(p)?;
        if p.len() != 2 {
            return Err("invalid pin tuple".into());
        }
        let kind = uint(&p[0])?;
        if !(1..=19).contains(&kind) {
            return Err("invalid pin type".into());
        }
        native(&p[1])?;
        let key = (kind, bytes(&array(&p[1])?[2])?);
        if last.is_some_and(|p| p >= key) {
            return Err("unsorted or duplicate pins".into());
        }
        last = Some(key);
    }
    Ok(())
}

// Called only on schema-validated records. A normal inline view may fail these
// additional constraints; that matters only when a root uses it as a submap.
pub(super) fn submap_bucket(v: &Value) -> Result<Option<u8>, String> {
    let Value::Map(m) = v else {
        return Err("expected view map".into());
    };
    if uint(&m[&5])? != 0 || uint(&m[&0])? != 0 {
        return Ok(None);
    }
    let mut result = None;
    for section in 1..=4 {
        for entry in array(&m[&section])? {
            let mut h = blake3::Hasher::new();
            h.update(b"KAI-VIEW-BUCKET\0");
            h.update(&[section as u8]);
            match section {
                1 | 2 => {
                    h.update(bytes(&array(entry)?[0])?);
                }
                3 => {
                    h.update(bytes(&map(entry, &[0], &[1, 2])?[&0])?);
                }
                4 => {
                    let p = array(entry)?;
                    h.update(&(uint(&p[0])? as u16).to_be_bytes());
                    h.update(bytes(&array(&p[1])?[2])?);
                }
                _ => unreachable!(),
            }
            let bucket = h.finalize().as_bytes()[0];
            if result.is_some_and(|p| p != bucket) {
                return Ok(None);
            }
            result = Some(bucket);
        }
    }
    Ok(result)
}
