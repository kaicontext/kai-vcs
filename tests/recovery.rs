use kai_vcs::*;
use std::collections::HashMap;
mod support;
use support::{id, person, sample};

type Store = HashMap<ObjectId, Vec<u8>>;
fn put(store: &mut Store, object: Object) -> ObjectId {
    let id = object.id().unwrap();
    store.insert(id, object.encode().unwrap());
    id
}
fn verify(store: &Store, root: ObjectId) -> Result<VerifiedClosure, String> {
    verify_closure(&[root], ClosureLimits::default(), |id| {
        store.get(&id).cloned().ok_or_else(|| "missing".into())
    })
}
fn revision(tree: ObjectId, conflicts: ConflictManifest) -> Revision {
    Revision {
        tree,
        parents: vec![],
        change_id: [1; 16],
        author: person(),
        committer: person(),
        message: vec![],
        provenance: vec![],
        git_envelope: None,
        conflicts,
    }
}
fn checkpoint(tree: ObjectId) -> Checkpoint {
    Checkpoint {
        workspace: [1; 16],
        tree,
        selected_tree: tree,
        capture: CaptureClass::Observed,
        actor: person(),
        inclusion: vec![],
        base: None,
        conflicts: vec![],
        predecessor: None,
        barrier: None,
        selected_conflicts: vec![],
    }
}
fn conflict(path: &[u8], target: ObjectId) -> Conflict {
    Conflict {
        kind: ConflictKind::DeleteModify,
        paths: vec![path.to_vec()],
        bases: vec![],
        sides: vec![
            ConflictVersion {
                label: b"ours".to_vec(),
                states: vec![None],
                source_revision: None,
            },
            ConflictVersion {
                label: b"theirs".to_vec(),
                states: vec![Some(FileState {
                    kind: EntryKind::Regular,
                    target: Target::Native(target),
                })],
                source_revision: None,
            },
        ],
        algorithm: b"diff3/v1".to_vec(),
        policy: b"conservative/v1".to_vec(),
        renames: vec![],
        resolution_provenance: vec![],
    }
}
fn tree(name: &[u8], kind: EntryKind, target: ObjectId) -> Tree {
    Tree::new(vec![TreeEntry {
        name: name.to_vec(),
        kind,
        target: Target::Native(target),
    }])
    .unwrap()
}
fn roundtrip(object: Object) {
    let bytes = object.encode().unwrap();
    assert_eq!(Object::decode(&bytes).unwrap(), object);
    let reference = kai_object_reference::decode(&bytes).unwrap();
    assert_eq!(kai_object_reference::encode(&reference).unwrap(), bytes);
    assert_eq!(
        kai_object_reference::id(&bytes).unwrap(),
        object.id().unwrap().to_string()
    );
}

#[test]
fn t046_conflict_preserves_order_missing_sides_and_multiple_base_candidates() {
    let mut c = conflict(b"file\xff", id(1));
    c.bases = vec![c.sides[1].clone(), c.sides[1].clone()];
    let original = c.id().unwrap();
    roundtrip(Object::Conflict(Box::new(c.clone())));
    c.sides.reverse();
    assert_ne!(original, c.id().unwrap());
    c.bases.pop();
    assert_ne!(original, c.id().unwrap());
    let bytes = c.encode().unwrap();
    let restored = Conflict::decode(&bytes).unwrap();
    assert_eq!(restored.sides[1].states, vec![None]);
    for kind in [
        ConflictKind::Content,
        ConflictKind::Binary,
        ConflictKind::DeleteModify,
        ConflictKind::KindMode,
        ConflictKind::Rename,
        ConflictKind::MultipleBases,
        ConflictKind::AddAdd,
        ConflictKind::Symlink,
        ConflictKind::Gitlink,
    ] {
        c.kind = kind;
        roundtrip(Object::Conflict(Box::new(c.clone())));
    }
}

