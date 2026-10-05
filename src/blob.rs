pub struct Blob {
    pub data: Vec<u8>,
}

impl Blob {
    pub fn id(&self) -> String {
        let encoded = self.encode();
        blake3::hash(&encoded).to_hex().to_string()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();

        out.extend_from_slice(b"KV");
        out.push(0x01);

        let len = self.data.len() as u64;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&self.data);

        out
    }

    pub fn decode(input: &[u8]) -> Result<Blob, String> {
        if input.len() < 11 {
            return Err("input too short".to_string());
        }

        if &input[0..2] != b"KV" {
            return Err("invalid magic".to_string());
        }

        if input[2] != 0x01 {
            return Err("not a blob".to_string());
        }

        let len_bytes: [u8; 8] = input[3..11]
            .try_into()
            .map_err(|_| "invalid length bytes".to_string())?;

        let len = u64::from_le_bytes(len_bytes) as usize;

        if input.len() != 11 + len {
            return Err("length mismatch".to_string());
        }

        let data = input[11..].to_vec();

        Ok(Blob { data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_encoding_is_canonical() {
        let blob = Blob {
            data: b"hello\n".to_vec(),
        };

        let encoded = blob.encode();

        let expected = vec![
            0x4b, 0x56,
            0x01,
            0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x0a,
        ];

        assert_eq!(encoded, expected);
    }

    #[test]
    fn blob_round_trip() {
        let original = Blob {
            data: b"hello\n".to_vec(),
        };

        let encoded = original.encode();
        let decoded = Blob::decode(&encoded).unwrap();

        assert_eq!(decoded.data, original.data);
    }

    #[test]
    fn same_blob_has_same_id() {
        let a = Blob {
            data: b"hello\n".to_vec(),
        };

        let b = Blob {
            data: b"hello\n".to_vec(),
        };

        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn blob_id_is_canonical() {
        let blob = Blob {
            data: b"hello\n".to_vec(),
        };

        assert_eq!(blob.id(), "a7ec3141ecd67b1c4efa18758e52bda8f45b07ce1c1758e10a755f81f8b235bd");
    }

    #[test]
    fn reject_bad_magic() {
        let input = b"XX\x01\x00\x00\x00\x00\x00\x00\x00\x00";

        assert!(Blob::decode(input).is_err());
    }

    #[test]
    fn reject_wrong_type() {
        let input = b"KV\x02\x00\x00\x00\x00\x00\x00\x00\x00";

        assert!(Blob::decode(input).is_err());
    }

    #[test]
    fn reject_length_mismatch() {
        let input = b"KV\x01\x05\x00\x00\x00\x00\x00\x00\x00hi";

        assert!(Blob::decode(input).is_err());
    }
}