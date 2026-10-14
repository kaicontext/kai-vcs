use crate::{Identity, ObjectId, codec, recovery::*};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub target: ObjectId,
    pub message: Vec<u8>,
    pub tagger: Identity,
    pub git_envelope: Option<ObjectId>,
}
impl Tag {
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        input_bound(self.message.len())?;
        let mut out = Vec::new();
        codec::head(&mut out, 5, 3 + u64::from(self.git_envelope.is_some()));
        field(&mut out, 0);
        codec::reference(&mut out, self.target);
        field(&mut out, 1);
        codec::bytes(&mut out, &self.message);
        field(&mut out, 2);
        self.tagger.encode(&mut out)?;
        if let Some(id) = self.git_envelope {
            field(&mut out, 3);
            codec::reference(&mut out, id);
        }
        codec::bounded(out, 15)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, count) = decoder(input, 15, 3, 4)?;
        d.expect(0, 0)?;
        let target = d.reference()?;
        d.expect(0, 1)?;
        let message = d.bytes()?.to_vec();
        d.expect(0, 2)?;
        let tagger = Identity::decode(&mut d)?;
        let git_envelope = if count == 4 {
            d.expect(0, 3)?;
            Some(d.reference()?)
        } else {
            None
        };
        d.finish()?;
        Ok(Self {
            target,
            message,
            tagger,
            git_envelope,
        })
    }
}