#[test]
fn invalid_conflict_alignment_paths_and_rename_candidates_are_rejected() {
    let Object::Conflict(c) = sample("conflict-rename") else {
        unreachable!()
    };
    let mut bad = c.clone();
    bad.paths.reverse();
    assert!(bad.encode().is_err());
    let mut bad = c.clone();
    bad.sides[0].states.pop();
    assert!(bad.encode().is_err());
    let mut bad = c.clone();
    bad.sides.pop();
    assert!(bad.encode().is_err());
    let mut bad = c.clone();
    bad.renames[0].target = b"outside".to_vec();
    assert!(bad.encode().is_err());
    let mut bad = c.clone();
    bad.renames.push(bad.renames[0].clone());
    assert!(bad.encode().is_err());
    let mut bad = c.clone();
    bad.algorithm.clear();
    assert!(bad.encode().is_err());
    let mut bad = c.clone();
    bad.sides[0].states[1].as_mut().unwrap().target = Target::GitSha1([0; 20]);
    assert!(bad.encode().is_err());
}

#[test]
fn conflict_alternatives_keep_native_modes_and_external_gitlinks() {
    let mut c = conflict(b"f", id(1));
    for kind in [
        EntryKind::Regular,
        EntryKind::Executable,
        EntryKind::Symlink,
        EntryKind::Directory,
    ] {
        c.sides[1].states[0].as_mut().unwrap().kind = kind;
        roundtrip(Object::Conflict(Box::new(c.clone())));
    }
    c.sides[1].states[0] = Some(FileState {
        kind: EntryKind::Gitlink,
        target: Target::GitSha1([1; 20]),
    });
    roundtrip(Object::Conflict(Box::new(c.clone())));
    assert!(
        Object::Conflict(Box::new(c.clone()))
            .references()
            .is_empty()
    );
    c.sides[1].states[0].as_mut().unwrap().target = Target::GitSha256([2; 32]);
    roundtrip(Object::Conflict(Box::new(c)));
}

#[test]
fn t007_conflict_closure_requires_every_alternative_and_rejects_empty_directories() {
    let mut store = Store::new();
    let blob = put(
        &mut store,
        Object::Blob(Blob {
            data: b"literal <<<<<<< text\0\xff".to_vec(),
        }),
    );
    let mut c = conflict(b"f", blob);
    let root = put(&mut store, Object::Conflict(Box::new(c.clone())));
    assert_eq!(verify(&store, root).unwrap().objects, 2);
    let old = store.remove(&blob).unwrap();
    assert!(verify(&store, root).is_err());
    store.insert(blob, old);
    let empty = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    c.sides[1].states[0] = Some(FileState {
        kind: EntryKind::Directory,
        target: Target::Native(empty),
    });
    let root = put(&mut store, Object::Conflict(Box::new(c)));
    assert!(
        verify(&store, root)
            .unwrap_err()
            .contains("empty directory")
    );
}

#[test]
fn unresolved_revision_cannot_also_contain_a_resolved_file() {
    let mut store = Store::new();
    let blob = put(
        &mut store,
        Object::Blob(Blob {
            data: b"both alternatives survive".to_vec(),
        }),
    );
    let conflict = put(&mut store, Object::Conflict(Box::new(conflict(b"a", blob))));
    let occupied = put(
        &mut store,
        Object::Tree(tree(b"a", EntryKind::Regular, blob)),
    );
    let root = put(
        &mut store,
        Object::Revision(Box::new(revision(
            occupied,
            vec![(b"a".to_vec(), conflict)],
        ))),
    );
    assert!(verify(&store, root).unwrap_err().contains("ordinary tree"));
    let empty = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let root = put(
        &mut store,
        Object::Revision(Box::new(revision(empty, vec![(b"a".to_vec(), conflict)]))),
    );
    assert_eq!(verify(&store, root).unwrap().objects, 4);
}

#[test]
fn conflict_manifest_requires_exact_paths_and_all_rename_bindings() {
    let mut store = Store::new();
    let blob = put(&mut store, Object::Blob(Blob { data: vec![] }));
    let empty = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let mut c = conflict(b"a", blob);
    c.paths.push(b"b".to_vec());
    for v in &mut c.sides {
        v.states.push(None);
    }
    let conflict = put(&mut store, Object::Conflict(Box::new(c)));
    for bindings in [
        vec![(b"a".to_vec(), conflict)],
        vec![(b"x".to_vec(), conflict)],
        vec![
            (b"a".to_vec(), conflict),
            (b"b".to_vec(), conflict),
            (b"c".to_vec(), conflict),
        ],
    ] {
        let root = put(
            &mut store,
            Object::Revision(Box::new(revision(empty, bindings))),
        );
        assert!(
            verify(&store, root)
                .unwrap_err()
                .contains("manifest/path disagreement")
        );
    }
    let root = put(
        &mut store,
        Object::Revision(Box::new(revision(
            empty,
            vec![(b"a".to_vec(), conflict), (b"b".to_vec(), conflict)],
        ))),
    );
    assert!(verify(&store, root).is_ok());
}

