//! Independent, bounded source-bundle verification. No production crate dependency.
use super::{Payload, Record, Value, array, bytes, decode, map, native, uint};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use unicode_normalization::UnicodeNormalization;
const INPUT_LIMIT: u64 = 256 * 1024 * 1024;
const EDGE_LIMIT: usize = 1_000_000;
pub(super) type Digest = [u8; 32];
pub(super) fn digest(v: &Value) -> Result<Digest, String> {
    native(v)?;
    bytes(&array(v)?[2])?
        .try_into()
        .map_err(|_| "invalid digest".into())
}
pub(super) fn fields(r: &Record) -> Result<&BTreeMap<u64, Value>, String> {
    match &r.payload {
        Payload::Structured(Value::Map(m)) => Ok(m),
        _ => Err("expected map".into()),
    }
}
fn take<'a>(input: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
    if input.len() < n {
        return Err("truncated bundle".into());
    }
    let (a, b) = input.split_at(n);
    *input = b;
    Ok(a)
}
fn framed<'a>(input: &mut &'a [u8]) -> Result<&'a [u8], String> {
    let n = u64::from_be_bytes(take(input, 8)?.try_into().unwrap());
    if !(23..=67108887).contains(&n) {
        return Err("bundle object length limit".into());
    }
    take(input, n as usize)
}
fn envelope(b: &[u8], ty: u16) -> Result<(), String> {
    if b.len() < 23
        || &b[..13] != b"KAI-OBJECT\0\0\x01"
        || u16::from_be_bytes([b[13], b[14]]) != ty
        || u64::from_be_bytes(b[15..23].try_into().unwrap()) != (b.len() - 23) as u64
    {
        return Err("inventory frame mismatch".into());
    }
    Ok(())
}
pub struct Bundle {
    pub(super) records: BTreeMap<Digest, Record>,
    pub(super) objects: BTreeMap<Digest, Vec<u8>>,
    pub(super) manifest: Record,
    pub(super) manifest_bytes: Vec<u8>,
    roots: BTreeMap<Digest, u16>,
}
impl Bundle {
    pub fn read(reader: impl Read) -> Result<Self, String> {
        let mut input = Vec::new();
        reader
            .take(INPUT_LIMIT + 1)
            .read_to_end(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() as u64 > INPUT_LIMIT {
            return Err("bundle input limit exceeded".into());
        }
        let mut cursor = input.as_slice();
        if take(&mut cursor, 13)? != b"KAI-BUNDLE\0\0\x01" {
            return Err("invalid bundle signature/version".into());
        }
        let manifest_bytes = framed(&mut cursor)?.to_vec();
        let manifest = decode(&manifest_bytes)?;
        if !matches!(manifest.kind, 16 | 19) {
            return Err("expected backup or transfer manifest".into());
        }
        let key = if manifest.kind == 16 { 4 } else { 1 };
        let m = map(&fields(&manifest)?[&key], &[0, 1, 2, 3, 4, 5, 6], &[])?;
        if uint(&m[&0])? != 1 || !array(&m[&3])?.is_empty() {
            return Err("incremental bundle is not standalone".into());
        }
        if !array(&m[&4])?.is_empty() {
            return Err("unsupported required feature".into());
        }
        let inventory = array(&m[&2])?;
        if inventory.len() > 100_000 {
            return Err("bundle object count limit".into());
        }
        let mut records = BTreeMap::new();
        let mut objects = BTreeMap::new();
        for entry in inventory {
            let e = array(entry)?;
            let id = digest(&e[0])?;
            let ty = uint(&e[1])? as u16;
            let b = framed(&mut cursor)?;
            if b.len() as u64 != uint(&e[2])? || blake3::hash(b).as_bytes() != &id {
                return Err("inventory size/checksum mismatch".into());
            }
            envelope(b, ty)?;
            if uint(&e[3])? == 1 {
                records.insert(id, decode(b)?);
            }
            objects.insert(id, b.to_vec());
        }
        if !cursor.is_empty() {
            return Err("trailing bundle bytes".into());
        }
        let mut roots = BTreeMap::new();
        for r in array(&m[&1])? {
            let r = array(r)?;
            let ty = uint(&r[1])? as u16;
            roots.insert(digest(&r[0])?, ty);
        }
        if manifest.kind == 16 {
            let top = fields(&manifest)?;
            roots.insert(digest(&top[&2])?, 6);
            if let Some(op) = top.get(&3) {
                roots.insert(digest(op)?, 7);
            }
        }
        let bundle = Self {
            records,
            roots,
            objects,
            manifest,
            manifest_bytes,
        };
        bundle.verify()?;
        Ok(bundle)
    }
    pub(super) fn get(&self, id: Digest, kinds: &[u16]) -> Result<&Record, String> {
        let r = self
            .records
            .get(&id)
            .ok_or("missing interpreted source closure")?;
        if !kinds.contains(&r.kind) {
            return Err("source closure type mismatch".into());
        }
        Ok(r)
    }
    fn edges(&self, r: &Record) -> Result<Vec<(Digest, Vec<u16>)>, String> {
        let mut edges = Vec::new();
        let mut add = |v: &Value, kinds: &[u16]| -> Result<(), String> {
            edges.push((digest(v)?, kinds.to_vec()));
            Ok(())
        };
        match r.kind {
            2 => {
                let Payload::Structured(v) = &r.payload else {
                    unreachable!()
                };
                for e in array(v)? {
                    let m = map(e, &[0, 1, 2], &[])?;
                    let kinds: &[u16] = match uint(&m[&1])? {
                        1 | 2 => &[1, 14],
                        3 => &[1],
                        4 => &[2],
                        5 => continue,
                        _ => unreachable!(),
                    };
                    add(&m[&2], kinds)?;
                    if kinds == [2] {
                        let child = self.get(digest(&m[&2])?, &[2])?;
                        if matches!(&child.payload,Payload::Structured(Value::Array(v)) if v.is_empty())
                        {
                            return Err("empty nested tree".into());
                        }
                    }
                }
            }
            3 => {
                let m = fields(r)?;
                if m.contains_key(&8) && m.contains_key(&7) {
                    return Err("conflicted revision cannot carry Git envelope".into());
                }
                if let Some(v) = m.get(&8) {
                    for p in array(v)? {
                        add(&array(p)?[1], &[5])?;
                    }
                }
                add(&m[&0], &[2])?;
                for p in array(&m[&1])? {
                    add(p, &[3])?;
                }
                if let Some(g) = m.get(&7) {
                    add(g, &[12])?;
                    self.git_kind(g, 3)?;
                }
            }
            15 => {
                let m = fields(r)?;
                add(&m[&0], &[3])?;
                if let Some(g) = m.get(&3) {
                    add(g, &[12])?;
                    self.git_kind(g, 4)?;
                }
            }
            14 => {
                for c in array(&fields(r)?[&2])? {
                    add(c, &[13])?;
                }
            }
            _ => edges.extend(self.repository_edges(r)?),
        }
        Ok(edges)
    }
    fn git_kind(&self, g: &Value, expected: u64) -> Result<(), String> {
        if uint(&fields(self.get(digest(g)?, &[12])?)?[&0])? != expected {
            return Err("Git envelope owner kind mismatch".into());
        }
        Ok(())
    }
    fn verify(&self) -> Result<(), String> {
        for (id, ty) in &self.roots {
            self.get(*id, &[*ty])?;
        }
        let mut done = BTreeSet::new();
        let mut active = BTreeSet::new();
        let mut work = 0usize;
        let mut logical = 0u64;
        for id in self.records.keys() {
            let mut stack = vec![(*id, false)];
            while let Some((id, leave)) = stack.pop() {
                if leave {
                    active.remove(&id);
                    done.insert(id);
                    continue;
                }
                if done.contains(&id) {
                    continue;
                }
                if !active.insert(id) {
                    return Err("source closure cycle".into());
                }
                stack.push((id, true));
                let r = &self.records[&id];
                if r.kind == 14 {
                    logical = logical
                        .checked_add(uint(&fields(r)?[&0])?)
                        .ok_or("logical size overflow")?;
                    if logical > 1024 * 1024 * 1024 {
                        return Err("bundle logical verification limit".into());
                    }
                    self.file_bytes(id, &mut std::io::sink())?;
                }
                for (child, kinds) in self.edges(r)? {
                    work = work.checked_add(1).ok_or("edge overflow")?;
                    if work > EDGE_LIMIT {
                        return Err("bundle edge limit".into());
                    }
                    self.get(child, &kinds)?;
                    stack.push((child, false));
                }
            }
        }
        self.verify_repository_bindings(&mut work)?;
        Ok(())
    }
    pub fn roots(&self) -> Vec<String> {
        self.roots
            .keys()
            .map(|id| format!("k1:b3:{}", hex(id)))
            .collect()
    }
    fn file_bytes(&self, id: Digest, out: &mut impl Write) -> Result<u64, String> {
        let r = self.get(id, &[1, 14])?;
        if let Payload::Raw(b) = &r.payload {
            out.write_all(b).map_err(|e| e.to_string())?;
            return Ok(b.len() as u64);
        }
        let m = fields(r)?;
        let length = uint(&m[&0])?;
        let mut remaining = length;
        let mut hash = blake3::Hasher::new();
        for c in array(&m[&2])? {
            let Payload::Raw(b) = &self.get(digest(c)?, &[13])?.payload else {
                unreachable!()
            };
            if b.len() as u64 != remaining.min(4 * 1024 * 1024) {
                return Err("invalid chunk boundary".into());
            }
            hash.update(b);
            out.write_all(b).map_err(|e| e.to_string())?;
            remaining -= b.len() as u64;
        }
        if remaining != 0 || hash.finalize().as_bytes() != bytes(&m[&1])? {
            return Err("large file digest mismatch".into());
        }
        Ok(length)
    }
}
pub(super) fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
struct ExportEntry {
    path: Vec<u8>,
    kind: u64,
    id: Digest,
    size: u64,
    link: Vec<u8>,
}
/// A fully verified and preflighted export. No filesystem source paths are opened.
pub struct Archive<'a> {
    bundle: &'a Bundle,
    entries: Vec<ExportEntry>,
}
fn alias(name: &[u8]) -> Result<Vec<u8>, String> {
    if name.contains(&b'\\') || name.contains(&b':') || name.ends_with(b".") || name.ends_with(b" ")
    {
        return Err("unsafe platform path alias".into());
    }
    let folded = match std::str::from_utf8(name) {
        Ok(s) => s
            .nfc()
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .into_bytes(),
        Err(_) => name.iter().map(u8::to_ascii_lowercase).collect(),
    };
    let stem = folded.split(|b| *b == b'.').next().unwrap_or_default();
    if folded == b".git"
        || folded == b".kai"
        || [b"con".as_slice(), b"prn", b"aux", b"nul"].contains(&stem)
        || (stem.len() == 4
            && (&stem[..3] == b"com" || &stem[..3] == b"lpt")
            && (b'1'..=b'9').contains(&stem[3]))
    {
        return Err("protected or reserved path alias".into());
    }
    Ok(folded)
}
impl Bundle {
    pub fn archive(&self, root: &str) -> Result<Archive<'_>, String> {
        let id = *self
            .records
            .keys()
            .find(|id| format!("k1:b3:{}", hex(*id)) == root)
            .ok_or("snapshot is not an interpreted inventory object")?;
        let mut r = &self.records[&id];
        if r.kind == 15 {
            r = self.get(digest(&fields(r)?[&0])?, &[3])?;
        }
        let tree = match r.kind {
            3 => {
                let m = fields(r)?;
                if m.contains_key(&8) {
                    return Err("unresolved revision cannot be source-exported".into());
                }
                digest(&m[&0])?
            }
            8 => {
                let m = fields(r)?;
                if m.contains_key(&7) || m.contains_key(&10) {
                    return Err("unresolved checkpoint cannot be source-exported".into());
                }
                digest(&m[&1])?
            }
            2 => id,
            _ => return Err("archive requires tree, revision, tag or checkpoint".into()),
        };
        let mut stack = vec![(tree, Vec::<u8>::new(), 0usize)];
        let mut entries = Vec::new();
        let mut path_bytes = 0usize;
        let mut content = 0u64;
        while let Some((id, prefix, depth)) = stack.pop() {
            if depth > 256 {
                return Err("archive depth limit".into());
            }
            let Payload::Structured(v) = &self.get(id, &[2])?.payload else {
                unreachable!()
            };
            let mut aliases = BTreeSet::new();
            for entry in array(v)? {
                let m = map(entry, &[0, 1, 2], &[])?;
                let name = bytes(&m[&0])?;
                if !aliases.insert(alias(name)?) {
                    return Err("case/normalization path collision".into());
                }
                let mut path = prefix.clone();
                if !path.is_empty() {
                    path.push(b'/');
                }
                path.extend(name);
                path_bytes = path_bytes.checked_add(path.len()).ok_or("path overflow")?;
                if path_bytes > 64 * 1024 * 1024 || entries.len() >= EDGE_LIMIT {
                    return Err("archive path limit".into());
                }
                let kind = uint(&m[&1])?;
                if kind == 5 {
                    return Err("Gitlink cannot be represented in source tar archive".into());
                }
                let id = digest(&m[&2])?;
                let mut size = 0;
                let mut link = Vec::new();
                if kind == 4 {
                    stack.push((id, path.clone(), depth + 1));
                } else if kind == 3 {
                    let Payload::Raw(b) = &self.get(id, &[1])?.payload else {
                        unreachable!()
                    };
                    if b.is_empty() || b.contains(&0) {
                        return Err("symlink target cannot be materialized".into());
                    }
                    content = content
                        .checked_add(b.len() as u64)
                        .ok_or("archive content overflow")?;
                    if content > 1024 * 1024 * 1024 {
                        return Err("archive logical content limit".into());
                    }
                    link = b.clone();
                } else {
                    size = match &self.records[&id].payload {
                        Payload::Raw(b) => b.len() as u64,
                        _ => uint(&fields(&self.records[&id])?[&0])?,
                    };
                }
                content = content
                    .checked_add(size)
                    .ok_or("archive content overflow")?;
                if content > 1024 * 1024 * 1024 {
                    return Err("archive logical content limit".into());
                }
                entries.push(ExportEntry {
                    path,
                    kind,
                    id,
                    size,
                    link,
                });
            }
        }
        entries.sort_by(|a, b| (a.kind == 3, &a.path).cmp(&(b.kind == 3, &b.path)));
        Ok(Archive {
            bundle: self,
            entries,
        })
    }
}
fn octal(dst: &mut [u8], n: u64) -> Result<(), String> {
    let s = format!("{n:0width$o}", width = dst.len() - 1);
    if s.len() >= dst.len() {
        return Err("tar integer overflow".into());
    }
    dst[..s.len()].copy_from_slice(s.as_bytes());
    Ok(())
}
pub(super) fn header(
    out: &mut impl Write,
    path: &[u8],
    mode: u64,
    size: u64,
    ty: u8,
    link: &[u8],
) -> Result<(), String> {
    let mut h = [0u8; 512];
    let n = path.len().min(100);
    h[..n].copy_from_slice(&path[..n]);
    octal(&mut h[100..108], mode)?;
    octal(&mut h[108..116], 0)?;
    octal(&mut h[116..124], 0)?;
    octal(&mut h[124..136], size)?;
    octal(&mut h[136..148], 0)?;
    h[148..156].fill(b' ');
    h[156] = ty;
    let n = link.len().min(100);
    h[157..157 + n].copy_from_slice(&link[..n]);
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    let sum: u64 = h.iter().map(|b| u64::from(*b)).sum();
    octal(&mut h[148..155], sum)?;
    h[155] = b' ';
    out.write_all(&h).map_err(|e| e.to_string())
}
pub(super) fn pad(out: &mut impl Write, n: u64) -> Result<(), String> {
    out.write_all(&[0u8; 512][..((512 - n % 512) % 512) as usize])
        .map_err(|e| e.to_string())
}
fn long(out: &mut impl Write, data: &[u8], ty: u8) -> Result<(), String> {
    header(out, b"././@LongLink", 0, (data.len() + 1) as u64, ty, &[])?;
    out.write_all(data)
        .and_then(|_| out.write_all(&[0]))
        .map_err(|e| e.to_string())?;
    pad(out, (data.len() + 1) as u64)
}
// POSIX pax path/linkpath values are explicitly UTF-8, unlike legacy tar names.
fn pax_field(data: &mut Vec<u8>, key: &str, value: &[u8]) {
    let body = key.len() + value.len() + 2; // equals sign and final newline
    let mut length = body + 2;
    loop {
        let next = body + 1 + length.to_string().len();
        if next == length {
            break;
        }
        length = next;
    }
    data.extend(format!("{length} {key}=").as_bytes());
    data.extend(value);
    data.push(b'\n');
}
fn unicode_field(value: &[u8]) -> bool {
    !value.is_ascii() && std::str::from_utf8(value).is_ok()
}
impl Archive<'_> {
    pub fn write(&self, mut out: impl Write) -> Result<(), String> {
        for e in &self.entries {
            let path_utf8 = unicode_field(&e.path);
            let link_utf8 = unicode_field(&e.link);
            if e.path.len() > 100 && !path_utf8 {
                long(&mut out, &e.path, b'L')?;
            }
            if e.link.len() > 100 && !link_utf8 {
                long(&mut out, &e.link, b'K')?;
            }
            let mut extended = Vec::new();
            if path_utf8 {
                pax_field(&mut extended, "path", &e.path);
            }
            if link_utf8 {
                pax_field(&mut extended, "linkpath", &e.link);
            }
            if !extended.is_empty() {
                header(
                    &mut out,
                    b"PaxHeaders/kai",
                    0o644,
                    extended.len() as u64,
                    b'x',
                    &[],
                )?;
                out.write_all(&extended).map_err(|e| e.to_string())?;
                pad(&mut out, extended.len() as u64)?;
            }
            let (mode, ty) = match e.kind {
                1 => (0o644, b'0'),
                2 => (0o755, b'0'),
                3 => (0o777, b'2'),
                4 => (0o755, b'5'),
                _ => unreachable!(),
            };
            header(&mut out, &e.path, mode, e.size, ty, &e.link)?;
            if e.kind == 1 || e.kind == 2 {
                self.bundle.file_bytes(e.id, &mut out)?;
                pad(&mut out, e.size)?;
            }
        }
        out.write_all(&[0u8; 1024])
            .and_then(|_| out.flush())
            .map_err(|e| e.to_string())
    }
}
