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

        out.extend_from_slice(b"KAI-OBJECT");
        out.push(0x00);
        let one:u16 = 1;
        out.extend_from_slice(&one.to_be_bytes());
        out.extend_from_slice(&one.to_be_bytes());

        let len = self.data.len() as u64;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.data);

        out
    }

    pub fn decode(input: &[u8]) -> Result<Blob, String> {
        if input.len() < 23 {
            return Err("input too short".to_string());
        }

        if &input[0..11] != b"KAI-OBJECT\0" {
            return Err("invalid magic".to_string());
        }

        let version: [u8; 2] = input[11..13]
            .try_into()
            .map_err(|_| "invalid version bytes".to_string())?;

        if u16::from_be_bytes(version) != 1 {
            return Err("invalid version value".to_string());
        }

        let blob_type: [u8; 2] = input[13..15]
            .try_into()
            .map_err(|_| "invalid blob type".to_string())?;

        if u16::from_be_bytes(blob_type) != 1 {
            return Err("invalid blob value".to_string());
        }

        let len_bytes: [u8; 8] = input[15..23]
            .try_into()
            .map_err(|_| "invalid length bytes".to_string())?;

        let len = usize::try_from(u64::from_be_bytes(len_bytes))
            .map_err(|_| "invalid length".to_string())?;

        if input.len() - 23 != len {
            return Err("length mismatch".to_string());
        }

        let data = input[23..].to_vec();

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
            0x4b, 0x41, 0x49, 0x2d, 0x4f, 0x42, 0x4a, 0x45, 0x43, 0x54,
            0x00,
            0x00, 0x01,
            0x00, 0x01,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06,
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

        assert_eq!(blob.id(), "153979b75f125db760f3432123db4fc1ecaa50f80a02d3a9b5020424eb081aeb");
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