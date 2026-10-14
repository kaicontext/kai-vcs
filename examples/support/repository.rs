use kai_vcs::*;
use std::collections::BTreeMap;
pub type Store = BTreeMap<ObjectId, Vec<u8>>;
pub fn person() -> Identity {
    Identity {
        name: b"Demo".to_vec(),
        email: b"demo@example.test".to_vec(),
        timestamp_seconds: 0,
        timezone_minutes: 0,
    }
}
pub fn put(s: &mut Store, o: Object) -> ObjectId {
    let b = o.encode().unwrap();
    let id = ObjectId::hash(&b);
    s.insert(id, b);
    id
}
pub fn inventory(s: &Store) -> Vec<InventoryEntry> {
    s.iter()
        .map(|(id, b)| InventoryEntry {
            id: *id,
            type_code: Object::decode(b).unwrap().type_code(),
            framed_length: b.len() as u64,
            interpretation: Interpretation::Interpreted,
        })
        .collect()
}
pub struct Fixture {
    pub store: Store,
    pub backup: BackupManifest,
    pub view: ObjectId,
    pub checkpoint: ObjectId,
    pub conflict: ObjectId,
    pub revision: ObjectId,
    pub resolved: ObjectId,
}
pub fn fixture() -> Fixture {
    let mut store = Store::new();
    let blob = put(
        &mut store,
        Object::Blob(Blob {
            data: b"saved source\n".to_vec(),
        }),
    );
    let alt = put(
        &mut store,
        Object::Blob(Blob {
            data: b"alternative edit\n".to_vec(),
        }),
    );
    let tree = put(
        &mut store,
        Object::Tree(
            Tree::new(vec![TreeEntry {
                name: b"file".to_vec(),
                kind: EntryKind::Regular,
                target: Target::Native(blob),
            }])
            .unwrap(),
        ),
    );
    let empty = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let mut rev = Revision {
        tree,
        parents: vec![],
        change_id: [1; 16],
        author: person(),
        committer: person(),
        message: b"resolved base".to_vec(),
        provenance: vec![],
        git_envelope: None,
        conflicts: vec![],
    };
    let resolved = put(&mut store, Object::Revision(Box::new(rev.clone())));
    let conflict = put(
        &mut store,
        Object::Conflict(Box::new(Conflict {
            kind: ConflictKind::DeleteModify,
            paths: vec![b"file".to_vec()],
            bases: vec![ConflictVersion {
                label: b"base".to_vec(),
                states: vec![Some(FileState {
                    kind: EntryKind::Regular,
                    target: Target::Native(blob),
                })],
                source_revision: Some(resolved),
            }],
            sides: vec![
                ConflictVersion {
                    label: b"deleted".to_vec(),
                    states: vec![None],
                    source_revision: None,
                },
                ConflictVersion {
                    label: b"edited".to_vec(),
                    states: vec![Some(FileState {
                        kind: EntryKind::Executable,
                        target: Target::Native(alt),
                    })],
                    source_revision: None,
                },
            ],
            algorithm: b"demo/v1".to_vec(),
            policy: b"preserve alternatives".to_vec(),
            renames: vec![],
            resolution_provenance: vec![],
        })),
    );
    rev.tree = empty;
    rev.parents = vec![resolved];
    rev.conflicts = vec![(b"file".to_vec(), conflict)];
    rev.message = b"unresolved draft".to_vec();
    let revision = put(&mut store, Object::Revision(Box::new(rev)));
    let checkpoint = put(
        &mut store,
        Object::Checkpoint(Box::new(Checkpoint {
            workspace: [2; 16],
            tree: empty,
            selected_tree: tree,
            capture: CaptureClass::Observed,
            actor: person(),
            inclusion: vec![],
            base: Some(resolved),
            conflicts: vec![(b"file".to_vec(), conflict)],
            predecessor: Some(ObjectId::from_digest([99; 32])),
            barrier: None,
            selected_conflicts: vec![],
        })),
    );
    let old = put(&mut store, Object::View(Box::default()));
    let mut heads = vec![resolved, revision];
    heads.sort();
    let view = put(
        &mut store,
        Object::View(Box::new(View {
            namespace_generation: 9,
            refs: vec![
                ViewRef {
                    name: "branches/draft".into(),
                    generation: 3,
                    state: RefState::DraftConflict(heads.clone()),
                },
                ViewRef {
                    name: "branches/main".into(),
                    generation: 1,
                    state: RefState::Target(resolved),
                },
                ViewRef {
                    name: "branches/removed".into(),
                    generation: 4,
                    state: RefState::Deleted,
                },
                ViewRef {
                    name: "shelves/work".into(),
                    generation: 1,
                    state: RefState::Target(checkpoint),
                },
            ],
            drafts: vec![DraftHeads {
                change_id: [1; 16],
                heads,
            }],
            workspaces: vec![WorkspaceRoot {
                workspace: [2; 16],
                base: Some(resolved),
                checkpoint: Some(checkpoint),
            }],
            retained: vec![RetainedRoot {
                type_code: 6,
                id: old,
            }],
        })),
    );
    let operation = put(
        &mut store,
        Object::Operation(Box::new(Operation {
            operation_id: [3; 16],
            predecessors: vec![ObjectId::from_digest([98; 32])],
            pre_view: old,
            post_view: view,
            action: b"save-demo".to_vec(),
            actor: person(),
            started_seconds: 0,
            finished_seconds: 1,
            workspaces: vec![[2; 16]],
            intent_id: [4; 16],
            external_receipts: vec![],
        })),
    );
    let backup = BackupManifest {
        repository: [5; 16],
        generation: 9,
        current_view: view,
        current_operation: Some(operation),
        contents: PackageContents {
            completeness: PackageCompleteness::Standalone,
            roots: vec![],
            inventory: inventory(&store),
            prerequisites: vec![],
            required_features: vec![],
            optional_features: vec![],
        },
    };
    Fixture {
        store,
        backup,
        view,
        checkpoint,
        conflict,
        revision,
        resolved,
    }
}
pub fn container(f: &Fixture) -> Vec<u8> {
    let mut b = b"KAI-BUNDLE\0\0\x01".to_vec();
    for data in std::iter::once(f.backup.encode().unwrap()).chain(
        f.backup
            .contents
            .inventory
            .iter()
            .map(|e| f.store[&e.id].clone()),
    ) {
        b.extend((data.len() as u64).to_be_bytes());
        b.extend(data);
    }
    b
}

/// Same repository state represented by content-addressed view submaps.
pub fn partitioned_fixture() -> Fixture {
    let mut f = fixture();
    let Object::View(view) = Object::decode(&f.store.remove(&f.view).unwrap()).unwrap() else {
        unreachable!()
    };
    let (root, submaps) = view.partition().unwrap();
    for submap in submaps {
        put(&mut f.store, Object::View(Box::new(submap)));
    }
    f.view = put(&mut f.store, Object::PartitionedView(Box::new(root)));
    let old_operation = f.backup.current_operation.unwrap();
    let Object::Operation(mut op) =
        Object::decode(&f.store.remove(&old_operation).unwrap()).unwrap()
    else {
        unreachable!()
    };
    op.post_view = f.view;
    f.backup.current_operation = Some(put(&mut f.store, Object::Operation(op)));
    f.backup.current_view = f.view;
    f.backup.contents.inventory = inventory(&f.store);
    f
}