#[test]
fn unresolved_nested_path_detects_shadowing_but_allows_missing_ancestors() {
    let mut store = Store::new();
    let blob = put(&mut store, Object::Blob(Blob { data: vec![] }));
    let conflict = put(
        &mut store,
        Object::Conflict(Box::new(conflict(b"a/b", blob))),
    );
    let file_ancestor = put(
        &mut store,
        Object::Tree(tree(b"a", EntryKind::Regular, blob)),
    );
    let root = put(
        &mut store,
        Object::Revision(Box::new(revision(
            file_ancestor,
            vec![(b"a/b".to_vec(), conflict)],
        ))),
    );
    assert!(
        verify(&store, root)
            .unwrap_err()
            .contains("non-directory ancestor")
    );
    let child = put(
        &mut store,
        Object::Tree(tree(b"other", EntryKind::Regular, blob)),
    );
    let parent = put(
        &mut store,
        Object::Tree(tree(b"a", EntryKind::Directory, child)),
    );
    let root = put(
        &mut store,
        Object::Revision(Box::new(revision(
            parent,
            vec![(b"a/b".to_vec(), conflict)],
        ))),
    );
    assert!(verify(&store, root).is_ok());
    let empty = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let root = put(
        &mut store,
        Object::Revision(Box::new(revision(empty, vec![(b"a/b".to_vec(), conflict)]))),
    );
    assert!(verify(&store, root).is_ok());
}

#[test]
fn checkpoint_working_and_selected_conflicts_are_independent() {
    let mut store = Store::new();
    let blob = put(
        &mut store,
        Object::Blob(Blob {
            data: b"staged choice".to_vec(),
        }),
    );
    let conflict = put(&mut store, Object::Conflict(Box::new(conflict(b"a", blob))));
    let empty = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let selected = put(
        &mut store,
        Object::Tree(tree(b"a", EntryKind::Regular, blob)),
    );
    let mut c = checkpoint(empty);
    c.selected_tree = selected;
    c.conflicts = vec![(b"a".to_vec(), conflict)];
    let root = put(&mut store, Object::Checkpoint(Box::new(c.clone())));
    assert!(verify(&store, root).is_ok());
    c.selected_conflicts = c.conflicts.clone();
    let root = put(&mut store, Object::Checkpoint(Box::new(c.clone())));
    assert!(verify(&store, root).unwrap_err().contains("ordinary tree"));
    c.selected_tree = empty;
    let root = put(&mut store, Object::Checkpoint(Box::new(c)));
    assert!(verify(&store, root).is_ok());
}

#[test]
fn expired_checkpoint_predecessor_is_not_permanent_source_ancestry() {
    let mut store = Store::new();
    let empty = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let mut c = checkpoint(empty);
    c.predecessor = Some(id(99));
    let object = Object::Checkpoint(Box::new(c.clone()));
    assert!(
        object
            .references()
            .iter()
            .any(|r| r.id == id(99) && r.class == ClosureClass::RetainedMetadata)
    );
    let root = put(&mut store, object);
    assert!(verify(&store, root).is_ok());
    c.base = Some(id(98));
    let root = put(&mut store, Object::Checkpoint(Box::new(c)));
    assert!(
        verify(&store, root)
            .unwrap_err()
            .contains(&id(98).to_string())
    );
}

#[test]
fn checkpoint_capture_and_inclusion_validation() {
    let mut c = checkpoint(id(1));
    roundtrip(Object::Checkpoint(Box::new(c.clone())));
    c.barrier = Some(id(2));
    assert!(c.encode().is_err());
    c.capture = CaptureClass::BarrierConsistent;
    roundtrip(Object::Checkpoint(Box::new(c.clone())));
    c.inclusion = vec![(b"../outside".to_vec(), InclusionKind::Ignored)];
    assert!(c.encode().is_err());
    c.inclusion = vec![
        (b"a".to_vec(), InclusionKind::Ignored),
        (b"a".to_vec(), InclusionKind::Untracked),
    ];
    assert!(c.encode().is_err());
    c.inclusion = vec![(b"\xff".to_vec(), InclusionKind::Untracked)];
    roundtrip(Object::Checkpoint(Box::new(c)));
}

