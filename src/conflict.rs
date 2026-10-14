use crate::{
    EntryKind, ObjectId, Target,
    codec::{self, Decoder},
    recovery::*,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ConflictKind {
    Content = 1,
    Binary = 2,
    DeleteModify = 3,
    KindMode = 4,
    Rename = 5,
    MultipleBases = 6,
    AddAdd = 7,
    Symlink = 8,
    Gitlink = 9,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileState {
    pub kind: EntryKind,
    pub target: Target,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictVersion {
    pub label: Vec<u8>,
    pub states: Vec<Option<FileState>>,
    pub source_revision: Option<ObjectId>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RenameCandidate {
    pub identity: [u8; 16],
    pub source: Vec<u8>,
    pub target: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub paths: Vec<Vec<u8>>,
    pub bases: Vec<ConflictVersion>,
    pub sides: Vec<ConflictVersion>,
    pub algorithm: Vec<u8>,
    pub policy: Vec<u8>,
    pub renames: Vec<RenameCandidate>,
    pub resolution_provenance: Vec<ObjectId>,
}

impl FileState {
    fn validate(&self) -> Result<(), String> {
        if (self.kind == EntryKind::Gitlink) == matches!(self.target, Target::Native(_)) {
            return Err("file kind and target namespace disagree".into());
        }
        Ok(())
    }
    fn encode(&self, out: &mut Vec<u8>) {
        codec::head(out, 4, 3);
        field(out, 1);
        field(out, self.kind as u64);
        match &self.target {
            Target::Native(id) => codec::reference(out, *id),
            Target::GitSha1(id) => {
                codec::head(out, 4, 3);
                field(out, 0);
                field(out, 1);
                codec::bytes(out, id);
            }
            Target::GitSha256(id) => {
                codec::head(out, 4, 3);
                field(out, 0);
                field(out, 2);
                codec::bytes(out, id);
            }
        }
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Option<Self>, String> {
        let count = d.head(4)?;
        let tag = d.head(0)?;
        if count == 1 && tag == 0 {
            return Ok(None);
        }
        if count != 3 || tag != 1 {
            return Err("invalid conflict state".into());
        }
        let kind = match d.head(0)? {
            1 => EntryKind::Regular,
            2 => EntryKind::Executable,
            3 => EntryKind::Symlink,
            4 => EntryKind::Directory,
            5 => EntryKind::Gitlink,
            _ => return Err("invalid conflict file kind".into()),
        };
        d.expect(4, 3)?;
        let namespace = d.head(0)?;
        let algorithm = d.head(0)?;
        let target = match (namespace, algorithm) {
            (1, 1) => Target::Native(ObjectId::from_digest(fixed(d)?)),
            (0, 1) => Target::GitSha1(fixed(d)?),
            (0, 2) => Target::GitSha256(fixed(d)?),
            _ => return Err("invalid target namespace/algorithm".into()),
        };
        let state = Self { kind, target };
        state.validate()?;
        Ok(Some(state))
    }
}

impl ConflictVersion {
    fn encode(&self, out: &mut Vec<u8>) {
        codec::head(out, 5, 2 + u64::from(self.source_revision.is_some()));
        field(out, 0);
        codec::bytes(out, &self.label);
        field(out, 1);
        codec::head(out, 4, self.states.len() as u64);
        for state in &self.states {
            match state {
                Some(state) => state.encode(out),
                None => {
                    codec::head(out, 4, 1);
                    field(out, 0);
                }
            }
        }
        if let Some(id) = self.source_revision {
            field(out, 2);
            codec::reference(out, id);
        }
    }
    fn decode(d: &mut Decoder<'_>, paths: usize) -> Result<Self, String> {
        let count = d.head(5)?;
        if count != 2 && count != 3 {
            return Err("invalid conflict version fields".into());
        }
        d.expect(0, 0)?;
        let label = d.bytes()?.to_vec();
        nonempty(&label)?;
        d.expect(0, 1)?;
        let len = d.count(4, 2)?;
        if len != paths {
            return Err("conflict version/path count mismatch".into());
        }
        let states = (0..len)
            .map(|_| FileState::decode(d))
            .collect::<Result<_, _>>()?;
        let source_revision = if count == 3 {
            d.expect(0, 2)?;
            Some(d.reference()?)
        } else {
            None
        };
        Ok(Self {
            label,
            states,
            source_revision,
        })
    }
}

impl Conflict {
    pub fn validate(&self) -> Result<(), String> {
        if self.paths.is_empty() || !ordered(&self.paths) || self.paths.iter().any(|p| !path(p)) {
            return Err("invalid or unordered conflict paths".into());
        }
        if self.sides.len() < 2 {
            return Err("conflict requires at least two sides".into());
        }
        nonempty(&self.algorithm)?;
        nonempty(&self.policy)?;
        let mut size = self.algorithm.len().saturating_add(self.policy.len());
        for p in &self.paths {
            size = size.saturating_add(p.len()).saturating_add(9);
        }
        for version in self.bases.iter().chain(&self.sides) {
            nonempty(&version.label)?;
            if version.states.len() != self.paths.len() {
                return Err("conflict version/path count mismatch".into());
            }
            size = size
                .saturating_add(version.label.len())
                .saturating_add(version.states.len().saturating_mul(48))
                .saturating_add(64);
            for state in version.states.iter().flatten() {
                state.validate()?;
            }
        }
        if !ordered(&self.renames) {
            return Err("unordered or duplicate rename candidate".into());
        }
        for rename in &self.renames {
            if rename.source == rename.target
                || self.paths.binary_search(&rename.source).is_err()
                || self.paths.binary_search(&rename.target).is_err()
            {
                return Err("rename candidate outside conflict paths".into());
            }
            size = size
                .saturating_add(rename.source.len())
                .saturating_add(rename.target.len())
                .saturating_add(40);
        }
        size = size.saturating_add(self.resolution_provenance.len().saturating_mul(37));
        input_bound(size)
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut out = Vec::new();
        codec::head(
            &mut out,
            5,
            6 + u64::from(!self.renames.is_empty())
                + u64::from(!self.resolution_provenance.is_empty()),
        );
        field(&mut out, 0);
        field(&mut out, self.kind as u64);
        field(&mut out, 1);
        codec::head(&mut out, 4, self.paths.len() as u64);
        for p in &self.paths {
            codec::bytes(&mut out, p);
        }
        for (key, versions) in [(2, &self.bases), (3, &self.sides)] {
            field(&mut out, key);
            codec::head(&mut out, 4, versions.len() as u64);
            for version in versions {
                version.encode(&mut out);
            }
        }
        field(&mut out, 4);
        codec::bytes(&mut out, &self.algorithm);
        field(&mut out, 5);
        codec::bytes(&mut out, &self.policy);
        if !self.renames.is_empty() {
            field(&mut out, 6);
            codec::head(&mut out, 4, self.renames.len() as u64);
            for r in &self.renames {
                codec::head(&mut out, 4, 3);
                codec::bytes(&mut out, &r.identity);
                codec::bytes(&mut out, &r.source);
                codec::bytes(&mut out, &r.target);
            }
        }
        if !self.resolution_provenance.is_empty() {
            field(&mut out, 7);
            write_refs(&mut out, &self.resolution_provenance);
        }
        codec::bounded(out, 5)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, count) = decoder(input, 5, 6, 8)?;
        d.expect(0, 0)?;
        let kind = match d.head(0)? {
            1 => ConflictKind::Content,
            2 => ConflictKind::Binary,
            3 => ConflictKind::DeleteModify,
            4 => ConflictKind::KindMode,
            5 => ConflictKind::Rename,
            6 => ConflictKind::MultipleBases,
            7 => ConflictKind::AddAdd,
            8 => ConflictKind::Symlink,
            9 => ConflictKind::Gitlink,
            _ => return Err("unknown conflict kind".into()),
        };
        d.expect(0, 1)?;
        let n = d.count(4, 2)?;
        let paths = (0..n)
            .map(|_| Ok(d.bytes()?.to_vec()))
            .collect::<Result<Vec<_>, String>>()?;
        d.expect(0, 2)?;
        let n = d.count(4, 5)?;
        let bases = (0..n)
            .map(|_| ConflictVersion::decode(&mut d, paths.len()))
            .collect::<Result<_, _>>()?;
        d.expect(0, 3)?;
        let n = d.count(4, 5)?;
        let sides = (0..n)
            .map(|_| ConflictVersion::decode(&mut d, paths.len()))
            .collect::<Result<_, _>>()?;
        d.expect(0, 4)?;
        let algorithm = d.bytes()?.to_vec();
        d.expect(0, 5)?;
        let policy = d.bytes()?.to_vec();
        let mut value = Self {
            kind,
            paths,
            bases,
            sides,
            algorithm,
            policy,
            renames: vec![],
            resolution_provenance: vec![],
        };
        let mut last = 5;
        for _ in 6..count {
            match next_key(&mut d, &mut last)? {
                6 => {
                    let n = d.count(4, 22)?;
                    if n == 0 {
                        return Err("empty optional renames".into());
                    }
                    for _ in 0..n {
                        d.expect(4, 3)?;
                        value.renames.push(RenameCandidate {
                            identity: fixed(&mut d)?,
                            source: d.bytes()?.to_vec(),
                            target: d.bytes()?.to_vec(),
                        });
                    }
                }
                7 => {
                    value.resolution_provenance = read_refs(&mut d)?;
                    if value.resolution_provenance.is_empty() {
                        return Err("empty optional provenance".into());
                    }
                }
                _ => return Err("unknown conflict field".into()),
            }
        }
        d.finish()?;
        value.validate()?;
        Ok(value)
    }
}
