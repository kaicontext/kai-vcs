use crate::{Identity, ObjectId, codec, recovery::*};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operation {
    pub operation_id: [u8; 16],
    pub predecessors: Vec<ObjectId>,
    pub pre_view: ObjectId,
    pub post_view: ObjectId,
    pub action: Vec<u8>,
    pub actor: Identity,
    pub started_seconds: i64,
    pub finished_seconds: i64,
    pub workspaces: Vec<[u8; 16]>,
    pub intent_id: [u8; 16],
    pub external_receipts: Vec<Vec<u8>>,
}
impl Operation {
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        unique_refs(&self.predecessors)?;
        nonempty(&self.action)?;
        if !ordered(&self.workspaces) {
            return Err("unordered or duplicate workspace ID".into());
        }
        let mut size = self
            .action
            .len()
            .saturating_add(self.predecessors.len().saturating_mul(37))
            .saturating_add(self.workspaces.len().saturating_mul(17));
        for receipt in &self.external_receipts {
            nonempty(receipt)?;
            size = size.saturating_add(receipt.len()).saturating_add(9);
        }
        input_bound(size)?;
        let mut out = Vec::new();
        codec::head(
            &mut out,
            5,
            10 + u64::from(!self.external_receipts.is_empty()),
        );
        field(&mut out, 0);
        codec::bytes(&mut out, &self.operation_id);
        field(&mut out, 1);
        write_refs(&mut out, &self.predecessors);
        field(&mut out, 2);
        codec::reference(&mut out, self.pre_view);
        field(&mut out, 3);
        codec::reference(&mut out, self.post_view);
        field(&mut out, 4);
        codec::bytes(&mut out, &self.action);
        field(&mut out, 5);
        self.actor.encode(&mut out)?;
        field(&mut out, 6);
        codec::signed(&mut out, self.started_seconds);
        field(&mut out, 7);
        codec::signed(&mut out, self.finished_seconds);
        field(&mut out, 8);
        codec::head(&mut out, 4, self.workspaces.len() as u64);
        for workspace in &self.workspaces {
            codec::bytes(&mut out, workspace);
        }
        field(&mut out, 9);
        codec::bytes(&mut out, &self.intent_id);
        if !self.external_receipts.is_empty() {
            field(&mut out, 10);
            codec::head(&mut out, 4, self.external_receipts.len() as u64);
            for receipt in &self.external_receipts {
                codec::bytes(&mut out, receipt);
            }
        }
        codec::bounded(out, 7)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, count) = decoder(input, 7, 10, 11)?;
        d.expect(0, 0)?;
        let operation_id = fixed(&mut d)?;
        d.expect(0, 1)?;
        let predecessors = read_refs(&mut d)?;
        d.expect(0, 2)?;
        let pre_view = d.reference()?;
        d.expect(0, 3)?;
        let post_view = d.reference()?;
        d.expect(0, 4)?;
        let action = d.bytes()?.to_vec();
        d.expect(0, 5)?;
        let actor = Identity::decode(&mut d)?;
        d.expect(0, 6)?;
        let started_seconds = d.signed()?;
        d.expect(0, 7)?;
        let finished_seconds = d.signed()?;
        d.expect(0, 8)?;
        let n = d.count(4, 17)?;
        let workspaces = (0..n).map(|_| fixed(&mut d)).collect::<Result<_, _>>()?;
        d.expect(0, 9)?;
        let intent_id = fixed(&mut d)?;
        let mut external_receipts = Vec::new();
        if count == 11 {
            d.expect(0, 10)?;
            let n = d.count(4, 2)?;
            if n == 0 {
                return Err("empty optional external receipts".into());
            }
            for _ in 0..n {
                external_receipts.push(d.bytes()?.to_vec());
            }
        }
        d.finish()?;
        let value = Self {
            operation_id,
            predecessors,
            pre_view,
            post_view,
            action,
            actor,
            started_seconds,
            finished_seconds,
            workspaces,
            intent_id,
            external_receipts,
        };
        value.encode()?;
        Ok(value)
    }
}
