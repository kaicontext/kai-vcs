use kai_object_reference as independent;
use kai_vcs::*;
use std::collections::HashMap;
mod support;

const MIB: usize = 1024 * 1024;
fn id(byte: u8) -> ObjectId {
    ObjectId::from_digest([byte; 32])
}
fn entry(name: &[u8], kind: EntryKind, target: Target) -> TreeEntry {
    TreeEntry {
        name: name.to_vec(),
        kind,
        target,
    }
}
fn person() -> Identity {
    Identity {
        name: b"A\xff".to_vec(),
        email: b"a@example.test".to_vec(),
        timestamp_seconds: -1,
        timezone_minutes: -90,
    }
}
fn revision(parents: Vec<ObjectId>, change: u8) -> Revision {
    Revision {
        tree: id(0x22),
        parents,
        change_id: [change; 16],
        author: person(),
        committer: person(),
        message: b"message\0\xff\r\n".to_vec(),
        provenance: vec![],
        git_envelope: None,
        conflicts: vec![],
    }
}
fn sample(name: &str) -> Object {
    match name {
        "blob-empty" => Object::Blob(Blob { data: vec![] }),
        "blob-hello" => Object::Blob(Blob {
            data: b"hello\n".to_vec(),
        }),
        "blob-binary" => Object::Blob(Blob {
            data: (0u8..=255).collect(),
        }),
        "tree-empty" => Object::Tree(Tree::new(vec![]).unwrap()),
        "tree-mixed" => Object::Tree(
            Tree::new(vec![
                entry(b"\xff", EntryKind::Regular, Target::Native(id(0x66))),
                entry(b"link", EntryKind::Symlink, Target::Native(id(0x55))),
                entry(b"git256", EntryKind::Gitlink, Target::GitSha256([0x44; 32])),
                entry(b"git", EntryKind::Gitlink, Target::GitSha1([0x33; 20])),
                entry(b"dir", EntryKind::Directory, Target::Native(id(0x22))),
                entry(b"a", EntryKind::Executable, Target::Native(id(0x11))),
                entry(b"A", EntryKind::Regular, Target::Native(id(0x11))),
            ])
            .unwrap(),
        ),
        "revision-root" => Object::Revision(Box::new(revision(vec![], 0x77))),
        "revision-merge" => Object::Revision(Box::new(revision(vec![id(0x88), id(0x99)], 0x78))),
        "chunk-small" => Object::Chunk(b"\0\xff\r\n".to_vec()),
        "large-manifest" => Object::LargeBlob(LargeBlob {
            length: (16 * MIB + 1) as u64,
            content_digest: [0xaa; 32],
            chunks: vec![id(0xbb), id(0xbb), id(0xbb), id(0xbb), id(0xcc)],
        }),
        _ => support::sample(name),
    }
}
fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
fn fixtures() -> Vec<(&'static str, Vec<u8>, &'static str)> {
    include_str!("fixtures/objects.tsv")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .map(|s| {
            let parts: Vec<_> = s.split('\t').collect();
            (parts[0], hex(parts[1]), parts[2])
        })
        .collect()
}
fn frame(kind: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = b"KAI-OBJECT\0\0\x01".to_vec();
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    out.extend_from_slice(payload);
    out
}
fn both_reject(data: &[u8]) {
    assert!(
        Object::decode(data).is_err(),
        "production accepted malformed {} bytes",
        data.len()
    );
    assert!(
        independent::decode(data).is_err(),
        "reference accepted malformed {} bytes",
        data.len()
    );
}

#[test]
fn t005_published_vectors_agree_in_two_independent_implementations() {
    for (name, bytes, expected_id) in fixtures() {
        let object = sample(name);
        assert_eq!(object.encode().unwrap(), bytes, "{name}");
        assert_eq!(object.id().unwrap().to_string(), expected_id, "{name}");
        assert_eq!(Object::decode(&bytes).unwrap(), object, "{name}");
        assert_eq!(
            independent::encode(&independent::decode(&bytes).unwrap()).unwrap(),
            bytes,
            "{name}"
        );
        assert_eq!(independent::id(&bytes).unwrap(), expected_id, "{name}");
    }
}

