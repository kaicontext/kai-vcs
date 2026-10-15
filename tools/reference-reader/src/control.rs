use super::{Value, array, bytes, map, native, uint};
use unicode_normalization::is_nfc;
fn nonempty(v: &Value) -> Result<&[u8], String> {
    let b = bytes(v)?;
    if b.is_empty() {
        return Err("empty control label".into());
    }
    Ok(b)
}
fn fixed(v: &Value) -> Result<&[u8], String> {
    let b = bytes(v)?;
    if b.len() != 16 {
        return Err("invalid fixed ID".into());
    }
    Ok(b)
}
fn labels(v: &Value) -> Result<(), String> {
    let mut last = None;
    for b in array(v)? {
        let b = nonempty(b)?;
        if last.is_some_and(|p| p >= b) {
            return Err("unordered control labels".into());
        }
        last = Some(b);
    }
    Ok(())
}
pub(super) fn validate(kind: u16, v: &Value) -> Result<(), String> {
    if kind == 17 {
        let m = map(v, &[0, 1, 2, 3, 4, 5, 6, 7, 8], &[])?;
        fixed(&m[&0])?;
        fixed(&m[&1])?;
        uint(&m[&2])?;
        native(&m[&3])?;
        nonempty(&m[&4])?;
        native(&m[&7])?;
        nonempty(&m[&8])?;
        let mut included = std::collections::BTreeSet::new();
        let mut last = None;
        for r in array(&m[&5])? {
            let r = array(r)?;
            if r.len() != 2 {
                return Err("invalid frontier pair".into());
            }
            let id = fixed(&r[0])?;
            uint(&r[1])?;
            if last.is_some_and(|p| p >= id) {
                return Err("unordered replicas".into());
            }
            last = Some(id);
            included.insert(id);
        }
        if included.is_empty() {
            return Err("empty frontier".into());
        }
        let mut last = None;
        for r in array(&m[&6])? {
            let r = array(r)?;
            if r.len() != 2 {
                return Err("invalid exclusion pair".into());
            }
            let id = fixed(&r[0])?;
            nonempty(&r[1])?;
            if last.is_some_and(|p| p >= id) || included.contains(id) {
                return Err("unordered or overlapping replicas".into());
            }
            last = Some(id);
        }
    } else {
        let m = map(v, &[0, 1, 2, 3, 4, 5, 6, 7, 8], &[9])?;
        fixed(&m[&0])?;
        if uint(&m[&1])? == 0 {
            return Err("zero policy revision".into());
        }
        for key in [2, 3, 5, 8] {
            labels(&m[&key])?;
        }
        for r in array(&m[&2])? {
            let name = std::str::from_utf8(bytes(r)?).map_err(|_| "invalid UTF-8 ref")?;
            let parts: Vec<_> = name.split('/').collect();
            if !is_nfc(name)
                || name.chars().any(|c| c.is_control() || c == '\\')
                || parts.iter().any(|p| matches!(*p, "" | "." | ".."))
                || !matches!(parts.as_slice(), ["branches" | "tags", _, ..])
            {
                return Err("invalid policy ref".into());
            }
        }
        uint(&m[&4])?;
        if ![1, 2, 3].contains(&uint(&m[&6])?) || ![1, 2].contains(&uint(&m[&7])?) {
            return Err("invalid policy mode".into());
        }
        if let Some(p) = m.get(&9) {
            native(p)?;
        }
    }
    Ok(())
}
