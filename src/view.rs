//! Experimental type-6 registry: docs/VIEW-FORMAT.md.
use crate::{ObjectId, codec, recovery::*};
use unicode_normalization::is_nfc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefState {
    Deleted,
    Target(ObjectId),
    DraftConflict(Vec<ObjectId>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewRef {
    pub name: String,
    pub generation: u64,
    pub state: RefState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftHeads {
    pub change_id: [u8; 16],
    pub heads: Vec<ObjectId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRoot {
    pub workspace: [u8; 16],
    pub base: Option<ObjectId>,
    pub checkpoint: Option<ObjectId>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RetainedRoot {
    pub type_code: u16,
    pub id: ObjectId,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub namespace_generation: u64,
    pub refs: Vec<ViewRef>,
    pub drafts: Vec<DraftHeads>,
    pub workspaces: Vec<WorkspaceRoot>,
    pub retained: Vec<RetainedRoot>,
}

pub(crate) fn ref_types(name: &str) -> Result<&'static [u16], String> {
    if !is_nfc(name) || name.chars().any(|c| c.is_control() || c == '\\') {
        return Err("ref name must be NFC without controls or backslashes".into());
    }
    let parts: Vec<_> = name.split('/').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return Err("invalid ref name component".into());
    }
    match parts.as_slice() {
        ["branches", _, ..] => Ok(&[3]),
        ["tags", _, ..] => Ok(&[3, 15]),
        ["shelves", _, ..] => Ok(&[8]),
        ["remotes", _, "branches", _, ..] => Ok(&[3]),
        _ => Err("unknown or reserved ref namespace".into()),
    }
}
pub(crate) fn pin_types(kind: u16) -> &'static [u16] {
    match kind {
        1 => &[1],
        2 => &[2],
        3 => &[3],
        4 => &[4],
        5 => &[5],
        6 => &[6],
        7 => &[7],
        8 => &[8],
        9 => &[9],
        10 => &[10],
        11 => &[11],
        12 => &[12],
        13 => &[13],
        14 => &[14],
        15 => &[15],
        16 => &[16],
        17 => &[17],
        18 => &[18],
        19 => &[19],
        _ => &[],
    }
}
impl View {
    fn validate(&self) -> Result<(), String> {
        if !self.refs.windows(2).all(|w| w[0].name < w[1].name)
            || !self
                .drafts
                .windows(2)
                .all(|w| w[0].change_id < w[1].change_id)
            || !self
                .workspaces
                .windows(2)
                .all(|w| w[0].workspace < w[1].workspace)
            || !ordered(&self.retained)
        {
            return Err("duplicate or unordered view keys".into());
        }
        for r in &self.refs {
            ref_types(&r.name)?;
            if r.generation == 0 {
                return Err("ref generation must be positive".into());
            }
            if let RefState::DraftConflict(heads) = &r.state
                && (!r.name.starts_with("branches/") || heads.len() < 2 || !ordered(heads))
            {
                return Err("invalid explicit branch alternatives".into());
            }
        }
        for d in &self.drafts {
            if d.heads.is_empty() || !ordered(&d.heads) {
                return Err("draft heads must be nonempty, unique and sorted".into());
            }
        }
        if self
            .retained
            .iter()
            .any(|p| pin_types(p.type_code).is_empty())
        {
            return Err("invalid retained root type".into());
        }
        Ok(())
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        // Bound input collections before allocating the encoded payload. This is
        // a conservative lower bound; bounded() checks the exact encoded size.
        let mut size = 0usize;
        for r in &self.refs {
            size = size.saturating_add(r.name.len()).saturating_add(5);
            if let RefState::DraftConflict(ids) = &r.state {
                size = size.saturating_add(ids.len().saturating_mul(37));
            }
        }
        for draft in &self.drafts {
            size = size
                .saturating_add(19)
                .saturating_add(draft.heads.len().saturating_mul(37));
        }
        size = size
            .saturating_add(self.workspaces.len().saturating_mul(19))
            .saturating_add(self.retained.len().saturating_mul(39));
        input_bound(size)?;
        self.validate()?;
        let mut out = Vec::new();
        codec::head(&mut out, 5, 6);
        field(&mut out, 0);
        field(&mut out, self.namespace_generation);
        field(&mut out, 1);
        codec::head(&mut out, 4, self.refs.len() as u64);
        for r in &self.refs {
            codec::head(&mut out, 4, 3);
            codec::bytes(&mut out, r.name.as_bytes());
            field(&mut out, r.generation);
            match &r.state {
                RefState::Deleted => {
                    codec::head(&mut out, 4, 1);
                    field(&mut out, 0);
                }
                RefState::Target(id) => {
                    codec::head(&mut out, 4, 2);
                    field(&mut out, 1);
                    codec::reference(&mut out, *id);
                }
                RefState::DraftConflict(ids) => {
                    codec::head(&mut out, 4, 2);
                    field(&mut out, 2);
                    write_refs(&mut out, ids);
                }
            }
            input_bound(out.len())?;
        }
        field(&mut out, 2);
        codec::head(&mut out, 4, self.drafts.len() as u64);
        for d in &self.drafts {
            codec::head(&mut out, 4, 2);
            codec::bytes(&mut out, &d.change_id);
            write_refs(&mut out, &d.heads);
            input_bound(out.len())?;
        }
        field(&mut out, 3);
        codec::head(&mut out, 4, self.workspaces.len() as u64);
        for w in &self.workspaces {
            codec::head(
                &mut out,
                5,
                1 + u64::from(w.base.is_some()) + u64::from(w.checkpoint.is_some()),
            );
            field(&mut out, 0);
            codec::bytes(&mut out, &w.workspace);
            if let Some(id) = w.base {
                field(&mut out, 1);
                codec::reference(&mut out, id);
            }
            if let Some(id) = w.checkpoint {
                field(&mut out, 2);
                codec::reference(&mut out, id);
            }
            input_bound(out.len())?;
        }
        field(&mut out, 4);
        codec::head(&mut out, 4, self.retained.len() as u64);
        for p in &self.retained {
            codec::head(&mut out, 4, 2);
            field(&mut out, p.type_code as u64);
            codec::reference(&mut out, p.id);
            input_bound(out.len())?;
        }
        field(&mut out, 5);
        field(&mut out, 0);
        codec::bounded(out, 6)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, _) = decoder(input, 6, 6, 6)?;
        d.expect(0, 0)?;
        let namespace_generation = d.head(0)?;
        d.expect(0, 1)?;
        let count = d.count(4, 5)?;
        let mut refs = Vec::new();
        for _ in 0..count {
            d.expect(4, 3)?;
            let name = std::str::from_utf8(d.bytes()?)
                .map_err(|_| "ref name is not UTF-8")?
                .to_owned();
            let generation = d.head(0)?;
            let len = d.head(4)?;
            let state = match (len, d.head(0)?) {
                (1, 0) => RefState::Deleted,
                (2, 1) => RefState::Target(d.reference()?),
                (2, 2) => RefState::DraftConflict(read_refs(&mut d)?),
                _ => return Err("invalid ref state".into()),
            };
            refs.push(ViewRef {
                name,
                generation,
                state,
            });
        }
        d.expect(0, 2)?;
        let count = d.count(4, 19)?;
        let mut drafts = Vec::new();
        for _ in 0..count {
            d.expect(4, 2)?;
            drafts.push(DraftHeads {
                change_id: fixed(&mut d)?,
                heads: read_refs(&mut d)?,
            });
        }
        d.expect(0, 3)?;
        let count = d.count(4, 19)?;
        let mut workspaces = Vec::new();
        for _ in 0..count {
            let fields = d.head(5)?;
            if !(1..=3).contains(&fields) {
                return Err("invalid workspace field count".into());
            }
            d.expect(0, 0)?;
            let workspace = fixed(&mut d)?;
            let mut base = None;
            let mut checkpoint = None;
            let mut previous = 0;
            for _ in 1..fields {
                match next_key(&mut d, &mut previous)? {
                    1 => base = Some(d.reference()?),
                    2 => checkpoint = Some(d.reference()?),
                    _ => return Err("unknown workspace field".into()),
                }
            }
            workspaces.push(WorkspaceRoot {
                workspace,
                base,
                checkpoint,
            });
        }
        d.expect(0, 4)?;
        let count = d.count(4, 39)?;
        let mut retained = Vec::new();
        for _ in 0..count {
            d.expect(4, 2)?;
            let type_code = u16::try_from(d.head(0)?).map_err(|_| "retained type overflow")?;
            retained.push(RetainedRoot {
                type_code,
                id: d.reference()?,
            });
        }
        d.expect(0, 5)?;
        d.expect(0, 0)?;
        d.finish()?;
        let view = Self {
            namespace_generation,
            refs,
            drafts,
            workspaces,
            retained,
        };
        view.validate()?;
        Ok(view)
    }
}
