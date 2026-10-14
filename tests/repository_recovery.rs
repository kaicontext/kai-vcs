use kai_object_reference::bundle::Bundle;
use kai_vcs::*;
use std::collections::BTreeMap;
#[allow(dead_code)]
#[path = "../examples/support/repository.rs"]
mod fixture;
mod support;
use fixture::{Fixture, container, fixture, inventory, put};
fn read(f: &Fixture) -> Result<Bundle, String> {
    Bundle::read(container(f).as_slice())
}
fn add(f: &mut Fixture, o: Object) -> ObjectId {
    let id = put(&mut f.store, o);
    f.backup.contents.inventory = inventory(&f.store);
    id
}
fn tar_files(data: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pos = 0;
    while pos + 512 <= data.len() && data[pos] != 0 {
        let h = &data[pos..pos + 512];
        let end = h[..100].iter().position(|b| *b == 0).unwrap_or(100);
        let name = std::str::from_utf8(&h[..end]).unwrap().to_string();
        let size = usize::from_str_radix(std::str::from_utf8(&h[124..135]).unwrap(), 8).unwrap();
        pos += 512;
        if h[156] == b'0' {
            files.insert(name, data[pos..pos + size].to_vec());
        }
        pos += size.div_ceil(512) * 512;
    }
    files
}
#[test]
fn recovers_exact_backup_objects_refs_checkpoints_conflicts_and_retention() {
    let f = fixture();
    assert!(
        verify_package(
            &Object::BackupManifest(Box::new(f.backup.clone())),
            &[],
            ClosureLimits::default(),
            |id, _| f.store.get(&id).cloned().ok_or("missing".into())
        )
        .is_ok()
    );
    let b = read(&f).unwrap();
    let index = b.recovery_index().unwrap();
    for text in [
        "backup generation=9",
        "state=0 targets=deleted",
        "state=2 targets=",
        "workspace 0202",
        "working-conflict",
        "state 0 absent",
        "kind=2 target=",
        "included=false",
    ] {
        assert!(index.contains(text), "missing {text}");
    }
    let mut tar = Vec::new();
    b.recovery().unwrap().write(&mut tar).unwrap();
    let files = tar_files(&tar);
    assert_eq!(files["repository.kai-bundle"], container(&f));
    assert_eq!(files["manifest.object"], f.backup.encode().unwrap());
    assert_eq!(files["INDEX.txt"], index.as_bytes());
    for (id, bytes) in &f.store {
        let path = format!(
            "objects/{}.object",
            id.to_string().strip_prefix("k1:b3:").unwrap()
        );
        assert_eq!(&files[&path], bytes);
    }
    let restored = Bundle::read(files["repository.kai-bundle"].as_slice()).unwrap();
    assert_eq!(restored.recovery_index().unwrap(), index);
    assert!(restored.archive(&f.resolved.to_string()).is_ok());
    assert!(
        restored
            .archive(&f.revision.to_string())
            .err()
            .unwrap()
            .contains("unresolved")
    );
    assert!(
        restored
            .archive(&f.checkpoint.to_string())
            .err()
            .unwrap()
            .contains("unresolved")
    );
}
#[test]
fn rejects_backup_operation_post_view_mismatch() {
    let mut f = fixture();
    f.backup.current_view = put(&mut f.store, Object::View(Box::default()));
    assert!(read(&f).err().unwrap().contains("post-view"));
}
#[test]
fn rejects_view_draft_and_workspace_mismatches() {
    for case in 0..2 {
        let mut f = fixture();
        let Object::View(mut v) = Object::decode(&f.store[&f.view]).unwrap() else {
            panic!()
        };
        if case == 0 {
            v.drafts[0].change_id = [9; 16];
        } else {
            v.workspaces[0].workspace = [9; 16];
        }
        add(&mut f, Object::View(v));
        let error = read(&f).err().unwrap();
        assert!(
            error.contains(if case == 0 { "change ID" } else { "workspace" }),
            "{error}"
        );
    }
}
#[test]
fn rejects_missing_conflict_sides_and_wrong_path_bindings() {
    for case in 0..3 {
        let mut f = fixture();
        if case == 0 {
            let Object::Conflict(c) = Object::decode(&f.store[&f.conflict]).unwrap() else {
                panic!()
            };
            let Target::Native(id) = c.sides[1].states[0].as_ref().unwrap().target else {
                panic!()
            };
            f.store.remove(&id);
            f.backup.contents.inventory = inventory(&f.store);
        } else {
            let Object::Revision(mut r) = Object::decode(&f.store[&f.revision]).unwrap() else {
                panic!()
            };
            if case == 1 {
                r.conflicts[0].0 = b"wrong".to_vec();
            } else {
                let Object::Revision(base) = Object::decode(&f.store[&f.resolved]).unwrap() else {
                    panic!()
                };
                r.tree = base.tree;
            }
            add(&mut f, Object::Revision(r));
        }
        let error = read(&f).err().unwrap();
        assert!(
            error.contains(match case {
                0 => "missing",
                1 => "manifest/path",
                _ => "ordinary tree",
            }),
            "{error}"
        );
    }
}
#[test]
fn validates_checkpoint_barrier_and_staged_conflict_bindings() {
    let mut f = fixture();
    let Object::Checkpoint(mut cp) = Object::decode(&f.store[&f.checkpoint]).unwrap() else {
        panic!()
    };
    let manifest = add(
        &mut f,
        Object::Blob(Blob {
            data: b"opaque frontier".to_vec(),
        }),
    );
    let barrier = add(
        &mut f,
        Object::CollaborationBarrier(Box::new(CollaborationBarrier {
            workspace: [9; 16],
            epoch: [1; 16],
            generation: 1,
            tree: cp.tree,
            protocol: b"test".to_vec(),
            frontier: vec![([1; 16], 1)],
            excluded: vec![],
            manifest,
            manifest_format: b"test".to_vec(),
        })),
    );
    cp.capture = CaptureClass::BarrierConsistent;
    cp.barrier = Some(barrier);
    add(&mut f, Object::Checkpoint(cp));
    assert!(read(&f).err().unwrap().contains("barrier binding"));
    let mut f = fixture();
    let Object::Checkpoint(mut cp) = Object::decode(&f.store[&f.checkpoint]).unwrap() else {
        panic!()
    };
    cp.selected_conflicts = cp.conflicts.clone();
    add(&mut f, Object::Checkpoint(cp));
    assert!(read(&f).err().unwrap().contains("ordinary tree"));
}
#[test]
fn preserves_advisory_metadata_without_promoting_expired_archival_edges() {
    let mut f = fixture();
    let Object::Revision(r) = Object::decode(&f.store[&f.resolved]).unwrap() else {
        panic!()
    };
    let blob = add(
        &mut f,
        Object::Blob(Blob {
            data: b"opaque evidence profile".to_vec(),
        }),
    );
    let Object::Evidence(mut e) = support::sample("evidence-execution") else {
        panic!()
    };
    e.revision = f.resolved;
    e.tree = r.tree;
    e.source_state = SourceState::Resolved;
    e.inputs.object = blob;
    e.environment.object = blob;
    let evidence = add(&mut f, Object::Evidence(e.clone()));
    let Object::Attestation(mut a) = support::sample("attestation-full") else {
        panic!()
    };
    a.subject = evidence;
    a.subject_type = 9;
    add(&mut f, Object::Attestation(a));
    let Object::AnalysisManifest(mut a) = support::sample("analysis-partial") else {
        panic!()
    };
    a.tree = r.tree;
    add(&mut f, Object::AnalysisManifest(a));
    add(&mut f, support::sample("change-rewrite"));
    add(&mut f, support::sample("policy-full"));
    assert!(read(&f).is_ok());
    e.source_state = SourceState::Conflicted;
    add(&mut f, Object::Evidence(e));
    assert!(read(&f).err().unwrap().contains("evidence source"));
}
#[test]
fn opaque_inventory_is_preserved_but_cannot_supply_required_edges() {
    let mut f = fixture();
    let mut opaque = b"KAI-OBJECT\0\0\x01".to_vec();
    opaque.extend(99u16.to_be_bytes());
    opaque.extend(3u64.to_be_bytes());
    opaque.extend(b"xyz");
    let id = ObjectId::hash(&opaque);
    f.store.insert(id, opaque.clone());
    f.backup.contents.inventory.push(InventoryEntry {
        id,
        type_code: 99,
        framed_length: opaque.len() as u64,
        interpretation: Interpretation::Opaque,
    });
    f.backup.contents.inventory.sort_by_key(|e| e.id);
    let b = read(&f).unwrap();
    let mut tar = Vec::new();
    b.recovery().unwrap().write(&mut tar).unwrap();
    assert_eq!(
        tar_files(&tar)[&format!(
            "objects/{}.object",
            id.to_string().strip_prefix("k1:b3:").unwrap()
        )],
        opaque
    );
    f.backup
        .contents
        .inventory
        .iter_mut()
        .find(|e| e.id == f.conflict)
        .unwrap()
        .interpretation = Interpretation::Opaque;
    assert!(read(&f).err().unwrap().contains("missing"));
}
#[test]
fn resolved_checkpoint_exports_working_tree_and_retains_selected_tree() {
    let mut f = fixture();
    let Object::Checkpoint(mut cp) = Object::decode(&f.store[&f.checkpoint]).unwrap() else {
        panic!()
    };
    cp.conflicts.clear();
    cp.tree = cp.selected_tree;
    let id = add(&mut f, Object::Checkpoint(cp));
    let b = read(&f).unwrap();
    let mut tar = Vec::new();
    b.archive(&id.to_string()).unwrap().write(&mut tar).unwrap();
    assert_eq!(tar_files(&tar)["file"], b"saved source\n");
}

#[test]
fn nested_backup_and_transfer_records_keep_their_typed_roots() {
    let mut f = fixture();
    let prior = f.backup.clone();
    let nested = add(&mut f, Object::BackupManifest(Box::new(prior)));
    let transfer = TransferManifest {
        repository: [8; 16],
        contents: PackageContents {
            completeness: PackageCompleteness::Standalone,
            roots: vec![PackageRoot {
                id: nested,
                type_code: 16,
            }],
            inventory: inventory(&f.store),
            prerequisites: vec![],
            required_features: vec![],
            optional_features: vec![],
        },
    };
    add(&mut f, Object::TransferManifest(Box::new(transfer)));
    assert!(read(&f).is_ok());
}
