//! Independent required-edge and repository-binding checks over generic CBOR.
use super::bundle::{Bundle, Digest, digest, fields, header, hex, pad};
use super::{Payload, Record, Value, array, bytes, map, uint};
use std::{collections::BTreeMap, io::Write};
impl Bundle {
    pub(super) fn repository_edges(&self, r: &Record) -> Result<Vec<(Digest, Vec<u16>)>, String> {
        let mut edges = Vec::new();
        if matches!(r.kind, 1 | 4 | 7 | 12 | 13 | 18) {
            return Ok(edges);
        }
        let m = fields(r)?;
        let mut add = |v: &Value, kinds: &[u16]| -> Result<(), String> {
            edges.push((digest(v)?, kinds.to_vec()));
            Ok(())
        };
        match r.kind {
            5 => {
                for key in [2, 3] {
                    for version in array(&m[&key])? {
                        let v = map(version, &[0, 1], &[2])?;
                        if let Some(rev) = v.get(&2) {
                            add(rev, &[3])?;
                        }
                        for state in array(&v[&1])? {
                            let s = array(state)?;
                            if uint(&s[0])? == 0 {
                                continue;
                            }
                            let kinds: &[u16] = match uint(&s[1])? {
                                1 | 2 => &[1, 14],
                                3 => &[1],
                                4 => &[2],
                                5 => continue,
                                _ => return Err("invalid conflict kind".into()),
                            };
                            add(&s[2], kinds)?;
                        }
                    }
                }
            }
            6 => {
                for entry in array(&m[&1])? {
                    let r = array(entry)?;
                    let name = bytes(&r[0])?;
                    let kinds: &[u16] = if name.starts_with(b"tags/") {
                        &[3, 15]
                    } else if name.starts_with(b"shelves/") {
                        &[8]
                    } else {
                        &[3]
                    };
                    let state = array(&r[2])?;
                    match uint(&state[0])? {
                        0 => {}
                        1 => add(&state[1], kinds)?,
                        2 => {
                            for id in array(&state[1])? {
                                add(id, kinds)?;
                            }
                        }
                        _ => return Err("invalid ref state".into()),
                    }
                }
                for draft in array(&m[&2])? {
                    for id in array(&array(draft)?[1])? {
                        add(id, &[3])?;
                    }
                }
                for workspace in array(&m[&3])? {
                    let w = map(workspace, &[0], &[1, 2])?;
                    if let Some(v) = w.get(&1) {
                        add(v, &[3])?;
                    }
                    if let Some(v) = w.get(&2) {
                        add(v, &[8])?;
                    }
                }
                for pin in array(&m[&4])? {
                    let p = array(pin)?;
                    add(&p[1], &[uint(&p[0])? as u16])?;
                }
            }
            8 => {
                add(&m[&1], &[2])?;
                add(&m[&2], &[2])?;
                if let Some(v) = m.get(&6) {
                    add(v, &[3])?;
                }
                if let Some(v) = m.get(&9) {
                    add(v, &[17])?;
                }
                for key in [7, 10] {
                    if let Some(v) = m.get(&key) {
                        for p in array(v)? {
                            add(&array(p)?[1], &[5])?;
                        }
                    }
                }
            }
            9 => {
                add(&m[&0], &[3])?;
                add(&m[&1], &[2])?;
                for key in [6, 7] {
                    add(&array(&m[&key])?[1], &[1, 14])?;
                }
            }
            10 => add(&m[&4], &[uint(&m[&3])? as u16])?,
            11 => add(&m[&0], &[2])?,
            16 | 19 => {
                if r.kind == 16 {
                    add(&m[&2], &[6])?;
                    if let Some(v) = m.get(&3) {
                        add(v, &[7])?;
                    }
                }
                let c = map(
                    &m[&if r.kind == 16 { 4 } else { 1 }],
                    &[0, 1, 2, 3, 4, 5, 6],
                    &[],
                )?;
                for root in array(&c[&1])? {
                    let p = array(root)?;
                    add(&p[0], &[uint(&p[1])? as u16])?;
                }
            }
            17 => {
                add(&m[&3], &[2])?;
                add(&m[&7], &[1, 14])?;
            }
            _ => return Err("unsupported repository edge type".into()),
        }
        Ok(edges)
    }
    pub(super) fn verify_repository_bindings(&self, work: &mut usize) -> Result<(), String> {
        for r in std::iter::once(&self.manifest).chain(self.records.values()) {
            if !matches!(r.kind, 3 | 6 | 8 | 9 | 16) {
                continue;
            }
            let m = fields(r)?;
            match r.kind {
                3 => {
                    if let Some(conflicts) = m.get(&8) {
                        self.conflict_bindings(digest(&m[&0])?, conflicts, work)?;
                    }
                }
                6 => {
                    for draft in array(&m[&2])? {
                        let d = array(draft)?;
                        for head in array(&d[1])? {
                            if bytes(&fields(self.get(digest(head)?, &[3])?)?[&2])? != bytes(&d[0])?
                            {
                                return Err("draft change ID mismatch".into());
                            }
                        }
                    }
                    for workspace in array(&m[&3])? {
                        let w = map(workspace, &[0], &[1, 2])?;
                        if let Some(cp) = w.get(&2)
                            && bytes(&fields(self.get(digest(cp)?, &[8])?)?[&0])? != bytes(&w[&0])?
                        {
                            return Err("checkpoint workspace mismatch".into());
                        }
                    }
                }
                8 => {
                    for (key, tree) in [(7, 1), (10, 2)] {
                        if let Some(c) = m.get(&key) {
                            self.conflict_bindings(digest(&m[&tree])?, c, work)?;
                        }
                    }
                    if let Some(barrier) = m.get(&9) {
                        let b = fields(self.get(digest(barrier)?, &[17])?)?;
                        if b[&0] != m[&0] || b[&3] != m[&1] {
                            return Err("checkpoint barrier binding mismatch".into());
                        }
                    }
                }
                9 => {
                    let rev = fields(self.get(digest(&m[&0])?, &[3])?)?;
                    if rev[&0] != m[&1]
                        || matches!(
                            (uint(&m[&2])?, rev.contains_key(&8)),
                            (1, true) | (2, false)
                        )
                    {
                        return Err("evidence source binding mismatch".into());
                    }
                }
                16 => {
                    if let Some(op) = m.get(&3)
                        && fields(self.get(digest(op)?, &[7])?)?[&3] != m[&2]
                    {
                        return Err("backup operation post-view mismatch".into());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn conflict_bindings(
        &self,
        tree: Digest,
        manifest: &Value,
        work: &mut usize,
    ) -> Result<(), String> {
        let mut bindings = BTreeMap::<Digest, Vec<&[u8]>>::new();
        for p in array(manifest)? {
            let p = array(p)?;
            bindings
                .entry(digest(&p[1])?)
                .or_default()
                .push(bytes(&p[0])?);
        }
        for (id, paths) in bindings {
            let c = fields(self.get(id, &[5])?)?;
            let declared = array(&c[&1])?;
            if paths.len() != declared.len() {
                return Err("conflict manifest/path mismatch".into());
            }
            for (path, declared) in paths.iter().zip(declared) {
                if *path != bytes(declared)? {
                    return Err("conflict manifest/path mismatch".into());
                }
            }
        }
        for p in array(manifest)? {
            let path = bytes(&array(p)?[0])?;
            let mut parts = path.split(|b| *b == b'/').peekable();
            let mut id = tree;
            while let Some(part) = parts.next() {
                *work = work.checked_add(1).ok_or("conflict work overflow")?;
                if *work > 1_000_000 {
                    return Err("conflict path work limit".into());
                }
                let Payload::Structured(v) = &self.get(id, &[2])?.payload else {
                    unreachable!()
                };
                let entries = array(v)?;
                // Entries already passed the independent tree schema validator.
                let found = entries.binary_search_by(|e| {
                    let Value::Map(m) = e else { unreachable!() };
                    let Value::Bytes(n) = &m[&0] else {
                        unreachable!()
                    };
                    n.as_slice().cmp(part)
                });
                let Ok(i) = found else {
                    break;
                };
                let e = map(&entries[i], &[0, 1, 2], &[])?;
                if parts.peek().is_none() {
                    return Err("unresolved path exists in ordinary tree".into());
                }
                if uint(&e[&1])? != 4 {
                    return Err("unresolved path has non-directory ancestor".into());
                }
                id = digest(&e[&2])?;
            }
        }
        Ok(())
    }
}
fn id_text(v: &Value) -> Result<String, String> {
    Ok(format!("k1:b3:{}", hex(&digest(v)?)))
}
fn line(out: &mut String, text: String) -> Result<(), String> {
    if out.len().saturating_add(text.len()).saturating_add(1) > 64 * 1024 * 1024 {
        return Err("recovery index size limit".into());
    }
    out.push_str(&text);
    out.push('\n');
    Ok(())
}
impl Bundle {
    /// Descriptive index; canonical objects and the original manifest remain authority.
    pub fn recovery_index(&self) -> Result<String, String> {
        let mut out = String::from(
            "KAI-RECOVERY-INDEX 1\nPortable immutable state; no live database installed.\nNames and byte identifiers use lowercase hex.\n",
        );
        let top = fields(&self.manifest)?;
        line(&mut out, format!("repository {}", hex(bytes(&top[&0])?)))?;
        if self.manifest.kind == 16 {
            line(
                &mut out,
                format!(
                    "backup generation={} current-view={}",
                    uint(&top[&1])?,
                    id_text(&top[&2])?
                ),
            )?;
            if let Some(op) = top.get(&3) {
                line(&mut out, format!("current-operation {}", id_text(op)?))?;
            }
        } else {
            line(
                &mut out,
                "transfer; no asserted current repository generation".into(),
            )?;
        }
        for root in self.roots() {
            line(&mut out, format!("root {root}"))?;
        }
        for (id, data) in &self.objects {
            let ty = u16::from_be_bytes([data[13], data[14]]);
            line(
                &mut out,
                format!(
                    "object k1:b3:{} type={ty} bytes={} interpretation={}",
                    hex(id),
                    data.len(),
                    if self.records.contains_key(id) {
                        "interpreted"
                    } else {
                        "opaque"
                    }
                ),
            )?;
            let Some(r) = self.records.get(id) else {
                continue;
            };
            if !matches!(r.kind, 3 | 5 | 6 | 7 | 8) {
                continue;
            }
            let m = fields(r)?;
            match r.kind {
                3 => {
                    line(
                        &mut out,
                        format!(
                            "  revision tree={} change={}",
                            id_text(&m[&0])?,
                            hex(bytes(&m[&2])?)
                        ),
                    )?;
                    for p in array(&m[&1])? {
                        line(&mut out, format!("  parent {}", id_text(p)?))?;
                    }
                    index_conflicts(&mut out, m.get(&8), "revision")?;
                }
                5 => {
                    for p in array(&m[&1])? {
                        line(&mut out, format!("  conflict-path {}", hex(bytes(p)?)))?;
                    }
                    for key in [2, 3] {
                        for (i, v) in array(&m[&key])?.iter().enumerate() {
                            let v = map(v, &[0, 1], &[2])?;
                            line(
                                &mut out,
                                format!(
                                    "  conflict-{} {i} label={}",
                                    if key == 2 { "base" } else { "side" },
                                    hex(bytes(&v[&0])?)
                                ),
                            )?;
                            for (j, s) in array(&v[&1])?.iter().enumerate() {
                                let s = array(s)?;
                                if uint(&s[0])? == 0 {
                                    line(&mut out, format!("    state {j} absent"))?;
                                } else {
                                    let kind = uint(&s[1])?;
                                    let target = if kind == 5 {
                                        let r = array(&s[2])?;
                                        format!("git-{}:{}", uint(&r[1])?, hex(bytes(&r[2])?))
                                    } else {
                                        id_text(&s[2])?
                                    };
                                    line(
                                        &mut out,
                                        format!("    state {j} kind={kind} target={target}"),
                                    )?;
                                }
                            }
                        }
                    }
                }
                6 => {
                    line(&mut out, format!("  view generation={}", uint(&m[&0])?))?;
                    for v in array(&m[&1])? {
                        let r = array(v)?;
                        let state = array(&r[2])?;
                        let targets = match uint(&state[0])? {
                            0 => "deleted".into(),
                            1 => id_text(&state[1])?,
                            _ => array(&state[1])?
                                .iter()
                                .map(id_text)
                                .collect::<Result<Vec<_>, _>>()?
                                .join(","),
                        };
                        line(
                            &mut out,
                            format!(
                                "  ref name={} generation={} state={} targets={targets}",
                                hex(bytes(&r[0])?),
                                uint(&r[1])?,
                                uint(&state[0])?
                            ),
                        )?;
                    }
                    for v in array(&m[&2])? {
                        let d = array(v)?;
                        for h in array(&d[1])? {
                            line(
                                &mut out,
                                format!(
                                    "  draft change={} head={}",
                                    hex(bytes(&d[0])?),
                                    id_text(h)?
                                ),
                            )?;
                        }
                    }
                    for v in array(&m[&3])? {
                        let w = map(v, &[0], &[1, 2])?;
                        line(
                            &mut out,
                            format!(
                                "  workspace {} base={} checkpoint={}",
                                hex(bytes(&w[&0])?),
                                optional_id(w.get(&1))?,
                                optional_id(w.get(&2))?
                            ),
                        )?;
                    }
                    for v in array(&m[&4])? {
                        let p = array(v)?;
                        line(
                            &mut out,
                            format!("  retained type={} {}", uint(&p[0])?, id_text(&p[1])?),
                        )?;
                    }
                }
                7 => {
                    for (name, key) in [("pre-view", 2), ("post-view", 3)] {
                        let id = digest(&m[&key])?;
                        line(
                            &mut out,
                            format!(
                                "  operation {name}={} included={}",
                                id_text(&m[&key])?,
                                self.records.contains_key(&id)
                            ),
                        )?;
                    }
                    for p in array(&m[&1])? {
                        line(
                            &mut out,
                            format!(
                                "  predecessor {} included={}",
                                id_text(p)?,
                                self.records.contains_key(&digest(p)?)
                            ),
                        )?;
                    }
                }
                8 => {
                    line(
                        &mut out,
                        format!(
                            "  checkpoint workspace={} tree={} staged={} base={} predecessor={}",
                            hex(bytes(&m[&0])?),
                            id_text(&m[&1])?,
                            id_text(&m[&2])?,
                            optional_id(m.get(&6))?,
                            optional_id(m.get(&8))?
                        ),
                    )?;
                    index_conflicts(&mut out, m.get(&7), "working")?;
                    index_conflicts(&mut out, m.get(&10), "staged")?;
                }
                _ => {}
            }
        }
        Ok(out)
    }
    /// Stream the exact canonical container represented by this verified bundle.
    pub fn write_bundle(&self, mut out: impl Write) -> Result<(), String> {
        out.write_all(b"KAI-BUNDLE\0\0\x01")
            .map_err(|e| e.to_string())?;
        for b in std::iter::once(&self.manifest_bytes).chain(self.objects.values()) {
            out.write_all(&(b.len() as u64).to_be_bytes())
                .and_then(|_| out.write_all(b))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    /// Preflight the derived index before the caller creates an output file.
    pub fn recovery(&self) -> Result<RecoveryArchive<'_>, String> {
        Ok(RecoveryArchive {
            bundle: self,
            index: self.recovery_index()?,
        })
    }
}
fn optional_id(v: Option<&Value>) -> Result<String, String> {
    v.map(id_text)
        .transpose()
        .map(|v| v.unwrap_or_else(|| "absent".into()))
}
fn index_conflicts(out: &mut String, v: Option<&Value>, scope: &str) -> Result<(), String> {
    if let Some(v) = v {
        for p in array(v)? {
            let p = array(p)?;
            line(
                out,
                format!(
                    "  {scope}-conflict path={} object={}",
                    hex(bytes(&p[0])?),
                    id_text(&p[1])?
                ),
            )?;
        }
    }
    Ok(())
}
pub struct RecoveryArchive<'a> {
    bundle: &'a Bundle,
    index: String,
}
fn archive_file(out: &mut impl Write, name: &str, data: &[u8]) -> Result<(), String> {
    header(out, name.as_bytes(), 0o644, data.len() as u64, b'0', &[])?;
    out.write_all(data).map_err(|e| e.to_string())?;
    pad(out, data.len() as u64)
}
impl RecoveryArchive<'_> {
    pub fn write(&self, mut out: impl Write) -> Result<(), String> {
        archive_file(&mut out, "INDEX.txt", self.index.as_bytes())?;
        archive_file(&mut out, "manifest.object", &self.bundle.manifest_bytes)?;
        header(&mut out, b"objects", 0o755, 0, b'5', &[])?;
        for (id, b) in &self.bundle.objects {
            archive_file(&mut out, &format!("objects/{}.object", hex(id)), b)?;
        }
        let size = 13
            + 8
            + self.bundle.manifest_bytes.len() as u64
            + self
                .bundle
                .objects
                .values()
                .map(|b| 8 + b.len() as u64)
                .sum::<u64>();
        header(&mut out, b"repository.kai-bundle", 0o644, size, b'0', &[])?;
        self.bundle.write_bundle(&mut out)?;
        pad(&mut out, size)?;
        out.write_all(&[0; 1024])
            .and_then(|_| out.flush())
            .map_err(|e| e.to_string())
    }
}
