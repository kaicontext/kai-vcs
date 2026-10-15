//! Declarative control records. No policy authority or live writer coordination.
use crate::{
    ObjectId,
    codec::{self, Decoder},
    recovery::*,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollaborationBarrier {
    pub workspace: [u8; 16],
    pub epoch: [u8; 16],
    pub generation: u64,
    pub tree: ObjectId,
    pub protocol: Vec<u8>,
    pub frontier: Vec<([u8; 16], u64)>,
    pub excluded: Vec<([u8; 16], Vec<u8>)>,
    pub manifest: ObjectId,
    pub manifest_format: Vec<u8>,
}
impl CollaborationBarrier {
    fn validate(&self) -> Result<(), String> {
        nonempty(&self.protocol)?;
        nonempty(&self.manifest_format)?;
        if self.frontier.is_empty()
            || !self.frontier.windows(2).all(|w| w[0].0 < w[1].0)
            || !self.excluded.windows(2).all(|w| w[0].0 < w[1].0)
            || self.excluded.iter().any(|(id, reason)| {
                reason.is_empty() || self.frontier.binary_search_by_key(id, |r| r.0).is_ok()
            })
        {
            return Err("invalid, duplicate or overlapping barrier replicas".into());
        }
        Ok(())
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        input_bound(
            self.excluded.iter().fold(
                self.frontier
                    .len()
                    .saturating_mul(19)
                    .saturating_add(self.protocol.len())
                    .saturating_add(self.manifest_format.len()),
                |n, (_, b)| n.saturating_add(b.len()).saturating_add(19),
            ),
        )?;
        self.validate()?;
        let mut out = Vec::new();
        codec::head(&mut out, 5, 9);
        field(&mut out, 0);
        codec::bytes(&mut out, &self.workspace);
        field(&mut out, 1);
        codec::bytes(&mut out, &self.epoch);
        field(&mut out, 2);
        field(&mut out, self.generation);
        field(&mut out, 3);
        codec::reference(&mut out, self.tree);
        field(&mut out, 4);
        codec::bytes(&mut out, &self.protocol);
        field(&mut out, 5);
        codec::head(&mut out, 4, self.frontier.len() as u64);
        for (id, n) in &self.frontier {
            codec::head(&mut out, 4, 2);
            codec::bytes(&mut out, id);
            field(&mut out, *n);
        }
        field(&mut out, 6);
        codec::head(&mut out, 4, self.excluded.len() as u64);
        for (id, reason) in &self.excluded {
            codec::head(&mut out, 4, 2);
            codec::bytes(&mut out, id);
            codec::bytes(&mut out, reason);
        }
        field(&mut out, 7);
        codec::reference(&mut out, self.manifest);
        field(&mut out, 8);
        codec::bytes(&mut out, &self.manifest_format);
        codec::bounded(out, 17)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, _) = decoder(input, 17, 9, 9)?;
        d.expect(0, 0)?;
        let workspace = fixed(&mut d)?;
        d.expect(0, 1)?;
        let epoch = fixed(&mut d)?;
        d.expect(0, 2)?;
        let generation = d.head(0)?;
        d.expect(0, 3)?;
        let tree = d.reference()?;
        d.expect(0, 4)?;
        let protocol = d.bytes()?.to_vec();
        d.expect(0, 5)?;
        let n = d.count(4, 19)?;
        let mut frontier = Vec::new();
        for _ in 0..n {
            d.expect(4, 2)?;
            frontier.push((fixed(&mut d)?, d.head(0)?));
        }
        d.expect(0, 6)?;
        let n = d.count(4, 20)?;
        let mut excluded = Vec::new();
        for _ in 0..n {
            d.expect(4, 2)?;
            excluded.push((fixed(&mut d)?, d.bytes()?.to_vec()));
        }
        d.expect(0, 7)?;
        let manifest = d.reference()?;
        d.expect(0, 8)?;
        let manifest_format = d.bytes()?.to_vec();
        d.finish()?;
        let v = Self {
            workspace,
            epoch,
            generation,
            tree,
            protocol,
            frontier,
            excluded,
            manifest,
            manifest_format,
        };
        v.validate()?;
        Ok(v)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum SelectiveCi {
    Disabled = 1,
    FullOnUnknown = 2,
    RejectUnknown = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum RewritePolicy {
    Deny = 1,
    Administrative = 2,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub repository: [u8; 16],
    pub revision: u64,
    pub protected_refs: Vec<Vec<u8>>,
    pub required_checks: Vec<Vec<u8>>,
    pub required_reviewers: u64,
    pub allowed_principals: Vec<Vec<u8>>,
    pub selective_ci: SelectiveCi,
    pub rewrite: RewritePolicy,
    pub required_features: Vec<Vec<u8>>,
    pub predecessor: Option<ObjectId>,
}
fn write_labels(out: &mut Vec<u8>, list: &[Vec<u8>]) {
    codec::head(out, 4, list.len() as u64);
    for b in list {
        codec::bytes(out, b);
    }
}
fn read_labels(d: &mut Decoder<'_>) -> Result<Vec<Vec<u8>>, String> {
    let n = d.count(4, 2)?;
    (0..n).map(|_| Ok(d.bytes()?.to_vec())).collect()
}
impl Policy {
    fn validate(&self) -> Result<(), String> {
        if self.revision == 0 {
            return Err("policy revision must be positive".into());
        }
        for list in [
            &self.protected_refs,
            &self.required_checks,
            &self.allowed_principals,
            &self.required_features,
        ] {
            if list.iter().any(Vec::is_empty) || !ordered(list) {
                return Err("invalid policy label ordering".into());
            }
        }
        for name in &self.protected_refs {
            let name = std::str::from_utf8(name).map_err(|_| "policy ref must be UTF-8")?;
            crate::view::ref_types(name)?;
            if !name.starts_with("branches/") && !name.starts_with("tags/") {
                return Err("unsupported policy ref namespace".into());
            }
        }
        Ok(())
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        input_bound(
            self.protected_refs
                .iter()
                .chain(&self.required_checks)
                .chain(&self.allowed_principals)
                .chain(&self.required_features)
                .fold(0usize, |n, b| n.saturating_add(b.len()).saturating_add(1)),
        )?;
        self.validate()?;
        let mut out = Vec::new();
        codec::head(&mut out, 5, 9 + u64::from(self.predecessor.is_some()));
        field(&mut out, 0);
        codec::bytes(&mut out, &self.repository);
        field(&mut out, 1);
        field(&mut out, self.revision);
        field(&mut out, 2);
        write_labels(&mut out, &self.protected_refs);
        field(&mut out, 3);
        write_labels(&mut out, &self.required_checks);
        field(&mut out, 4);
        field(&mut out, self.required_reviewers);
        field(&mut out, 5);
        write_labels(&mut out, &self.allowed_principals);
        field(&mut out, 6);
        field(&mut out, self.selective_ci as u64);
        field(&mut out, 7);
        field(&mut out, self.rewrite as u64);
        field(&mut out, 8);
        write_labels(&mut out, &self.required_features);
        if let Some(id) = self.predecessor {
            field(&mut out, 9);
            codec::reference(&mut out, id);
        }
        codec::bounded(out, 18)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, n) = decoder(input, 18, 9, 10)?;
        d.expect(0, 0)?;
        let repository = fixed(&mut d)?;
        d.expect(0, 1)?;
        let revision = d.head(0)?;
        d.expect(0, 2)?;
        let protected_refs = read_labels(&mut d)?;
        d.expect(0, 3)?;
        let required_checks = read_labels(&mut d)?;
        d.expect(0, 4)?;
        let required_reviewers = d.head(0)?;
        d.expect(0, 5)?;
        let allowed_principals = read_labels(&mut d)?;
        d.expect(0, 6)?;
        let selective_ci = match d.head(0)? {
            1 => SelectiveCi::Disabled,
            2 => SelectiveCi::FullOnUnknown,
            3 => SelectiveCi::RejectUnknown,
            _ => return Err("unknown selective CI mode".into()),
        };
        d.expect(0, 7)?;
        let rewrite = match d.head(0)? {
            1 => RewritePolicy::Deny,
            2 => RewritePolicy::Administrative,
            _ => return Err("unknown rewrite policy".into()),
        };
        d.expect(0, 8)?;
        let required_features = read_labels(&mut d)?;
        let predecessor = if n == 10 {
            d.expect(0, 9)?;
            Some(d.reference()?)
        } else {
            None
        };
        d.finish()?;
        let v = Self {
            repository,
            revision,
            protected_refs,
            required_checks,
            required_reviewers,
            allowed_principals,
            selective_ci,
            rewrite,
            required_features,
            predecessor,
        };
        v.validate()?;
        Ok(v)
    }
}
