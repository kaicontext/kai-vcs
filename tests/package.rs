use kai_vcs::*;
use std::collections::HashMap;
mod support;
use support::{id, sample};
type Store = HashMap<ObjectId, Vec<u8>>;
fn put(s: &mut Store, o: Object) -> InventoryEntry {
    let bytes = o.encode().unwrap();
    let e = InventoryEntry {
        id: ObjectId::hash(&bytes),
        type_code: o.type_code(),
        framed_length: bytes.len() as u64,
        interpretation: Interpretation::Interpreted,
    };
    s.insert(e.id, bytes);
    e
}
fn setup() -> (Store, TransferManifest, InventoryEntry) {
    let mut s = Store::new();
    let blob = put(
        &mut s,
        Object::Blob(Blob {
            data: b"saved bytes".to_vec(),
        }),
    );
    let tree = put(
        &mut s,
        Object::Tree(
            Tree::new(vec![TreeEntry {
                name: b"file".to_vec(),
                kind: EntryKind::Regular,
                target: Target::Native(blob.id),
            }])
            .unwrap(),
        ),
    );
    let mut inventory = vec![blob.clone(), tree.clone()];
    inventory.sort_by_key(|e| e.id);
    (
        s,
        TransferManifest {
            repository: [1; 16],
            contents: PackageContents {
                completeness: PackageCompleteness::Standalone,
                roots: vec![PackageRoot {
                    id: tree.id,
                    type_code: 2,
                }],
                inventory,
                prerequisites: vec![],
                required_features: vec![],
                optional_features: vec![],
            },
        },
        blob,
    )
}
fn verify(s: &Store, v: TransferManifest) -> Result<VerifiedPackage, String> {
    verify_package(
        &Object::TransferManifest(Box::new(v)),
        &[],
        ClosureLimits::default(),
        |id, _| s.get(&id).cloned().ok_or("missing".into()),
    )
}
#[test]
fn standalone_verification_reads_every_object_and_required_source() {
    let (s, v, _) = setup();
    assert_eq!(verify(&s, v).unwrap(), VerifiedPackage::StandaloneVerified);
}
#[test]
fn incremental_prerequisites_remain_explicit_even_after_verification() {
    let (s, mut v, blob) = setup();
    v.contents.inventory.retain(|e| e.id != blob.id);
    v.contents.prerequisites.push(blob.clone());
    assert!(v.encode().is_err());
    v.contents.completeness = PackageCompleteness::Incremental;
    let mut consulted = false;
    let result = verify_package(
        &Object::TransferManifest(Box::new(v.clone())),
        &[],
        ClosureLimits::default(),
        |id, location| {
            if id == blob.id {
                assert_eq!(location, PackageLocation::Prerequisite);
                consulted = true;
            } else {
                assert_eq!(location, PackageLocation::Packaged);
            }
            Ok(s[&id].clone())
        },
    )
    .unwrap();
    assert!(consulted);
    assert_eq!(result, VerifiedPackage::IncrementalVerified);
    let mut missing = s;
    missing.remove(&blob.id);
    assert!(verify(&missing, v).is_err());
}
#[test]
fn closure_cannot_fetch_undeclared_objects_or_promote_opaque_metadata() {
    let (s, mut v, blob) = setup();
    v.contents.inventory.retain(|e| e.id != blob.id);
    assert!(
        verify(&s, v.clone())
            .unwrap_err()
            .contains("absent from package inventory")
    );
    let mut opaque = blob;
    opaque.interpretation = Interpretation::Opaque;
    v.contents.inventory.push(opaque);
    v.contents.inventory.sort_by_key(|e| e.id);
    assert!(verify(&s, v).unwrap_err().contains("opaque"));
}
#[test]
fn opaque_unknown_bytes_are_verified_and_not_interpreted_or_dropped() {
    let (mut s, mut v, _) = setup();
    let mut raw = b"KAI-OBJECT\0\0\x01".to_vec();
    raw.extend_from_slice(&500u16.to_be_bytes());
    raw.extend_from_slice(&3u64.to_be_bytes());
    raw.extend_from_slice(b"xyz");
    let oid = ObjectId::hash(&raw);
    s.insert(oid, raw.clone());
    v.contents.inventory.push(InventoryEntry {
        id: oid,
        type_code: 500,
        framed_length: raw.len() as u64,
        interpretation: Interpretation::Opaque,
    });
    v.contents.inventory.sort_by_key(|e| e.id);
    assert_eq!(
        verify(&s, v.clone()).unwrap(),
        VerifiedPackage::StandaloneVerified
    );
    s.get_mut(&oid).unwrap()[23] ^= 1;
    assert!(verify(&s, v.clone()).is_err());
    s.insert(oid, raw);
    v.contents
        .inventory
        .iter_mut()
        .find(|e| e.id == oid)
        .unwrap()
        .interpretation = Interpretation::Interpreted;
    assert!(verify(&s, v).is_err());
}
#[test]
fn features_fail_before_object_reads_and_optional_features_are_preserved() {
    let (s, mut v, _) = setup();
    v.contents.required_features = vec![b"future-required".to_vec()];
    assert!(
        verify_package(
            &Object::TransferManifest(Box::new(v.clone())),
            &[],
            ClosureLimits::default(),
            |_, _| panic!("read before feature check")
        )
        .is_err()
    );
    v.contents.required_features.clear();
    v.contents.optional_features = vec![b"future-optional".to_vec()];
    assert!(verify(&s, v).is_ok());
}
#[test]
fn inventory_checksums_types_and_sizes_are_not_trusted() {
    let (s, v, blob) = setup();
    for mutation in [0, 1, 2] {
        let mut v = v.clone();
        let entry = v
            .contents
            .inventory
            .iter_mut()
            .find(|e| e.id == blob.id)
            .unwrap();
        match mutation {
            0 => entry.framed_length += 1,
            1 => entry.type_code = 2,
            _ => entry.id = id(90),
        };
        v.contents.inventory.sort_by_key(|e| e.id);
        assert!(verify(&s, v).is_err());
    }
}
#[test]
fn duplicate_inventory_roots_and_feature_lists_are_rejected() {
    let (_, v, blob) = setup();
    let mut changed = v.clone();
    changed.contents.inventory.push(blob.clone());
    assert!(changed.encode().is_err());
    changed = v.clone();
    changed.contents.prerequisites.push(blob);
    changed.contents.completeness = PackageCompleteness::Incremental;
    assert!(changed.encode().is_err());
    changed = v.clone();
    changed
        .contents
        .roots
        .push(changed.contents.roots[0].clone());
    assert!(changed.encode().is_err());
    changed = v;
    changed.contents.required_features = vec![b"same".to_vec()];
    changed.contents.optional_features = changed.contents.required_features.clone();
    assert!(changed.encode().is_err());
}
#[test]
fn backup_captures_portable_view_and_checks_committed_operation_claim() {
    let mut s = Store::new();
    let view = put(&mut s, Object::View(Box::default()));
    let contents = PackageContents {
        completeness: PackageCompleteness::Standalone,
        roots: vec![],
        inventory: vec![view.clone()],
        prerequisites: vec![],
        required_features: vec![],
        optional_features: vec![],
    };
    let mut backup = BackupManifest {
        repository: [1; 16],
        generation: 0,
        current_view: view.id,
        current_operation: None,
        contents,
    };
    let verify = |s: &Store, b: BackupManifest| {
        verify_package(
            &Object::BackupManifest(Box::new(b)),
            &[],
            ClosureLimits::default(),
            |id, _| s.get(&id).cloned().ok_or("missing".into()),
        )
    };
    assert_eq!(
        verify(&s, backup.clone()).unwrap(),
        VerifiedPackage::StandaloneVerified
    );
    backup.generation = 1;
    assert!(backup.encode().is_err());
    let Object::Operation(mut operation) = sample("operation-full") else {
        panic!()
    };
    operation.post_view = view.id;
    let op = put(&mut s, Object::Operation(operation));
    backup.current_operation = Some(op.id);
    backup.contents.inventory.push(op);
    backup.contents.inventory.sort_by_key(|e| e.id);
    assert!(verify(&s, backup.clone()).is_ok());
    let other = View {
        namespace_generation: 1,
        ..View::default()
    };
    let other = put(&mut s, Object::View(Box::new(other)));
    backup.current_view = other.id;
    backup.contents.inventory.push(other);
    backup.contents.inventory.sort_by_key(|e| e.id);
    assert!(verify(&s, backup).unwrap_err().contains("linkage"));
}
#[test]
fn package_budgets_cover_inventory_and_closure_rereads() {
    let (s, v, _) = setup();
    let total = v.contents.inventory.iter().map(|e| e.framed_length).sum();
    for limits in [
        ClosureLimits {
            max_objects: 1,
            ..ClosureLimits::default()
        },
        ClosureLimits {
            max_bytes: total,
            ..ClosureLimits::default()
        },
        ClosureLimits {
            max_edges: 1,
            ..ClosureLimits::default()
        },
    ] {
        assert!(
            verify_package(
                &Object::TransferManifest(Box::new(v.clone())),
                &[],
                limits,
                |id, _| Ok(s[&id].clone())
            )
            .is_err()
        );
    }
}
#[test]
fn packaged_missing_objects_never_fall_back_to_external_prerequisites() {
    let (s, v, blob) = setup();
    assert!(
        verify_package(
            &Object::TransferManifest(Box::new(v)),
            &[],
            ClosureLimits::default(),
            |id, location| {
                assert_eq!(location, PackageLocation::Packaged);
                if id == blob.id {
                    Err("not packaged".into())
                } else {
                    Ok(s[&id].clone())
                }
            }
        )
        .is_err()
    );
}
