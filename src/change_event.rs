use crate::{Identity, ObjectId, codec, recovery::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum ChangeKind {
    Create = 1,
    Rewrite = 2,
    Split = 3,
    Squash = 4,
    Abandon = 5,
    Revive = 6,
    CherryPick = 7,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeEvent {
    pub kind: ChangeKind,
    pub change_ids: Vec<[u8; 16]>,
    pub prior: Vec<ObjectId>,
    pub resulting: Vec<ObjectId>,
    pub actor: Identity,
    pub reason: Vec<u8>,
    pub operation: Option<ObjectId>,
}
impl ChangeEvent {
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.change_ids.is_empty() || !ordered(&self.change_ids) {
            return Err("empty, unordered or duplicate change IDs".into());
        }
        unique_refs(&self.prior)?;
        unique_refs(&self.resulting)?;
        let valid = match self.kind {
            ChangeKind::Create => self.prior.is_empty() && !self.resulting.is_empty(),
            ChangeKind::Abandon => !self.prior.is_empty() && self.resulting.is_empty(),
            ChangeKind::Revive => !self.resulting.is_empty(),
            _ => !self.prior.is_empty() && !self.resulting.is_empty(),
        };
        if !valid {
            return Err("change event kind/revision cardinality mismatch".into());
        }
        input_bound(
            self.reason
                .len()
                .saturating_add(self.change_ids.len().saturating_mul(17))
                .saturating_add(
                    self.prior
                        .len()
                        .saturating_add(self.resulting.len())
                        .saturating_mul(37),
                ),
        )?;
        let mut out = Vec::new();
        codec::head(&mut out, 5, 6 + u64::from(self.operation.is_some()));
        field(&mut out, 0);
        field(&mut out, self.kind as u64);
        field(&mut out, 1);
        codec::head(&mut out, 4, self.change_ids.len() as u64);
        for change in &self.change_ids {
            codec::bytes(&mut out, change);
        }
        field(&mut out, 2);
        write_refs(&mut out, &self.prior);
        field(&mut out, 3);
        write_refs(&mut out, &self.resulting);
        field(&mut out, 4);
        self.actor.encode(&mut out)?;
        field(&mut out, 5);
        codec::bytes(&mut out, &self.reason);
        if let Some(id) = self.operation {
            field(&mut out, 6);
            codec::reference(&mut out, id);
        }
        codec::bounded(out, 4)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, count) = decoder(input, 4, 6, 7)?;
        d.expect(0, 0)?;
        let kind = match d.head(0)? {
            1 => ChangeKind::Create,
            2 => ChangeKind::Rewrite,
            3 => ChangeKind::Split,
            4 => ChangeKind::Squash,
            5 => ChangeKind::Abandon,
            6 => ChangeKind::Revive,
            7 => ChangeKind::CherryPick,
            _ => return Err("unknown change event kind".into()),
        };
        d.expect(0, 1)?;
        let n = d.count(4, 17)?;
        let change_ids = (0..n).map(|_| fixed(&mut d)).collect::<Result<_, _>>()?;
        d.expect(0, 2)?;
        let prior = read_refs(&mut d)?;
        d.expect(0, 3)?;
        let resulting = read_refs(&mut d)?;
        d.expect(0, 4)?;
        let actor = Identity::decode(&mut d)?;
        d.expect(0, 5)?;
        let reason = d.bytes()?.to_vec();
        let operation = if count == 7 {
            d.expect(0, 6)?;
            Some(d.reference()?)
        } else {
            None
        };
        d.finish()?;
        let value = Self {
            kind,
            change_ids,
            prior,
            resulting,
            actor,
            reason,
            operation,
        };
        value.encode()?;
        Ok(value)
    }
}
