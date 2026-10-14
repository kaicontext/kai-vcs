use kai_object_reference::{Payload, Record, Value};
use kai_vcs::*;
use std::collections::BTreeMap;
#[allow(dead_code)]
#[path = "../examples/support/repository.rs"]
mod fixture;

fn deleted(name: &str) -> ViewRef {
    ViewRef {
        name: name.into(),
        generation: 1,
        state: RefState::Deleted,
    }
}
fn check(
    root: &PartitionedView,
    leaves: &[View],
    limits: ClosureLimits,
) -> Result<VerifiedClosure, String> {
    let mut store = BTreeMap::new();
    for leaf in leaves {
        fixture::put(&mut store, Object::View(Box::new(leaf.clone())));
    }
    let id = fixture::put(&mut store, Object::PartitionedView(Box::new(root.clone())));
    verify_closure(&[id], limits, |id| {
        store.get(&id).cloned().ok_or("missing".into())
    })
}
#[test]
fn partitions_all_sections_reassembles_and_reuses_unmodified_buckets() {
    let f = fixture::fixture();
    let Object::View(mut original) = Object::decode(&f.store[&f.view]).unwrap() else {
        panic!()
    };
    for n in 0..2000 {
        original
            .refs
            .push(deleted(&format!("branches/test/{n:04}")));
    }
    original.refs.sort_by(|a, b| a.name.cmp(&b.name));
    let (root, leaves) = original.clone().partition().unwrap();
    assert_eq!(root.submaps.len(), 256);
    let mut assembled = View {
        namespace_generation: root.namespace_generation,
        ..View::default()
    };
    for leaf in &leaves {
        assert_eq!(leaf.namespace_generation, 0);
        assembled.refs.extend(leaf.refs.clone());
        assembled.drafts.extend(leaf.drafts.clone());
        assembled.workspaces.extend(leaf.workspaces.clone());
        assembled.retained.extend(leaf.retained.clone());
        let b = leaf.encode().unwrap();
        assert_eq!(
            kai_object_reference::encode(&kai_object_reference::decode(&b).unwrap()).unwrap(),
            b
        );
    }
    assembled.refs.sort_by(|a, b| a.name.cmp(&b.name));
    assembled.drafts.sort_by_key(|d| d.change_id);
    assembled.workspaces.sort_by_key(|w| w.workspace);
    assembled.retained.sort();
    assert_eq!(assembled, *original);
    original.refs[100].generation += 1;
    original.namespace_generation += 1;
    let (changed, _) = original.partition().unwrap();
    assert_ne!(root.id().unwrap(), changed.id().unwrap());
    assert_eq!(
        root.submaps
            .iter()
            .zip(&changed.submaps)
            .filter(|(a, b)| a != b)
            .count(),
        1
    );
}
#[test]
fn closure_rejects_missing_wrong_bucket_empty_nonzero_and_nested_submaps() {
    let (root, leaves) = View {
        refs: vec![deleted("branches/main")],
        ..View::default()
    }
    .partition()
    .unwrap();
    assert!(check(&root, &leaves, ClosureLimits::default()).is_ok());
    assert!(check(&root, &[], ClosureLimits::default()).is_err());
    let mut wrong = root.clone();
    wrong.submaps[0].0 ^= 1;
    assert!(
        check(&wrong, &leaves, ClosureLimits::default())
            .unwrap_err()
            .contains("submap")
    );
    for leaf in [
        View::default(),
        View {
            namespace_generation: 1,
            ..leaves[0].clone()
        },
        View {
            refs: (0..100)
                .map(|n| deleted(&format!("branches/{n:03}")))
                .collect(),
            ..View::default()
        },
    ] {
        let mut root = root.clone();
        root.submaps[0].1 = leaf.id().unwrap();
        assert!(
            check(&root, &[leaf], ClosureLimits::default())
                .unwrap_err()
                .contains("submap")
        );
    }
    let mut store = BTreeMap::new();
    let child = fixture::put(&mut store, Object::PartitionedView(Box::default()));
    let parent = fixture::put(
        &mut store,
        Object::PartitionedView(Box::new(PartitionedView {
            namespace_generation: 0,
            submaps: vec![(0, child)],
        })),
    );
    assert!(
        verify_closure(&[parent], ClosureLimits::default(), |id| Ok(
            store[&id].clone()
        ))
        .unwrap_err()
        .contains("submap")
    );
    assert!(
        check(
            &root,
            &leaves,
            ClosureLimits {
                max_objects: 1,
                ..ClosureLimits::default()
            }
        )
        .is_err()
    );
    assert!(
        check(
            &root,
            &leaves,
            ClosureLimits {
                max_edges: 1,
                ..ClosureLimits::default()
            }
        )
        .is_err()
    );
    assert!(
        check(
            &root,
            &leaves,
            ClosureLimits {
                max_bytes: 1,
                ..ClosureLimits::default()
            }
        )
        .is_err()
    );
    assert!(check(&PartitionedView::default(), &[], ClosureLimits::default()).is_ok());
}
fn wire(v: Value) -> Vec<u8> {
    // Serialize without either semantic validator to produce hostile wire inputs.
    fn convert(v: Value) -> ciborium::value::Value {
        use ciborium::value::Value as C;
        match v {
            Value::UInt(n) => C::Integer(n.into()),
            Value::Bytes(b) => C::Bytes(b),
            Value::Array(a) => C::Array(a.into_iter().map(convert).collect()),
            Value::Map(m) => C::Map(
                m.into_iter()
                    .map(|(k, v)| (C::Integer(k.into()), convert(v)))
                    .collect(),
            ),
            _ => unreachable!(),
        }
    }
    let mut payload = Vec::new();
    ciborium::ser::into_writer(&convert(v), &mut payload).unwrap();
    let mut out = b"KAI-OBJECT\0\0\x01\0\x06".to_vec();
    out.extend((payload.len() as u64).to_be_bytes());
    out.extend(payload);
    out
}
#[test]
fn both_decoders_reject_hostile_root_shapes_and_bucket_ordering() {
    use Value::{Array as A, UInt as U};
    let id = Value::Array(vec![U(1), U(1), Value::Bytes(vec![1; 32])]);
    let pair = |n| A(vec![U(n), id.clone()]);
    for entries in [
        vec![pair(256)],
        vec![pair(u64::MAX)],
        vec![pair(1), pair(1)],
        vec![pair(2), pair(1)],
        vec![A(vec![U(1)])],
        (0..257).map(pair).collect(),
    ] {
        let v = Value::Map([(0, U(0)), (1, A(entries)), (5, U(1))].into());
        let bytes = wire(v);
        assert!(Object::decode(&bytes).is_err());
        assert!(kai_object_reference::decode(&bytes).is_err());
    }
    let bytes = PartitionedView::default().encode().unwrap();
    let Record {
        payload: Payload::Structured(Value::Map(valid)),
        ..
    } = kai_object_reference::decode(&bytes).unwrap()
    else {
        panic!()
    };
    for key in [0, 1, 5] {
        let mut m = valid.clone();
        m.remove(&key);
        assert!(Object::decode(&wire(Value::Map(m.clone()))).is_err());
        assert!(kai_object_reference::decode(&wire(Value::Map(m))).is_err());
    }
    for (key, value) in [
        (2, A(vec![])),
        (5, U(2)),
        (5, U(0)),
        (0, Value::Bytes(vec![])),
    ] {
        let mut m = valid.clone();
        m.insert(key, value);
        let bytes = wire(Value::Map(m));
        assert!(Object::decode(&bytes).is_err());
        assert!(kai_object_reference::decode(&bytes).is_err());
    }
    for submaps in [
        vec![
            (1, ObjectId::from_digest([0; 32])),
            (1, ObjectId::from_digest([1; 32])),
        ],
        vec![
            (2, ObjectId::from_digest([0; 32])),
            (1, ObjectId::from_digest([1; 32])),
        ],
    ] {
        assert!(
            PartitionedView {
                namespace_generation: 0,
                submaps
            }
            .encode()
            .is_err()
        );
    }
}
