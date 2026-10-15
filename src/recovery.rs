//! Shared encoding helpers for the recovery schema registry.
use crate::{
    ObjectId,
    codec::{self, Decoder},
    tree::valid_name,
};
use std::collections::HashSet;

pub type ConflictManifest = Vec<(Vec<u8>, ObjectId)>;

pub(crate) fn path(path: &[u8]) -> bool {
    path.split(|b| *b == b'/').all(valid_name)
}
pub(crate) fn ordered<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|w| w[0] < w[1])
}
pub(crate) fn unique_refs(ids: &[ObjectId]) -> Result<(), String> {
    let mut seen = HashSet::new();
    if ids.iter().any(|id| !seen.insert(id)) {
        return Err("duplicate native reference".into());
    }
    Ok(())
}
pub(crate) fn field(out: &mut Vec<u8>, key: u64) {
    codec::head(out, 0, key);
}
pub(crate) fn fixed<const N: usize>(d: &mut Decoder<'_>) -> Result<[u8; N], String> {
    d.bytes()?
        .try_into()
        .map_err(|_| format!("expected {N} bytes"))
}
pub(crate) fn write_refs(out: &mut Vec<u8>, ids: &[ObjectId]) {
    codec::head(out, 4, ids.len() as u64);
    for id in ids {
        codec::reference(out, *id);
    }
}
pub(crate) fn read_refs(d: &mut Decoder<'_>) -> Result<Vec<ObjectId>, String> {
    let count = d.count(4, 37)?;
    (0..count).map(|_| d.reference()).collect()
}
pub(crate) fn manifest_valid(manifest: &ConflictManifest) -> Result<(), String> {
    if manifest.iter().any(|(p, _)| !path(p)) || !manifest.windows(2).all(|w| w[0].0 < w[1].0) {
        return Err("invalid or unordered conflict manifest".into());
    }
    Ok(())
}
pub(crate) fn write_manifest(out: &mut Vec<u8>, manifest: &ConflictManifest) {
    codec::head(out, 4, manifest.len() as u64);
    for (path, id) in manifest {
        codec::head(out, 4, 2);
        codec::bytes(out, path);
        codec::reference(out, *id);
    }
}
pub(crate) fn read_manifest(d: &mut Decoder<'_>) -> Result<ConflictManifest, String> {
    let count = d.count(4, 40)?;
    let mut manifest = Vec::new();
    for _ in 0..count {
        d.expect(4, 2)?;
        manifest.push((d.bytes()?.to_vec(), d.reference()?));
    }
    if manifest.is_empty() {
        return Err("empty optional conflict manifest".into());
    }
    manifest_valid(&manifest)?;
    Ok(manifest)
}
pub(crate) fn decoder(
    input: &[u8],
    kind: u16,
    min: u64,
    max: u64,
) -> Result<(Decoder<'_>, u64), String> {
    let (actual, payload) = codec::unframe(input)?;
    if actual != kind {
        return Err(format!("expected object type {kind}"));
    }
    let mut d = Decoder::new(payload);
    let count = d.head(5)?;
    if !(min..=max).contains(&count) {
        return Err("invalid object field count".into());
    }
    Ok((d, count))
}
pub(crate) fn next_key(d: &mut Decoder<'_>, previous: &mut u64) -> Result<u64, String> {
    let key = d.head(0)?;
    if key <= *previous {
        return Err("unordered or duplicate field".into());
    }
    *previous = key;
    Ok(key)
}
pub(crate) fn nonempty(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("empty required identifier".into());
    }
    Ok(())
}
pub(crate) fn input_bound(size: usize) -> Result<(), String> {
    if size > codec::STRUCTURED_LIMIT {
        return Err("structured object exceeds resource limit".into());
    }
    Ok(())
}
