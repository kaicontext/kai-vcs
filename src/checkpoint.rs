use crate::{Identity, ObjectId, codec, recovery::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum CaptureClass {
    Observed = 1,
    BarrierConsistent = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum InclusionKind {
    Untracked = 1,
    Ignored = 2,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub workspace: [u8; 16],
    pub tree: ObjectId,
    pub selected_tree: ObjectId,
    pub capture: CaptureClass,
    pub actor: Identity,
    pub inclusion: Vec<(Vec<u8>, InclusionKind)>,
    pub base: Option<ObjectId>,
    pub conflicts: ConflictManifest,
    pub predecessor: Option<ObjectId>,
    pub barrier: Option<ObjectId>,
    pub selected_conflicts: ConflictManifest,
}
impl Checkpoint {
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        manifest_valid(&self.conflicts)?;
        manifest_valid(&self.selected_conflicts)?;
        if self.inclusion.iter().any(|(p, _)| !path(p))
            || !self.inclusion.windows(2).all(|w| w[0].0 < w[1].0)
        {
            return Err("invalid or unordered inclusion paths".into());
        }
        if self.barrier.is_some() && self.capture != CaptureClass::BarrierConsistent {
            return Err("collaboration barrier requires barrier-consistent capture".into());
        }
        let size = self
            .inclusion
            .iter()
            .map(|(p, _)| p.len().saturating_add(16))
            .chain(
                self.conflicts
                    .iter()
                    .chain(&self.selected_conflicts)
                    .map(|(p, _)| p.len().saturating_add(48)),
            )
            .fold(0usize, usize::saturating_add);
        input_bound(size)?;
        let mut out = Vec::new();
        let extras = [
            self.base.is_some(),
            !self.conflicts.is_empty(),
            self.predecessor.is_some(),
            self.barrier.is_some(),
            !self.selected_conflicts.is_empty(),
        ]
        .into_iter()
        .filter(|v| *v)
        .count() as u64;
        codec::head(&mut out, 5, 6 + extras);
        field(&mut out, 0);
        codec::bytes(&mut out, &self.workspace);
        field(&mut out, 1);
        codec::reference(&mut out, self.tree);
        field(&mut out, 2);
        codec::reference(&mut out, self.selected_tree);
        field(&mut out, 3);
        field(&mut out, self.capture as u64);
        field(&mut out, 4);
        self.actor.encode(&mut out)?;
        field(&mut out, 5);
        codec::head(&mut out, 4, self.inclusion.len() as u64);
        for (p, kind) in &self.inclusion {
            codec::head(&mut out, 4, 2);
            codec::bytes(&mut out, p);
            field(&mut out, *kind as u64);
        }
        if let Some(id) = self.base {
            field(&mut out, 6);
            codec::reference(&mut out, id);
        }
        if !self.conflicts.is_empty() {
            field(&mut out, 7);
            write_manifest(&mut out, &self.conflicts);
        }
        if let Some(id) = self.predecessor {
            field(&mut out, 8);
            codec::reference(&mut out, id);
        }
        if let Some(id) = self.barrier {
            field(&mut out, 9);
            codec::reference(&mut out, id);
        }
        if !self.selected_conflicts.is_empty() {
            field(&mut out, 10);
            write_manifest(&mut out, &self.selected_conflicts);
        }
        codec::bounded(out, 8)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, count) = decoder(input, 8, 6, 11)?;
        d.expect(0, 0)?;
        let workspace = fixed(&mut d)?;
        d.expect(0, 1)?;
        let tree = d.reference()?;
        d.expect(0, 2)?;
        let selected_tree = d.reference()?;
        d.expect(0, 3)?;
        let capture = match d.head(0)? {
            1 => CaptureClass::Observed,
            2 => CaptureClass::BarrierConsistent,
            _ => return Err("invalid capture class".into()),
        };
        d.expect(0, 4)?;
        let actor = Identity::decode(&mut d)?;
        d.expect(0, 5)?;
        let n = d.count(4, 4)?;
        let mut inclusion = Vec::new();
        for _ in 0..n {
            d.expect(4, 2)?;
            let p = d.bytes()?.to_vec();
            let kind = match d.head(0)? {
                1 => InclusionKind::Untracked,
                2 => InclusionKind::Ignored,
                _ => return Err("invalid inclusion kind".into()),
            };
            inclusion.push((p, kind));
        }
        let mut value = Self {
            workspace,
            tree,
            selected_tree,
            capture,
            actor,
            inclusion,
            base: None,
            conflicts: vec![],
            predecessor: None,
            barrier: None,
            selected_conflicts: vec![],
        };
        let mut last = 5;
        for _ in 6..count {
            match next_key(&mut d, &mut last)? {
                6 => value.base = Some(d.reference()?),
                7 => value.conflicts = read_manifest(&mut d)?,
                8 => value.predecessor = Some(d.reference()?),
                9 => value.barrier = Some(d.reference()?),
                10 => value.selected_conflicts = read_manifest(&mut d)?,
                _ => return Err("unknown checkpoint field".into()),
            }
        }
        d.finish()?;
        value.encode()?;
        Ok(value)
    }
}
