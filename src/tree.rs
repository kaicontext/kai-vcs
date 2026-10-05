#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    Blob,
    Tree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub name: String,
    pub kind: EntryKind,
    pub id: [u8; 32],
}

pub struct Tree {
    pub entries: Vec<TreeEntry>,
}

impl Tree {
    pub fn id(&self) -> String {
        let encoded = self.encode();
        blake3::hash(&encoded).to_hex().to_string()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"KV");
        out.push(0x02);

        let mut entries = self.entries.clone();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let count = entries.len() as u32;
        out.extend_from_slice(&count.to_le_bytes());

        for entry in entries {
            let kind = match entry.kind {
                EntryKind::Blob => 0x01,
                EntryKind::Tree => 0x02,
            };

            out.push(kind);

            let name_bytes = entry.name.as_bytes();
            let name_len = name_bytes.len() as u16;

            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(name_bytes);

            out.extend_from_slice(&entry.id);
        }

        out
    }
}

impl Tree {
    pub fn decode(input: &[u8]) -> Result<Tree, String> {
        if input.len() < 7 {
            return Err("input too short".to_string());
        }

        if &input[0..2] != b"KV" {
            return Err("invalid magic".to_string());
        }

        if input[2] != 0x02 {
            return Err("not a tree".to_string());
        }

        let count_bytes: [u8; 4] = input[3..7]
            .try_into()
            .map_err(|_| "invalid entry count".to_string())?;

        let count = u32::from_le_bytes(count_bytes);

        let mut offset = 7;

        let mut entries = Vec::new();

        for _ in 0..count {
            if offset >= input.len() {
                return Err("missing entry kind".to_string());
            }
            let kind = match input[offset] {
                0x01 => EntryKind::Blob,
                0x02 => EntryKind::Tree,
                _ => return Err("invalid entry kind".to_string()),
            };

            offset += 1;

            if offset + 2 > input.len() {
                return Err("missing name length".to_string());
            }

            let name_len_bytes: [u8; 2] = input[offset..offset + 2]
                .try_into()
                .map_err(|_| "invalid name length".to_string())?;

            let name_len = u16::from_le_bytes(name_len_bytes) as usize;

            offset += 2;

            if offset + name_len > input.len() {
                return Err("missing name bytes".to_string());
            }

            let name = std::str::from_utf8(&input[offset..offset + name_len])
                .map_err(|_| "invalid utf-8 filename".to_string())?
                .to_string();

            offset += name_len;

            if offset + 32 > input.len() {
                return Err("missing object id".to_string());
            }

            let id: [u8; 32] = input[offset..offset + 32]
                .try_into()
                .map_err(|_| "invalid object id".to_string())?;

            offset += 32;

            entries.push(TreeEntry {
                name,
                kind,
                id,
            });
        }

        if offset != input.len() {
            return Err("trailing bytes".to_string());
        }

        Ok(Tree { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_encoding_is_canonical() {
        let tree = Tree {
            entries: vec![
                TreeEntry {
                    name: "hello.txt".to_string(),
                    kind: EntryKind::Blob,
                    id: [0x11; 32],
                },
            ],
        };

        let encoded = tree.encode();

        let mut expected = Vec::new();
        expected.extend_from_slice(b"KV");
        expected.push(0x02);
        expected.extend_from_slice(&1u32.to_le_bytes());

        expected.push(0x01);
        expected.extend_from_slice(&9u16.to_le_bytes());
        expected.extend_from_slice(b"hello.txt");
        expected.extend_from_slice(&[0x11; 32]);

        assert_eq!(encoded, expected);
    }

    #[test]
    fn tree_entry_order_does_not_change_encoding() {
        let a = TreeEntry {
            name: "a.txt".to_string(),
            kind: EntryKind::Blob,
            id: [0x11; 32],
        };

        let b = TreeEntry {
            name: "b.txt".to_string(),
            kind: EntryKind::Blob,
            id: [0x22; 32],
        };

        let tree1 = Tree {
            entries: vec![a.clone(), b.clone()],
        };

        let tree2 = Tree {
            entries: vec![b, a],
        };

        assert_eq!(tree1.encode(), tree2.encode());
    }

    #[test]
    fn same_tree_has_same_id_regardless_of_entry_order() {
        let a = TreeEntry {
            name: "a.txt".to_string(),
            kind: EntryKind::Blob,
            id: [0x11; 32],
        };

        let b = TreeEntry {
            name: "b.txt".to_string(),
            kind: EntryKind::Blob,
            id: [0x22; 32],
        };

        let tree1 = Tree {
            entries: vec![a.clone(), b.clone()],
        };

        let tree2 = Tree {
            entries: vec![b, a],
        };

        assert_eq!(tree1.id(), tree2.id());
    }

    #[test]
    fn tree_round_trip() {
        let original = Tree {
            entries: vec![
                TreeEntry {
                    name: "hello.txt".to_string(),
                    kind: EntryKind::Blob,
                    id: [0x11; 32],
                },
            ],
        };

        let encoded = original.encode();
        let decoded = Tree::decode(&encoded).unwrap();

        assert_eq!(decoded.entries, original.entries);
    }
}