use crate::{ObjectId, codec};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blob {
    pub data: Vec<u8>,
}

impl Blob {
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.data.len() > codec::BLOB_LIMIT {
            return Err("blob exceeds 16 MiB; use a large-blob manifest".into());
        }
        Ok(codec::frame(1, &self.data))
    }

    pub fn decode(input: &[u8]) -> Result<Blob, String> {
        let (kind, payload) = codec::unframe(input)?;
        if kind != 1 {
            return Err("invalid blob value".into());
        }
        Ok(Blob {
            data: payload.to_vec(),
        })
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

        let encoded = blob.encode().unwrap();

        let expected = vec![
            0x4b, 0x41, 0x49, 0x2d, 0x4f, 0x42, 0x4a, 0x45, 0x43, 0x54, 0x00, 0x00, 0x01, 0x00,
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x68, 0x65, 0x6c, 0x6c, 0x6f,
            0x0a,
        ];

        assert_eq!(encoded, expected);
    }

    #[test]
    fn blob_round_trip() {
        let original = Blob {
            data: b"hello\n".to_vec(),
        };

        let encoded = original.encode().unwrap();
        let decoded = Blob::decode(&encoded).unwrap();

        assert_eq!(decoded.data, original.data);
    }

    #[test]
    fn binary_blob_round_trip() {
        let blob = Blob {
            data: (0u8..=255).collect(),
        };

        let encoded = blob.encode().unwrap();
        let decoded = Blob::decode(&encoded).unwrap();

        assert_eq!(decoded.data, blob.data);
    }

    #[test]
    fn reject_truncated_header() {
        let blob = Blob { data: Vec::new() };
        let encoded = blob.encode().unwrap();

        for length in 0..23 {
            let error =
                Blob::decode(&encoded[..length]).expect_err("expected truncated-header rejection");
            assert_eq!(error, "input too short", "header length {length}");
        }
    }

    #[test]
    fn empty_blob_round_trip() {
        let blob = Blob { data: Vec::new() };

        let encoded = blob.encode().unwrap();
        assert_eq!(encoded.len(), 23);

        let decoded = Blob::decode(&encoded).unwrap();
        assert!(decoded.data.is_empty());
    }

    #[test]
    fn same_blob_has_same_id() {
        let a = Blob {
            data: b"hello\n".to_vec(),
        };

        let b = Blob {
            data: b"hello\n".to_vec(),
        };

        assert_eq!(a.id().unwrap(), b.id().unwrap());
    }

    #[test]
    fn blob_id_is_canonical() {
        let blob = Blob {
            data: b"hello\n".to_vec(),
        };

        assert_eq!(
            blob.id().unwrap().to_string(),
            "k1:b3:153979b75f125db760f3432123db4fc1ecaa50f80a02d3a9b5020424eb081aeb"
        );
    }

    #[test]
    fn reject_bad_magic() {
        let input = b"XAI-OBJECT\x00\x00\x01\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00";

        assert!(Blob::decode(input).is_err());
        assert_eq!(
            Blob::decode(input).expect_err("expected magic rejection"),
            "invalid magic".to_string()
        );
    }

    #[test]
    fn reject_wrong_type() {
        // - KAI-OBJECT marker and zero byte.
        // - Version 1: 00 01.
        // - Type 2: 00 02—a tree, rather than a blob.
        // - Payload length 0: eight zero bytes.
        // - No payload.
        let input = b"KAI-OBJECT\x00\x00\x01\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00";
        assert!(Blob::decode(input).is_err());
        assert_eq!(
            Blob::decode(input).expect_err("expected wrong-type rejection"),
            "invalid blob value".to_string()
        );
    }

    #[test]
    fn reject_wrong_version() {
        // - KAI-OBJECT marker and zero byte.
        // - Version 2: 00 02 (unsupported).
        // - Type 1: 00 01—a blob.
        // - Payload length 0: eight zero bytes.
        // - No payload.
        let input = b"KAI-OBJECT\x00\x00\x02\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00";
        assert!(Blob::decode(input).is_err());
        assert_eq!(
            Blob::decode(input).expect_err("expected wrong-version rejection"),
            "invalid version value".to_string()
        );
    }

    #[test]
    fn reject_length_mismatch() {
        let input = b"KAI-OBJECT\x00\x00\x01\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00hi";

        assert!(Blob::decode(input).is_err());
        assert_eq!(
            Blob::decode(input).expect_err("expected length rejection"),
            "length mismatch".to_string()
        );
    }
}
