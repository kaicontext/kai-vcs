use crate::{
    ObjectId,
    codec::{self, Decoder},
    tree::valid_name,
};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub name: Vec<u8>,
    pub email: Vec<u8>,
    pub timestamp_seconds: i64,
    pub timezone_minutes: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision {
    pub tree: ObjectId,
    pub parents: Vec<ObjectId>,
    pub change_id: [u8; 16],
    pub author: Identity,
    pub committer: Identity,
    pub message: Vec<u8>,
    pub provenance: Vec<ObjectId>,
    pub git_envelope: Option<ObjectId>,
    pub conflicts: Vec<(Vec<u8>, ObjectId)>,
}

impl Identity {
    pub(crate) fn encode(&self, out: &mut Vec<u8>) -> Result<(), String> {
        if !(-1439..=1439).contains(&self.timezone_minutes) {
            return Err("invalid timezone offset".into());
        }
        if self.name.len().saturating_add(self.email.len()) > codec::STRUCTURED_LIMIT {
            return Err("identity exceeds resource limit".into());
        }
        codec::head(out, 5, 4);
        codec::head(out, 0, 0);
        codec::bytes(out, &self.name);
        codec::head(out, 0, 1);
        codec::bytes(out, &self.email);
        codec::head(out, 0, 2);
        codec::signed(out, self.timestamp_seconds);
        codec::head(out, 0, 3);
        codec::signed(out, i64::from(self.timezone_minutes));
        Ok(())
    }
    pub(crate) fn decode(d: &mut Decoder<'_>) -> Result<Self, String> {
        d.expect(5, 4)?;
        d.expect(0, 0)?;
        let name = d.bytes()?.to_vec();
        d.expect(0, 1)?;
        let email = d.bytes()?.to_vec();
        d.expect(0, 2)?;
        let timestamp_seconds = d.signed()?;
        d.expect(0, 3)?;
        let timezone_minutes = i16::try_from(d.signed()?).map_err(|_| "timezone overflow")?;
        if !(-1439..=1439).contains(&timezone_minutes) {
            return Err("invalid timezone offset".into());
        }
        Ok(Self {
            name,
            email,
            timestamp_seconds,
            timezone_minutes,
        })
    }
}

fn references(d: &mut Decoder<'_>) -> Result<Vec<ObjectId>, String> {
    let count = d.count(4, 37)?;
    (0..count).map(|_| d.reference()).collect()
}

fn encode_references(out: &mut Vec<u8>, ids: &[ObjectId]) {
    codec::head(out, 4, ids.len() as u64);
    for id in ids {
        codec::reference(out, *id);
    }
}

impl Revision {
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.message.len() > codec::STRUCTURED_LIMIT
            || self.parents.len() > codec::STRUCTURED_LIMIT / 37
            || self.provenance.len() > codec::STRUCTURED_LIMIT / 37
        {
            return Err("revision exceeds resource limit".into());
        }
        let mut unique = HashSet::new();
        if self.parents.iter().any(|p| !unique.insert(p)) {
            return Err("duplicate revision parent".into());
        }
        let mut out = Vec::new();
        let extra = u64::from(!self.provenance.is_empty())
            + u64::from(self.git_envelope.is_some())
            + u64::from(!self.conflicts.is_empty());
        codec::head(&mut out, 5, 6 + extra);
        codec::head(&mut out, 0, 0);
        codec::reference(&mut out, self.tree);
        codec::head(&mut out, 0, 1);
        encode_references(&mut out, &self.parents);
        codec::head(&mut out, 0, 2);
        codec::bytes(&mut out, &self.change_id);
        codec::head(&mut out, 0, 3);
        self.author.encode(&mut out)?;
        codec::head(&mut out, 0, 4);
        self.committer.encode(&mut out)?;
        codec::head(&mut out, 0, 5);
        codec::bytes(&mut out, &self.message);
        if !self.provenance.is_empty() {
            codec::head(&mut out, 0, 6);
            encode_references(&mut out, &self.provenance);
        }
        if let Some(id) = self.git_envelope {
            codec::head(&mut out, 0, 7);
            codec::reference(&mut out, id);
        }
        if !self.conflicts.is_empty() {
            let mut sorted: Vec<_> = self.conflicts.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            codec::head(&mut out, 0, 8);
            codec::head(&mut out, 4, sorted.len() as u64);
            let mut previous: Option<&[u8]> = None;
            for (path, id) in sorted {
                if !path.split(|b| *b == b'/').all(valid_name) {
                    return Err("invalid conflict path".into());
                }
                if previous == Some(path.as_slice()) {
                    return Err("duplicate conflict path".into());
                }
                previous = Some(path);
                if path.len() > codec::STRUCTURED_LIMIT {
                    return Err("conflict path exceeds resource limit".into());
                }
                codec::head(&mut out, 4, 2);
                codec::bytes(&mut out, path);
                codec::reference(&mut out, *id);
                if out.len() > codec::STRUCTURED_LIMIT {
                    return Err("revision exceeds resource limit".into());
                }
            }
        }
        codec::bounded(out, 3)
    }

    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (kind, payload) = codec::unframe(input)?;
        if kind != 3 {
            return Err("not a revision".into());
        }
        let mut d = Decoder::new(payload);
        let count = d.head(5)?;
        if !(6..=9).contains(&count) {
            return Err("invalid revision field count".into());
        }
        d.expect(0, 0)?;
        let tree = d.reference()?;
        d.expect(0, 1)?;
        let parents = references(&mut d)?;
        d.expect(0, 2)?;
        let change_id = d
            .bytes()?
            .try_into()
            .map_err(|_| "invalid change ID length")?;
        d.expect(0, 3)?;
        let author = Identity::decode(&mut d)?;
        d.expect(0, 4)?;
        let committer = Identity::decode(&mut d)?;
        d.expect(0, 5)?;
        let message = d.bytes()?.to_vec();
        let mut revision = Self {
            tree,
            parents,
            change_id,
            author,
            committer,
            message,
            provenance: vec![],
            git_envelope: None,
            conflicts: vec![],
        };
        let mut last_key = 5;
        for _ in 6..count {
            let key = d.head(0)?;
            if key <= last_key {
                return Err("duplicate or unordered revision key".into());
            }
            last_key = key;
            match key {
                6 => {
                    revision.provenance = references(&mut d)?;
                    if revision.provenance.is_empty() {
                        return Err("empty optional provenance must be omitted".into());
                    }
                }
                7 => revision.git_envelope = Some(d.reference()?),
                8 => {
                    let count = d.count(4, 40)?;
                    if count == 0 {
                        return Err("empty optional conflicts must be omitted".into());
                    }
                    for _ in 0..count {
                        d.expect(4, 2)?;
                        let path = d.bytes()?.to_vec();
                        if revision.conflicts.last().is_some_and(|prev| prev.0 >= path) {
                            return Err("conflict paths must be strictly sorted".into());
                        }
                        revision.conflicts.push((path, d.reference()?));
                    }
                }
                _ => return Err("unknown revision field".into()),
            }
        }
        d.finish()?;
        revision.encode()?;
        Ok(revision)
    }
}
