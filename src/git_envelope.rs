//! Exact Git object body preservation. No Git semantic validation or import authority.
use crate::{ObjectId, codec, recovery::*};
use sha1::Digest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum GitKind {
    Blob = 1,
    Tree = 2,
    Commit = 3,
    Tag = 4,
}
impl GitKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Tree => "tree",
            Self::Commit => "commit",
            Self::Tag => "tag",
        }
    }
    fn read(n: u64) -> Result<Self, String> {
        match n {
            1 => Ok(Self::Blob),
            2 => Ok(Self::Tree),
            3 => Ok(Self::Commit),
            4 => Ok(Self::Tag),
            _ => Err("unknown Git object kind".into()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitObjectId {
    Sha1([u8; 20]),
    Sha256([u8; 32]),
}
impl GitObjectId {
    pub fn digest(&self) -> &[u8] {
        match self {
            Self::Sha1(b) => b,
            Self::Sha256(b) => b,
        }
    }
    pub fn algorithm(self) -> u64 {
        match self {
            Self::Sha1(_) => 1,
            Self::Sha256(_) => 2,
        }
    }
    pub fn hex(&self) -> String {
        self.digest().iter().map(|b| format!("{b:02x}")).collect()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitEnvelope {
    pub kind: GitKind,
    pub git_id: GitObjectId,
    pub body: Vec<u8>,
}
impl GitEnvelope {
    /// Construct an archival envelope; this does not certify valid Git history.
    pub fn new(kind: GitKind, algorithm: u64, body: Vec<u8>) -> Result<Self, String> {
        input_bound(body.len())?;
        let git_id = Self::hash(kind, algorithm, &body)?;
        let value = Self { kind, git_id, body };
        value.encode()?;
        Ok(value)
    }
    fn hash(kind: GitKind, algorithm: u64, body: &[u8]) -> Result<GitObjectId, String> {
        let header = format!("{} {}\0", kind.as_str(), body.len());
        match algorithm {
            1 => {
                let mut h = sha1::Sha1::new();
                h.update(header.as_bytes());
                h.update(body);
                Ok(GitObjectId::Sha1(h.finalize().into()))
            }
            2 => {
                let mut h = sha2::Sha256::new();
                h.update(header.as_bytes());
                h.update(body);
                Ok(GitObjectId::Sha256(h.finalize().into()))
            }
            _ => Err("unsupported Git hash algorithm".into()),
        }
    }
    pub fn verify_git_id(&self) -> Result<(), String> {
        input_bound(self.body.len())?;
        if Self::hash(self.kind, self.git_id.algorithm(), &self.body)? != self.git_id {
            return Err("Git object ID mismatch".into());
        }
        Ok(())
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.verify_git_id()?;
        let mut out = Vec::new();
        codec::head(&mut out, 5, 3);
        field(&mut out, 0);
        field(&mut out, self.kind as u64);
        field(&mut out, 1);
        codec::head(&mut out, 4, 3);
        field(&mut out, 0);
        field(&mut out, self.git_id.algorithm());
        codec::bytes(&mut out, self.git_id.digest());
        field(&mut out, 2);
        codec::bytes(&mut out, &self.body);
        codec::bounded(out, 12)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, _) = decoder(input, 12, 3, 3)?;
        d.expect(0, 0)?;
        let kind = GitKind::read(d.head(0)?)?;
        d.expect(0, 1)?;
        d.expect(4, 3)?;
        d.expect(0, 0)?;
        let git_id = match d.head(0)? {
            1 => GitObjectId::Sha1(fixed(&mut d)?),
            2 => GitObjectId::Sha256(fixed(&mut d)?),
            _ => return Err("unsupported Git hash algorithm".into()),
        };
        d.expect(0, 2)?;
        let body = d.bytes()?.to_vec();
        d.finish()?;
        let value = Self { kind, git_id, body };
        value.verify_git_id()?;
        Ok(value)
    }
}
