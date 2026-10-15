use crate::{ObjectId, codec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum EntryKind {
    Regular = 1,
    Executable = 2,
    Symlink = 3,
    Directory = 4,
    Gitlink = 5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Native(ObjectId),
    GitSha1([u8; 20]),
    GitSha256([u8; 32]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub name: Vec<u8>,
    pub kind: EntryKind,
    pub target: Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    entries: Vec<TreeEntry>,
}

pub(crate) fn valid_name(name: &[u8]) -> bool {
    !name.is_empty() && name != b"." && name != b".." && !name.contains(&0) && !name.contains(&b'/')
}

impl Tree {
    pub fn new(mut entries: Vec<TreeEntry>) -> Result<Self, String> {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for (i, entry) in entries.iter().enumerate() {
            if !valid_name(&entry.name) {
                return Err("invalid tree entry name".into());
            }
            if i > 0 && entries[i - 1].name == entry.name {
                return Err("duplicate tree entry name".into());
            }
            if (entry.kind == EntryKind::Gitlink) == matches!(entry.target, Target::Native(_)) {
                return Err("entry kind and target namespace disagree".into());
            }
        }
        let tree = Self { entries };
        tree.encode()?;
        Ok(tree)
    }

    pub fn entries(&self) -> &[TreeEntry] {
        &self.entries
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut payload = Vec::new();
        codec::head(&mut payload, 4, self.entries.len() as u64);
        for entry in &self.entries {
            if entry.name.len() > codec::STRUCTURED_LIMIT {
                return Err("tree name exceeds payload limit".into());
            }
            codec::head(&mut payload, 5, 3);
            codec::head(&mut payload, 0, 0);
            codec::bytes(&mut payload, &entry.name);
            codec::head(&mut payload, 0, 1);
            codec::head(&mut payload, 0, entry.kind as u64);
            codec::head(&mut payload, 0, 2);
            match &entry.target {
                Target::Native(id) => codec::reference(&mut payload, *id),
                Target::GitSha1(id) => {
                    codec::head(&mut payload, 4, 3);
                    codec::head(&mut payload, 0, 0);
                    codec::head(&mut payload, 0, 1);
                    codec::bytes(&mut payload, id);
                }
                Target::GitSha256(id) => {
                    codec::head(&mut payload, 4, 3);
                    codec::head(&mut payload, 0, 0);
                    codec::head(&mut payload, 0, 2);
                    codec::bytes(&mut payload, id);
                }
            }
            if payload.len() > codec::STRUCTURED_LIMIT {
                return Err("tree exceeds payload limit".into());
            }
        }
        codec::bounded(payload, 2)
    }

    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (kind, payload) = codec::unframe(input)?;
        if kind != 2 {
            return Err("not a tree".into());
        }
        let mut d = codec::Decoder::new(payload);
        let count = d.count(4, 31)?;
        let mut entries: Vec<TreeEntry> = Vec::new();
        for _ in 0..count {
            d.expect(5, 3)?;
            d.expect(0, 0)?;
            let name = d.bytes()?.to_vec();
            if entries.last().is_some_and(|prev| prev.name >= name) {
                return Err("tree names must be strictly sorted".into());
            }
            d.expect(0, 1)?;
            let kind = match d.head(0)? {
                1 => EntryKind::Regular,
                2 => EntryKind::Executable,
                3 => EntryKind::Symlink,
                4 => EntryKind::Directory,
                5 => EntryKind::Gitlink,
                _ => return Err("unknown tree entry kind".into()),
            };
            d.expect(0, 2)?;
            d.expect(4, 3)?;
            let version = d.head(0)?;
            let algorithm = d.head(0)?;
            let bytes = d.bytes()?;
            let target = match (version, algorithm) {
                (1, 1) => Target::Native(ObjectId::from_digest(
                    bytes
                        .try_into()
                        .map_err(|_| "invalid native digest length")?,
                )),
                (0, 1) => Target::GitSha1(
                    bytes
                        .try_into()
                        .map_err(|_| "invalid SHA-1 digest length")?,
                ),
                (0, 2) => Target::GitSha256(
                    bytes
                        .try_into()
                        .map_err(|_| "invalid SHA-256 digest length")?,
                ),
                _ => return Err("unsupported reference namespace/algorithm".into()),
            };
            entries.push(TreeEntry { name, kind, target });
        }
        d.finish()?;
        Self::new(entries)
    }
}
