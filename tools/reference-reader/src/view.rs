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
