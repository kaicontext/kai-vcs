use crate::{
    ObjectId,
    codec::{self, Decoder},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LargeBlob {
    pub length: u64,
    pub content_digest: [u8; 32],
    pub chunks: Vec<ObjectId>,
}

impl LargeBlob {
    fn validate(&self) -> Result<(), String> {
        if self.length <= codec::BLOB_LIMIT as u64 {
            return Err("large blob must exceed 16 MiB".into());
        }
        let count = self.length.div_ceil(codec::CHUNK_SIZE as u64);
        if count != self.chunks.len() as u64 {
            return Err("large blob chunk count mismatch".into());
        }
        if self.chunks.len() > codec::STRUCTURED_LIMIT / 37 {
            return Err("large blob manifest exceeds resource limit".into());
        }
        Ok(())
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut out = Vec::new();
        codec::head(&mut out, 5, 3);
        codec::head(&mut out, 0, 0);
        codec::head(&mut out, 0, self.length);
        codec::head(&mut out, 0, 1);
        codec::bytes(&mut out, &self.content_digest);
        codec::head(&mut out, 0, 2);
        codec::head(&mut out, 4, self.chunks.len() as u64);
        for id in &self.chunks {
            codec::reference(&mut out, *id);
        }
        codec::bounded(out, 14)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (kind, payload) = codec::unframe(input)?;
        if kind != 14 {
            return Err("not a large blob".into());
        }
        let mut d = Decoder::new(payload);
        d.expect(5, 3)?;
        d.expect(0, 0)?;
        let length = d.head(0)?;
        d.expect(0, 1)?;
        let content_digest = d
            .bytes()?
            .try_into()
            .map_err(|_| "invalid content digest length")?;
        d.expect(0, 2)?;
        let count = d.count(4, 37)?;
        let chunks = (0..count)
            .map(|_| d.reference())
            .collect::<Result<_, _>>()?;
        d.finish()?;
        let value = Self {
            length,
            content_digest,
            chunks,
        };
        value.validate()?;
        Ok(value)
    }

    /// Verify assembly while holding only one framed chunk at a time.
    /// The provider must bound its own input allocation. No output is published.
    pub fn verify_content(
        &self,
        mut read: impl FnMut(ObjectId) -> Result<Vec<u8>, String>,
    ) -> Result<(), String> {
        self.validate()?;
        let mut hash = blake3::Hasher::new();
        for (index, id) in self.chunks.iter().enumerate() {
            let encoded = read(*id)?;
            let (kind, payload) = codec::unframe(&encoded)?;
            if ObjectId::hash(&encoded) != *id {
                return Err(format!("chunk {index}: object ID mismatch"));
            }
            if kind != 13 {
                return Err(format!("chunk {index}: expected chunk type"));
            }
            let expected = if index + 1 == self.chunks.len() {
                ((self.length - 1) % codec::CHUNK_SIZE as u64 + 1) as usize
            } else {
                codec::CHUNK_SIZE
            };
            if payload.len() != expected {
                return Err(format!("chunk {index}: length mismatch"));
            }
            hash.update(payload);
        }
        if hash.finalize().as_bytes() != &self.content_digest {
            return Err("assembled content digest mismatch".into());
        }
        Ok(())
    }
}
