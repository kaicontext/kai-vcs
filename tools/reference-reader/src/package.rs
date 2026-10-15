//! Independent package schema validation, separate from production closure checks.
use super::{Value, array, bytes, map, native, uint};
use std::collections::{BTreeMap, BTreeSet};
fn id(v: &Value) -> Result<&[u8], String> {
    native(v)?;
    bytes(&array(v)?[2])
}
fn features(v: &Value) -> Result<BTreeSet<&[u8]>, String> {
    let mut set = BTreeSet::new();
    let mut last = None;
    for f in array(v)? {
        let b = bytes(f)?;
        if b.is_empty() || last.is_some_and(|p| p >= b) {
            return Err("invalid feature list".into());
        }
        last = Some(b);
        set.insert(b);
    }
    Ok(set)
}
pub(super) fn validate(kind: u16, v: &Value) -> Result<(), String> {
    let top = if kind == 16 {
        map(v, &[0, 1, 2, 4], &[3])?
    } else {
        map(v, &[0, 1], &[])?
    };
    if bytes(&top[&0])?.len() != 16 {
        return Err("invalid repository ID".into());
    }
    let m = map(
        &top[&if kind == 16 { 4 } else { 1 }],
        &[0, 1, 2, 3, 4, 5, 6],
        &[],
    )?;
    let mode = uint(&m[&0])?;
    if ![1, 2].contains(&mode) || (mode == 1) != array(&m[&3])?.is_empty() || uint(&m[&6])? != 1 {
        return Err("invalid completeness or external scope".into());
    }
    let mut entries = BTreeMap::new();
    for key in [2, 3] {
        let mut last = None;
        for e in array(&m[&key])? {
            let e = array(e)?;
            if e.len() != 4 {
                return Err("invalid inventory tuple".into());
            }
            let digest = id(&e[0])?;
            let ty = uint(&e[1])?;
            let len = uint(&e[2])?;
            let interpretation = uint(&e[3])?;
            if last.is_some_and(|p| p >= digest)
                || !(1..=65535).contains(&ty)
                || !(23..=67108887).contains(&len)
                || ![1, 2].contains(&interpretation)
                || entries.insert(digest, (ty, interpretation)).is_some()
            {
                return Err("invalid or overlapping inventory".into());
            }
            last = Some(digest);
        }
    }
    let check = |digest: &[u8], ty: u64| -> Result<(), String> {
        if !(1..=19).contains(&ty) || entries.get(digest) != Some(&(ty, 1)) {
            return Err("root absent or mistyped".into());
        }
        Ok(())
    };
    let mut last = None;
    for r in array(&m[&1])? {
        let r = array(r)?;
        if r.len() != 2 {
            return Err("invalid root tuple".into());
        }
        let digest = id(&r[0])?;
        if last.is_some_and(|p| p >= digest) {
            return Err("unordered roots".into());
        }
        last = Some(digest);
        check(digest, uint(&r[1])?)?;
    }
    if kind == 19 && array(&m[&1])?.is_empty() {
        return Err("no transfer roots".into());
    }
    let required = features(&m[&4])?;
    let optional = features(&m[&5])?;
    if !required.is_disjoint(&optional) {
        return Err("feature overlap".into());
    }
    if kind == 16 {
        let generation = uint(&top[&1])?;
        check(id(&top[&2])?, 6)?;
        if let Some(op) = top.get(&3) {
            check(id(op)?, 7)?;
        } else if generation != 0 {
            return Err("missing current operation".into());
        }
    }
    Ok(())
}