#[test]
fn t005_every_truncation_and_trailing_byte_is_rejected() {
    for (_, bytes, _) in fixtures() {
        for length in 0..bytes.len() {
            both_reject(&bytes[..length]);
        }
        let mut extra = bytes.clone();
        extra.push(0);
        both_reject(&extra);
        // Correct envelope length but truncated structured payload must also fail.
        let kind = u16::from_be_bytes([bytes[13], bytes[14]]);
        if [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 15, 16, 17, 18, 19].contains(&kind) {
            for length in 0..bytes.len() - 23 {
                both_reject(&frame(kind, &bytes[23..23 + length]));
            }
        }
    }
}

#[test]
fn t005_differential_single_byte_mutations() {
    for (name, bytes, _) in fixtures() {
        for i in 0..bytes.len() {
            for mask in [1, 0x20, 0x80, 0xff] {
                let mut changed = bytes.clone();
                changed[i] ^= mask;
                let production = Object::decode(&changed);
                let reference = independent::decode(&changed);
                assert_eq!(
                    production.is_ok(),
                    reference.is_ok(),
                    "{name}, byte {i}, mask {mask}: {production:?}, {reference:?}"
                );
                if let Ok(object) = production {
                    assert_eq!(object.encode().unwrap(), changed);
                }
            }
        }
    }
}