#[test]
fn operation_audit_links_do_not_pin_entire_recovery_history() {
    let mut store = Store::new();
    let object = sample("operation-full");
    assert!(
        object
            .references()
            .iter()
            .all(|r| r.class == ClosureClass::RetainedMetadata)
    );
    let root = put(&mut store, object);
    assert_eq!(verify(&store, root).unwrap().objects, 1);
    let Object::Operation(mut op) = sample("operation-full") else {
        unreachable!()
    };
    assert!(op.finished_seconds < op.started_seconds);
    roundtrip(Object::Operation(op.clone()));
    op.workspaces.reverse();
    assert!(op.encode().is_err());
    op.workspaces.clear();
    op.predecessors.push(op.predecessors[0]);
    assert!(op.encode().is_err());
    op.predecessors.clear();
    op.external_receipts.clear();
    roundtrip(Object::Operation(op));
}

#[test]
fn change_events_preserve_rewrite_provenance_without_hard_retention_cycles() {
    let Object::ChangeEvent(event) = sample("change-rewrite") else {
        unreachable!()
    };
    assert!(
        Object::ChangeEvent(event.clone())
            .references()
            .iter()
            .all(|r| r.class == ClosureClass::RetainedMetadata)
    );
    for kind in [
        ChangeKind::Rewrite,
        ChangeKind::Split,
        ChangeKind::Squash,
        ChangeKind::CherryPick,
        ChangeKind::Revive,
    ] {
        let mut e = event.clone();
        e.kind = kind;
        roundtrip(Object::ChangeEvent(e));
    }
    let mut create = event.clone();
    create.kind = ChangeKind::Create;
    assert!(create.encode().is_err());
    create.prior.clear();
    roundtrip(Object::ChangeEvent(create));
    let mut abandon = event.clone();
    abandon.kind = ChangeKind::Abandon;
    assert!(abandon.encode().is_err());
    abandon.resulting.clear();
    roundtrip(Object::ChangeEvent(abandon));
    let mut bad = event.clone();
    bad.change_ids.push(bad.change_ids[0]);
    assert!(bad.encode().is_err());
    let mut bad = event;
    bad.resulting.push(bad.resulting[0]);
    assert!(bad.encode().is_err());
}

#[test]
fn annotated_tags_require_source_revision_closure() {
    let mut store = Store::new();
    let empty = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let mut tag = Tag {
        target: empty,
        message: b"release\xff".to_vec(),
        tagger: person(),
        git_envelope: None,
    };
    let root = put(&mut store, Object::Tag(tag.clone()));
    assert!(
        verify(&store, root)
            .unwrap_err()
            .contains("wrong object type")
    );
    let revision = put(
        &mut store,
        Object::Revision(Box::new(revision(empty, vec![]))),
    );
    tag.target = revision;
    roundtrip(Object::Tag(tag.clone()));
    let root = put(&mut store, Object::Tag(tag));
    assert_eq!(verify(&store, root).unwrap().objects, 3);
}

#[test]
fn conflict_path_resolution_obeys_the_closure_work_budget() {
    let mut store = Store::new();
    let blob = put(&mut store, Object::Blob(Blob { data: vec![] }));
    let conflict = put(
        &mut store,
        Object::Conflict(Box::new(conflict(b"a/b/c", blob))),
    );
    let leaf = put(
        &mut store,
        Object::Tree(tree(b"other", EntryKind::Regular, blob)),
    );
    let middle = put(
        &mut store,
        Object::Tree(tree(b"b", EntryKind::Directory, leaf)),
    );
    let parent = put(
        &mut store,
        Object::Tree(tree(b"a", EntryKind::Directory, middle)),
    );
    let root = put(
        &mut store,
        Object::Revision(Box::new(revision(
            parent,
            vec![(b"a/b/c".to_vec(), conflict)],
        ))),
    );
    assert!(verify(&store, root).is_ok());
    let result = verify_closure(
        &[root],
        ClosureLimits {
            max_edges: 7,
            ..ClosureLimits::default()
        },
        |id| store.get(&id).cloned().ok_or_else(|| "missing".into()),
    );
    assert!(result.unwrap_err().contains("path work limit"));
}
