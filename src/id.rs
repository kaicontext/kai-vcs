use std::{fmt, str::FromStr};

/// An algorithm-qualified native v1 BLAKE3 object identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectId([u8; 32]);

impl ObjectId {
    pub fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn hash(encoded: &[u8]) -> Self {
        Self(*blake3::hash(encoded).as_bytes())
    }
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "k1:b3:{}", self.hex())
    }
}

impl FromStr for ObjectId {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let hex = value
            .strip_prefix("k1:b3:")
            .ok_or("expected k1:b3: object ID")?;
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("expected 64 lowercase hexadecimal digits".into());
        }
        let mut digest = [0; 32];
        for (i, byte) in digest.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
        }
        Ok(Self(digest))
    }
}
