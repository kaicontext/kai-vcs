use kai_vcs::{collaboration_frontier::*, *};
use std::collections::HashMap;
fn put(s: &mut HashMap<ObjectId, Vec<u8>>, o: Object) -> ObjectId {
    let b = o.encode().unwrap();
    let id = ObjectId::hash(&b);
    s.insert(id, b);
    id
}
fn replica(n: u64) -> ReplicaFrontier {
    ReplicaFrontier {
        replica_hex: "01".repeat(16),
        sequence: n.to_string(),
        operations_digest: format!("b3:{}", "ab".repeat(32)),
    }
}
fn fixture() -> (
    CollaborationBarrier,
    CollaborationFrontier,
    HashMap<ObjectId, Vec<u8>>,
) {
    let mut s = HashMap::new();
    let blob = put(
        &mut s,
        Object::Blob(Blob {
            data: b"hello".to_vec(),
        }),
    );
    let child = put(
        &mut s,
        Object::Tree(
            Tree::new(vec![TreeEntry {
                name: vec![255],
                kind: EntryKind::Executable,
                target: Target::Native(blob),
            }])
            .unwrap(),
        ),
    );
    let tree = put(
        &mut s,
        Object::Tree(
            Tree::new(vec![
                TreeEntry {
                    name: b"a".to_vec(),
                    kind: EntryKind::Directory,
                    target: Target::Native(child),
                },
                TreeEntry {
                    name: b"b".to_vec(),
                    kind: EntryKind::Directory,
                    target: Target::Native(child),
                },
                TreeEntry {
                    name: b"git".to_vec(),
                    kind: EntryKind::Gitlink,
                    target: Target::GitSha1([2; 20]),
                },
            ])
            .unwrap(),
        ),
    );
    let p = CollaborationFrontier {
        schema_version: 1,
        workspace_hex: "03".repeat(16),
        epoch_hex: "04".repeat(16),
        generation: u64::MAX.to_string(),
        tree: tree.to_string(),
        protocol_hex: "ff".into(),
        namespace: vec![replica(10)],
        entries: vec![
            ("61", 4, child.to_string()),
            ("612fff", 2, blob.to_string()),
            ("62", 4, child.to_string()),
            ("622fff", 2, blob.to_string()),
            ("676974", 5, format!("sha1:{}", "02".repeat(20))),
        ]
        .into_iter()
        .map(|(path, kind, target)| MaterializedEntry {
            path_hex: path.into(),
            kind,
            target,
            frontier: vec![replica(9)],
        })
        .collect(),
    };
    let manifest = put(
        &mut s,
        Object::Blob(Blob {
            data: p.to_bytes().unwrap(),
        }),
    );
    let b = CollaborationBarrier {
        workspace: [3; 16],
        epoch: [4; 16],
        generation: u64::MAX,
        tree,
        protocol: vec![255],
        frontier: vec![([1; 16], 10)],
        excluded: vec![([9; 16], b"offline".to_vec())],
        manifest,
        manifest_format: FORMAT.to_vec(),
    };
    (b, p, s)
}
fn verify(
    b: &CollaborationBarrier,
    s: &HashMap<ObjectId, Vec<u8>>,
    limits: ClosureLimits,
) -> Result<CollaborationFrontier, String> {
    verify_collaboration_frontier(b, limits, |id| s.get(&id).cloned().ok_or("missing".into()))
}
#[test]
fn verifies_shared_subtrees_raw_names_gitlinks_and_full_width_counters() {
    let (b, p, s) = fixture();
    assert_eq!(verify(&b, &s, ClosureLimits::default()).unwrap(), p);
}
#[test]
fn rejects_missing_added_or_changed_materialization() {
    for case in 0..4 {
        let (mut b, mut p, mut s) = fixture();
        match case {
            0 => {
                p.entries.remove(3);
            }
            1 => p.entries[0].kind = 1,
            2 => p.entries[1].target = b.tree.to_string(),
            _ => p.entries[4].path_hex = "7a".into(),
        }
        b.manifest = put(
            &mut s,
            Object::Blob(Blob {
                data: p.to_bytes().unwrap(),
            }),
        );
        assert!(
            verify(&b, &s, ClosureLimits::default())
                .unwrap_err()
                .contains("materialization")
        );
    }
}
#[test]
fn rejects_context_mismatches_and_missing_participants() {
    for case in 0..7 {
        let (mut b, _, s) = fixture();
        match case {
            0 => b.workspace = [5; 16],
            1 => b.epoch = [5; 16],
            2 => b.generation = 0,
            3 => b.protocol = vec![0],
            4 => b.frontier[0].1 = 11,
            5 => b.frontier.push(([2; 16], 0)),
            _ => b.frontier[0].0 = [0; 16],
        }
        assert!(
            verify(&b, &s, ClosureLimits::default())
                .unwrap_err()
                .contains("context")
        );
    }
}
#[test]
fn rejects_future_excluded_duplicate_and_unsorted_frontiers() {
    let (_, p, _) = fixture();
    for case in 0..5 {
        let mut q = p.clone();
        match case {
            0 => q.entries[0].frontier[0].sequence = "11".into(),
            1 => q.entries[0].frontier[0].replica_hex = "09".repeat(16),
            2 => q.namespace.push(replica(10)),
            3 => q.entries.reverse(),
            _ => q.entries[0].frontier.push(replica(1)),
        }
        assert!(q.to_bytes().is_err());
    }
}
#[test]
fn unknown_format_reads_nothing_and_missing_or_corrupt_objects_fail() {
    let (mut b, _, mut s) = fixture();
    b.manifest_format = b"future".to_vec();
    assert!(
        verify_collaboration_frontier(&b, ClosureLimits::default(), |_| panic!("unexpected read"))
            .is_err()
    );
    b.manifest_format = FORMAT.to_vec();
    let bytes = s.remove(&b.manifest).unwrap();
    assert!(verify(&b, &s, ClosureLimits::default()).is_err());
    let mut corrupt = bytes;
    *corrupt.last_mut().unwrap() ^= 1;
    s.insert(b.manifest, corrupt);
    assert!(verify(&b, &s, ClosureLimits::default()).is_err());
}
#[test]
fn bounds_reads_bytes_and_path_expansion() {
    let (b, _, s) = fixture();
    for limits in [
        ClosureLimits {
            max_objects: 1,
            ..ClosureLimits::default()
        },
        ClosureLimits {
            max_bytes: 1,
            ..ClosureLimits::default()
        },
        ClosureLimits {
            max_edges: 1,
            ..ClosureLimits::default()
        },
    ] {
        assert!(verify(&b, &s, limits).is_err());
    }
}
#[test]
fn shared_canonical_corpus() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/collaboration-frontier-cases.json")).unwrap();
    for c in cases.as_array().unwrap() {
        let result = CollaborationFrontier::from_bytes(c["json"].as_str().unwrap().as_bytes());
        assert_eq!(
            result.is_ok(),
            c["valid"].as_bool().unwrap(),
            "{}: {:?}",
            c["name"],
            result
        );
    }
}
