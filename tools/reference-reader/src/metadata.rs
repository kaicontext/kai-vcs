//! Independent metadata validation over generic CBOR values.
use super::{Value, array, bytes, identity, map, name, native, signed, uint};
fn nonempty(v: &Value) -> Result<&[u8], String> {
    let b = bytes(v)?;
    if b.is_empty() {
        return Err("empty metadata bytes".into());
    }
    Ok(b)
}
fn labels(v: &Value, sorted: bool) -> Result<(), String> {
    let mut last = None;
    for v in array(v)? {
        let b = nonempty(v)?;
        if sorted && last.is_some_and(|p| p >= b) {
            return Err("unordered labels".into());
        }
        last = Some(b);
    }
    Ok(())
}
fn manifest(v: &Value) -> Result<(), String> {
    let a = array(v)?;
    if a.len() != 2 {
        return Err("invalid manifest tuple".into());
    }
    nonempty(&a[0])?;
    native(&a[1])
}
fn range(v: &Value, a: u64, b: u64) -> Result<u64, String> {
    let n = uint(v)?;
    if !(a..=b).contains(&n) {
        return Err("unknown metadata enum".into());
    }
    Ok(n)
}
pub(super) fn validate(kind: u16, v: &Value) -> Result<(), String> {
    match kind {
        9 => {
            let m = map(
                v,
                &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
                &[16],
            )?;
            native(&m[&0])?;
            native(&m[&1])?;
            range(&m[&2], 1, 3)?;
            let kind = range(&m[&3], 1, 2)?;
            let args = array(&m[&4])?;
            if args.is_empty() || (kind == 2 && (args.len() != 1 || m.contains_key(&16))) {
                return Err("invalid invocation".into());
            }
            nonempty(&args[0])?;
            for arg in args {
                bytes(arg)?;
            }
            let cwd = bytes(&m[&5])?;
            if !cwd.is_empty() && !cwd.split(|b| *b == b'/').all(name) {
                return Err("invalid cwd".into());
            }
            manifest(&m[&6])?;
            manifest(&m[&7])?;
            identity(&m[&8])?;
            range(&m[&9], 1, 3)?;
            range(&m[&10], 1, 6)?;
            signed(&m[&11])?;
            signed(&m[&12])?;
            let mut last = None;
            for a in array(&m[&13])? {
                let a = array(a)?;
                if a.len() != 2 {
                    return Err("invalid artifact tuple".into());
                }
                let b = nonempty(&a[0])?;
                if last.is_some_and(|p| p >= b) {
                    return Err("unordered artifacts".into());
                }
                last = Some(b);
                native(&a[1])?;
            }
            let scope = map(&m[&14], &[0, 1, 2, 3], &[])?;
            labels(&scope[&0], true)?;
            labels(&scope[&1], true)?;
            bytes(&scope[&2])?;
            bytes(&scope[&3])?;
            labels(&m[&15], false)?;
            if let Some(exit) = m.get(&16) {
                signed(exit)?;
            }
        }
        10 => {
            let m = map(v, &[0, 1, 2, 3, 4, 5, 6, 7, 8], &[9, 10])?;
            for key in [0, 2, 5, 6, 8] {
                nonempty(&m[&key])?;
            }
            if uint(&m[&1])? == 0 {
                return Err("zero signature version".into());
            }
            range(&m[&3], 1, 19)?;
            native(&m[&4])?;
            identity(&m[&7])?;
            let issued = m.get(&9).map(signed).transpose()?;
            let expires = m.get(&10).map(signed).transpose()?;
            if matches!((issued,expires),(Some(a),Some(b)) if b<a) {
                return Err("invalid signed interval".into());
            }
        }
        11 => {
            let m = map(v, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], &[])?;
            native(&m[&0])?;
            for key in [1, 2, 5, 12] {
                nonempty(&m[&key])?;
            }
            for key in [3, 4] {
                let a = array(&m[&key])?;
                if a.len() != 2 || uint(&a[0])? != 1 || bytes(&a[1])?.len() != 32 {
                    return Err("invalid input digest".into());
                }
            }
            let completion = range(&m[&6], 1, 3)?;
            for key in [7, 8, 9, 10] {
                labels(&m[&key], true)?;
            }
            if completion == 1
                && [8, 9, 10]
                    .iter()
                    .any(|key| array(&m[key]).is_ok_and(|a| !a.is_empty()))
            {
                return Err("contradictory complete analysis".into());
            }
            let mut last = None;
            for r in array(&m[&11])? {
                native(r)?;
                let b = bytes(&array(r)?[2])?;
                if last.is_some_and(|p| p >= b) {
                    return Err("unordered results".into());
                }
                last = Some(b);
            }
        }
        _ => return Err("unknown metadata type".into()),
    }
    Ok(())
}
