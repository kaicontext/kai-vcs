//! Independent recovery-schema checks over the generic CBOR value tree.
use super::{Value, array, bytes, identity, map, name, native, signed, uint};
use std::collections::BTreeSet;

fn nonempty(v: &Value) -> Result<&[u8], String> {
    let b = bytes(v)?;
    if b.is_empty() {
        return Err("empty bytes identifier".into());
    }
    Ok(b)
}
fn fixed(v: &Value, n: usize) -> Result<&[u8], String> {
    let b = bytes(v)?;
    if b.len() != n {
        return Err("invalid fixed byte length".into());
    }
    Ok(b)
}
fn path(p: &[u8]) -> bool {
    p.split(|b| *b == b'/').all(name)
}
fn refs(v: &Value, unique: bool) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for r in array(v)? {
        native(r)?;
        if unique && !seen.insert(bytes(&array(r)?[2])?) {
            return Err("duplicate reference".into());
        }
    }
    Ok(())
}
fn manifest(v: &Value) -> Result<(), String> {
    let pairs = array(v)?;
    if pairs.is_empty() {
        return Err("empty optional manifest".into());
    }
    let mut last: Option<&[u8]> = None;
    for pair in pairs {
        let pair = array(pair)?;
        if pair.len() != 2 {
            return Err("bad manifest pair".into());
        }
        let p = bytes(&pair[0])?;
        if !path(p) || last.is_some_and(|v| v >= p) {
            return Err("invalid or unsorted manifest path".into());
        }
        last = Some(p);
        native(&pair[1])?;
    }
    Ok(())
}
fn state(v: &Value) -> Result<(), String> {
    let a = array(v)?;
    if a.len() == 1 && uint(&a[0])? == 0 {
        return Ok(());
    }
    if a.len() != 3 || uint(&a[0])? != 1 {
        return Err("bad conflict state".into());
    }
    match uint(&a[1])? {
        1..=4 => native(&a[2]),
        5 => {
            let r = array(&a[2])?;
            if r.len() != 3 || uint(&r[0])? != 0 {
                return Err("bad external reference".into());
            }
            let len = match uint(&r[1])? {
                1 => 20,
                2 => 32,
                _ => return Err("bad Git hash algorithm".into()),
            };
            fixed(&r[2], len)?;
            Ok(())
        }
        _ => Err("bad state kind".into()),
    }
}
pub(super) fn validate(kind: u16, value: &Value) -> Result<(), String> {
    match kind {
        5 => {
            let m = map(value, &[0, 1, 2, 3, 4, 5], &[6, 7])?;
            if !(1..=9).contains(&uint(&m[&0])?) {
                return Err("invalid conflict kind".into());
            }
            let mut paths = Vec::new();
            for p in array(&m[&1])? {
                let p = bytes(p)?;
                if !path(p) || paths.last().is_some_and(|last| *last >= p) {
                    return Err("invalid conflict paths".into());
                }
                paths.push(p);
            }
            if paths.is_empty() || array(&m[&3])?.len() < 2 {
                return Err("empty conflict paths or insufficient sides".into());
            }
            for v in array(&m[&2])?.iter().chain(array(&m[&3])?) {
                let v = map(v, &[0, 1], &[2])?;
                nonempty(&v[&0])?;
                let states = array(&v[&1])?;
                if states.len() != paths.len() {
                    return Err("misaligned conflict states".into());
                }
                for s in states {
                    state(s)?;
                }
                if let Some(r) = v.get(&2) {
                    native(r)?;
                }
            }
            nonempty(&m[&4])?;
            nonempty(&m[&5])?;
            if let Some(candidates) = m.get(&6) {
                let candidates = array(candidates)?;
                if candidates.is_empty() {
                    return Err("empty optional rename candidates".into());
                }
                let mut previous = None;
                for candidate in candidates {
                    let a = array(candidate)?;
                    if a.len() != 3 {
                        return Err("bad rename tuple".into());
                    }
                    let id = fixed(&a[0], 16)?;
                    let src = bytes(&a[1])?;
                    let dst = bytes(&a[2])?;
                    let tuple = (id, src, dst);
                    if previous.is_some_and(|p| p >= tuple)
                        || src == dst
                        || !paths.contains(&src)
                        || !paths.contains(&dst)
                    {
                        return Err("bad rename candidate".into());
                    }
                    previous = Some(tuple);
                }
            }
            if let Some(p) = m.get(&7) {
                if array(p)?.is_empty() {
                    return Err("empty optional provenance".into());
                }
                refs(p, false)?;
            }
        }
        8 => {
            let m = map(value, &[0, 1, 2, 3, 4, 5], &[6, 7, 8, 9, 10])?;
            fixed(&m[&0], 16)?;
            native(&m[&1])?;
            native(&m[&2])?;
            let capture = uint(&m[&3])?;
            if ![1, 2].contains(&capture) {
                return Err("bad capture class".into());
            }
            identity(&m[&4])?;
            let mut previous: Option<&[u8]> = None;
            for inclusion in array(&m[&5])? {
                let pair = array(inclusion)?;
                if pair.len() != 2 {
                    return Err("bad inclusion pair".into());
                }
                let p = bytes(&pair[0])?;
                if !path(p)
                    || previous.is_some_and(|prev| prev >= p)
                    || ![1, 2].contains(&uint(&pair[1])?)
                {
                    return Err("invalid inclusion".into());
                }
                previous = Some(p);
            }
            for key in [6, 8, 9] {
                if let Some(r) = m.get(&key) {
                    native(r)?;
                }
            }
            for key in [7, 10] {
                if let Some(c) = m.get(&key) {
                    manifest(c)?;
                }
            }
            if m.contains_key(&9) && capture != 2 {
                return Err("barrier class mismatch".into());
            }
        }
        7 => {
            let m = map(value, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9], &[10])?;
            fixed(&m[&0], 16)?;
            refs(&m[&1], true)?;
            native(&m[&2])?;
            native(&m[&3])?;
            nonempty(&m[&4])?;
            identity(&m[&5])?;
            signed(&m[&6])?;
            signed(&m[&7])?;
            let mut previous: Option<&[u8]> = None;
            for w in array(&m[&8])? {
                let id = fixed(w, 16)?;
                if previous.is_some_and(|p| p >= id) {
                    return Err("unordered workspaces".into());
                }
                previous = Some(id);
            }
            fixed(&m[&9], 16)?;
            if let Some(receipts) = m.get(&10) {
                if array(receipts)?.is_empty() {
                    return Err("empty optional receipts".into());
                }
                for r in array(receipts)? {
                    nonempty(r)?;
                }
            }
        }
        15 => {
            let m = map(value, &[0, 1, 2], &[3])?;
            native(&m[&0])?;
            bytes(&m[&1])?;
            identity(&m[&2])?;
            if let Some(g) = m.get(&3) {
                native(g)?;
            }
        }
        4 => {
            let m = map(value, &[0, 1, 2, 3, 4, 5], &[6])?;
            let kind = uint(&m[&0])?;
            if !(1..=7).contains(&kind) {
                return Err("bad change kind".into());
            }
            let ids = array(&m[&1])?;
            if ids.is_empty() {
                return Err("empty change IDs".into());
            }
            let mut previous: Option<&[u8]> = None;
            for id in ids {
                let id = fixed(id, 16)?;
                if previous.is_some_and(|p| p >= id) {
                    return Err("unordered change IDs".into());
                }
                previous = Some(id);
            }
            refs(&m[&2], true)?;
            refs(&m[&3], true)?;
            let prior = array(&m[&2])?;
            let result = array(&m[&3])?;
            let valid = match kind {
                1 => prior.is_empty() && !result.is_empty(),
                5 => !prior.is_empty() && result.is_empty(),
                6 => !result.is_empty(),
                _ => !prior.is_empty() && !result.is_empty(),
            };
            if !valid {
                return Err("change cardinality mismatch".into());
            }
            identity(&m[&4])?;
            bytes(&m[&5])?;
            if let Some(op) = m.get(&6) {
                native(op)?;
            }
        }
        _ => return Err("unknown recovery type".into()),
    }
    Ok(())
}