#[test]
fn t005_malformed_cbor_and_unsupported_types() {
    for payload in [
        vec![0x98, 0],
        vec![0x9f, 0xff],
        vec![0x80, 0],
        vec![0x81, 0xf6],
        vec![0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
    ] {
        both_reject(&frame(2, &payload));
    }
    for kind in [0, 4, 5, 6, 7, 8, 9, 10, 11, 12, 15, 16, 17, 18, 19, 65535] {
        both_reject(&frame(kind, &[]));
    }
    both_reject(b"KV\x02\0\0\0\0");
    both_reject(&frame(13, &[]));
}

#[test]
fn typed_ids_refuse_ambiguous_and_hostile_text() {
    let valid = id(0xaa).to_string();
    assert_eq!(valid.parse::<ObjectId>().unwrap(), id(0xaa));
    for bad in [
        valid.to_uppercase(),
        valid[6..].to_owned(),
        "k1:b3:../objects".into(),
        "k1:b3:é".into(),
        format!("{valid}\n"),
        format!("{valid}0"),
    ] {
        assert!(bad.parse::<ObjectId>().is_err());
    }
}

#[test]
fn t003_tree_identity_is_order_independent_and_preserves_modes_and_raw_names() {
    let a = entry(b"\xff", EntryKind::Regular, Target::Native(id(1)));
    let b = entry(b"a", EntryKind::Regular, Target::Native(id(1)));
    let t = Tree::new(vec![a.clone(), b.clone()]).unwrap();
    assert_eq!(t.id(), Tree::new(vec![b.clone(), a]).unwrap().id());
    let mut executable = b.clone();
    executable.kind = EntryKind::Executable;
    let mut symlink = b.clone();
    symlink.kind = EntryKind::Symlink;
    assert_ne!(
        Tree::new(vec![b]).unwrap().id(),
        Tree::new(vec![executable]).unwrap().id()
    );
    assert_ne!(Tree::new(vec![symlink]).unwrap().id(), t.id());
    assert_eq!(Tree::decode(&t.encode().unwrap()).unwrap(), t);
    // Storage identity must not conflate names even if some hosts cannot materialize them.
    assert!(
        Tree::new(vec![
            entry(b"A", EntryKind::Regular, Target::Native(id(1))),
            entry(b"a", EntryKind::Regular, Target::Native(id(1)))
        ])
        .is_ok()
    );
    let tiny = Tree::new(vec![entry(
        b"x",
        EntryKind::Gitlink,
        Target::GitSha1([1; 20]),
    )])
    .unwrap();
    assert_eq!(Tree::decode(&tiny.encode().unwrap()).unwrap(), tiny);
}

#[test]
fn tree_refuses_invalid_and_duplicate_names_and_mismatched_namespaces() {
    for name in [b"".as_slice(), b".", b"..", b"a/b", b"a\0b"] {
        assert!(Tree::new(vec![entry(name, EntryKind::Regular, Target::Native(id(1)))]).is_err());
    }
    let a = entry(b"a", EntryKind::Regular, Target::Native(id(1)));
    assert!(Tree::new(vec![a.clone(), a]).is_err());
    assert!(Tree::new(vec![entry(b"a", EntryKind::Gitlink, Target::Native(id(1)))]).is_err());
    assert!(
        Tree::new(vec![entry(
            b"a",
            EntryKind::Regular,
            Target::GitSha1([0; 20])
        )])
        .is_err()
    );
}

#[test]
fn decoder_refuses_unsorted_and_duplicate_entries() {
    let a = Tree::new(vec![entry(b"a", EntryKind::Regular, Target::Native(id(1)))])
        .unwrap()
        .encode()
        .unwrap();
    let b = Tree::new(vec![entry(b"b", EntryKind::Regular, Target::Native(id(1)))])
        .unwrap()
        .encode()
        .unwrap();
    for second in [&a, &b] {
        let mut payload = vec![0x82];
        payload.extend_from_slice(&b[24..]);
        payload.extend_from_slice(&second[24..]);
        both_reject(&frame(2, &payload));
    }
    let mut payload = a[23..].to_vec();
    // First key 0 encoded as a redundant one-byte integer.
    payload.splice(2..3, [0x18, 0]);
    both_reject(&frame(2, &payload));
}

#[test]
fn t004_a_b_a_are_three_causal_revisions_at_equal_timestamps() {
    let a = revision(vec![], 1);
    let mut b = revision(vec![a.id().unwrap()], 2);
    b.tree = id(0x33);
    let c = revision(vec![b.id().unwrap()], 3);
    assert_eq!(a.tree, c.tree);
    assert_eq!(a.author.timestamp_seconds, c.author.timestamp_seconds);
    assert_ne!(a.id(), c.id());
    assert_ne!(b.id(), c.id());
    let mut merge = revision(vec![a.id().unwrap(), b.id().unwrap()], 4);
    let before = merge.id();
    merge.parents.reverse();
    assert_ne!(before, merge.id());
    merge.parents.push(merge.parents[0]);
    assert!(merge.encode().is_err());
}

#[test]
fn revision_optional_fields_and_signed_extremes_roundtrip() {
    let mut value = revision(vec![], 1);
    value.author.timestamp_seconds = i64::MIN;
    value.committer.timestamp_seconds = i64::MAX;
    value.provenance = vec![id(1)];
    value.git_envelope = Some(id(2));
    value.conflicts = vec![(b"a/b".to_vec(), id(3)), (b"x\xff".to_vec(), id(4))];
    let encoded = value.encode().unwrap();
    assert_eq!(Revision::decode(&encoded).unwrap(), value);
    assert_eq!(
        independent::encode(&independent::decode(&encoded).unwrap()).unwrap(),
        encoded
    );
    value.conflicts.push((b"../outside".to_vec(), id(5)));
    assert!(value.encode().is_err());
    let mut bad = revision(vec![], 1);
    bad.author.timezone_minutes = 1440;
    assert!(bad.encode().is_err());
}

#[test]
fn t006_claimed_id_cannot_be_reused_for_altered_content() {
    let object = sample("blob-hello");
    let id = object.id().unwrap();
    let mut bytes = object.encode().unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    assert!(Object::verify(id, &bytes).is_err());
    assert_eq!(
        Object::verify(id, &object.encode().unwrap()).unwrap(),
        object
    );
}

#[test]
fn blob_and_chunk_thresholds() {
    let blob = Blob {
        data: vec![0; 16 * MIB],
    };
    assert!(blob.encode().is_ok());
    let blob = Blob {
        data: vec![0; 16 * MIB + 1],
    };
    assert!(blob.encode().is_err());
    assert!(Object::Chunk(vec![]).encode().is_err());
    assert!(Object::Chunk(vec![0; 4 * MIB]).encode().is_ok());
    assert!(Object::Chunk(vec![0; 4 * MIB + 1]).encode().is_err());
}

#[test]
fn t010_chunk_order_lengths_hashes_and_total_content_are_verified() {
    let mut store = HashMap::new();
    let mut chunks = vec![];
    let mut raw = blake3::Hasher::new();
    for i in 0..5 {
        let content = vec![i; if i == 4 { 1 } else { 4 * MIB }];
        raw.update(&content);
        let chunk = Object::Chunk(content);
        let key = chunk.id().unwrap();
        chunks.push(key);
        store.insert(key, chunk.encode().unwrap());
    }
    let manifest = LargeBlob {
        length: (16 * MIB + 1) as u64,
        content_digest: *raw.finalize().as_bytes(),
        chunks,
    };
    let read = |key| {
        store
            .get(&key)
            .cloned()
            .ok_or_else(|| "missing chunk".into())
    };
    manifest.verify_content(read).unwrap();
    let mut reordered = manifest.clone();
    reordered.chunks.swap(0, 1);
    assert!(reordered.verify_content(read).is_err());
    let mut wrong_length = manifest.clone();
    wrong_length.length += 1;
    assert!(wrong_length.verify_content(read).is_err());
    let mut wrong_hash = manifest.clone();
    wrong_hash.content_digest[0] ^= 1;
    assert!(wrong_hash.verify_content(read).is_err());
    let mut short = manifest.clone();
    short.chunks.pop();
    assert!(short.encode().is_err());
    let missing = manifest.chunks[2];
    store.remove(&missing);
    assert!(
        manifest
            .verify_content(|key| store
                .get(&key)
                .cloned()
                .ok_or_else(|| "missing chunk".into()))
            .is_err()
    );
}

fn insert(store: &mut HashMap<ObjectId, Vec<u8>>, object: Object) -> ObjectId {
    let id = object.id().unwrap();
    store.insert(id, object.encode().unwrap());
    id
}

#[test]
fn t007_typed_closure_rejects_missing_corrupt_wrong_type_and_empty_directory() {
    let mut store = HashMap::new();
    let blob = insert(&mut store, sample("blob-hello"));
    let tree = insert(
        &mut store,
        Object::Tree(
            Tree::new(vec![entry(b"a", EntryKind::Regular, Target::Native(blob))]).unwrap(),
        ),
    );
    let mut r = revision(vec![], 1);
    r.tree = tree;
    let root = insert(&mut store, Object::Revision(Box::new(r)));
    let verify = |store: &HashMap<ObjectId, Vec<u8>>, root| {
        verify_closure(&[root], ClosureLimits::default(), |id| {
            store.get(&id).cloned().ok_or_else(|| "missing".into())
        })
    };
    assert_eq!(verify(&store, root).unwrap().objects, 3);
    let good = store.remove(&blob).unwrap();
    assert!(
        verify(&store, root)
            .unwrap_err()
            .contains(&blob.to_string())
    );
    let mut corrupt = good.clone();
    corrupt[23] ^= 1;
    store.insert(blob, corrupt);
    assert!(verify(&store, root).is_err());
    store.insert(blob, good);
    let wrong = insert(
        &mut store,
        Object::Tree(
            Tree::new(vec![entry(
                b"a",
                EntryKind::Directory,
                Target::Native(blob),
            )])
            .unwrap(),
        ),
    );
    assert!(
        verify(&store, wrong)
            .unwrap_err()
            .contains("wrong object type")
    );
    let empty = insert(&mut store, sample("tree-empty"));
    assert!(verify(&store, empty).is_ok());
    let invalid = insert(
        &mut store,
        Object::Tree(
            Tree::new(vec![entry(
                b"a",
                EntryKind::Directory,
                Target::Native(empty),
            )])
            .unwrap(),
        ),
    );
    assert!(
        verify(&store, invalid)
            .unwrap_err()
            .contains("empty directory")
    );
}

#[test]
fn t011_deep_history_verifies_iteratively_with_explicit_limits() {
    let mut store = HashMap::new();
    let tree = insert(&mut store, sample("tree-empty"));
    let mut parents = vec![];
    for _ in 0..2000 {
        let mut r = revision(parents, 1);
        r.tree = tree;
        parents = vec![insert(&mut store, Object::Revision(Box::new(r)))];
    }
    let read = |id| store.get(&id).cloned().ok_or_else(|| "missing".into());
    assert_eq!(
        verify_closure(&parents, ClosureLimits::default(), read)
            .unwrap()
            .objects,
        2001
    );
    assert!(
        verify_closure(
            &parents,
            ClosureLimits {
                max_objects: 100,
                ..ClosureLimits::default()
            },
            read
        )
        .is_err()
    );
    assert!(
        verify_closure(
            &parents,
            ClosureLimits {
                max_bytes: 10,
                ..ClosureLimits::default()
            },
            read
        )
        .is_err()
    );
    assert!(
        verify_closure(
            &parents,
            ClosureLimits {
                max_edges: 10,
                ..ClosureLimits::default()
            },
            read
        )
        .is_err()
    );
}

#[test]
fn external_gitlinks_and_optional_provenance_do_not_imply_native_availability() {
    let mut store = HashMap::new();
    let tree = insert(
        &mut store,
        Object::Tree(
            Tree::new(vec![entry(
                b"g",
                EntryKind::Gitlink,
                Target::GitSha1([1; 20]),
            )])
            .unwrap(),
        ),
    );
    let mut r = revision(vec![], 1);
    r.tree = tree;
    r.provenance = vec![id(99)];
    let root = insert(&mut store, Object::Revision(Box::new(r.clone())));
    let read = |id| store.get(&id).cloned().ok_or_else(|| "missing".into());
    assert_eq!(
        verify_closure(&[root], ClosureLimits::default(), read)
            .unwrap()
            .objects,
        2
    );
    r.git_envelope = Some(id(88));
    let root = insert(&mut store, Object::Revision(Box::new(r)));
    assert!(
        verify_closure(&[root], ClosureLimits::default(), |id| store
            .get(&id)
            .cloned()
            .ok_or_else(|| "missing".into()))
        .is_err()
    );
}

#[test]
fn canonical_length_and_integer_boundaries_agree_with_independent_codec() {
    for length in [1, 23, 24, 255, 256, 65535, 65536] {
        let tree = Tree::new(vec![entry(
            &vec![b'x'; length],
            EntryKind::Regular,
            Target::Native(id(1)),
        )])
        .unwrap();
        let bytes = tree.encode().unwrap();
        assert_eq!(
            independent::encode(&independent::decode(&bytes).unwrap()).unwrap(),
            bytes
        );
        assert_eq!(Tree::decode(&bytes).unwrap(), tree);
    }
    for seconds in [
        i64::MIN,
        -4294967297,
        -65537,
        -257,
        -25,
        -24,
        -1,
        0,
        23,
        24,
        255,
        256,
        65535,
        65536,
        4294967295,
        4294967296,
        i64::MAX,
    ] {
        let mut r = revision(vec![], 1);
        r.author.timestamp_seconds = seconds;
        let bytes = r.encode().unwrap();
        assert_eq!(
            independent::encode(&independent::decode(&bytes).unwrap()).unwrap(),
            bytes
        );
        assert_eq!(Revision::decode(&bytes).unwrap(), r);
    }
}

#[test]
fn generated_byte_zoo_roundtrips_without_parser_or_text_normalization() {
    // Fixed xorshift seed; includes NUL, invalid UTF-8 and all newline forms.
    let mut seed = 0x4b41495f50305f01u64;
    let mut cases = vec![
        vec![],
        b"\xef\xbb\xbfline\r\nline\nline\r  \t\0\xff".to_vec(),
        b"<<<<<<< ours\n=======\n>>>>>>> theirs\n".to_vec(),
    ];
    for length in [1, 23, 24, 255, 256, 4096, 65536] {
        let bytes = (0..length)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect();
        cases.push(bytes);
    }
    for bytes in cases {
        let encoded = Blob {
            data: bytes.clone(),
        }
        .encode()
        .unwrap();
        assert_eq!(Blob::decode(&encoded).unwrap().data, bytes);
        let independent::Payload::Raw(decoded) = independent::decode(&encoded).unwrap().payload
        else {
            panic!("expected raw blob")
        };
        assert_eq!(decoded, bytes);
    }
}

#[test]
fn known_object_type_cannot_be_substituted_for_another() {
    let blob = Object::Blob(Blob {
        data: b"same bytes".to_vec(),
    });
    let chunk = Object::Chunk(b"same bytes".to_vec());
    assert_ne!(blob.id(), chunk.id());
    assert!(Object::verify(blob.id().unwrap(), &chunk.encode().unwrap()).is_err());
}

#[test]
fn decoder_bounds_hostile_nesting_and_declared_lengths() {
    let mut deeply_nested = vec![0x81; 1000];
    deeply_nested.push(0);
    both_reject(&frame(2, &deeply_nested));
    let mut huge = frame(2, &[0x80]);
    huge[15..23].copy_from_slice(&u64::MAX.to_be_bytes());
    both_reject(&huge);
    let mut overflow = vec![0x81, 0xa3, 0, 0x5b];
    overflow.extend_from_slice(&u64::MAX.to_be_bytes());
    both_reject(&frame(2, &overflow));
}
