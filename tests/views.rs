use kai_vcs::*;
use std::collections::HashMap;
mod support;
use support::{id, person, sample};

type Store = HashMap<ObjectId, Vec<u8>>;
fn put(store: &mut Store, object: Object) -> ObjectId {
    let bytes = object.encode().unwrap();
    let id = ObjectId::hash(&bytes);
    store.insert(id, bytes);
    id
}
fn verify(store: &Store, view: View) -> Result<VerifiedClosure, String> {
    let bytes = view.encode()?;
    let root = ObjectId::hash(&bytes);
    verify_closure(&[root], ClosureLimits::default(), |id| {
        if id == root {
            Ok(bytes.clone())
        } else {
            store.get(&id).cloned().ok_or("missing".into())
        }
    })
}
fn reference(name: &str, state: RefState) -> ViewRef {
    ViewRef {
        name: name.into(),
        generation: 1,
        state,
    }
}
fn with_ref(name: &str, state: RefState) -> View {
    View {
        refs: vec![reference(name, state)],
        ..View::default()
    }
}
fn revision(tree: ObjectId, change_id: [u8; 16], message: &[u8]) -> Object {
    Object::Revision(Box::new(Revision {
        tree,
        parents: vec![],
        change_id,
        author: person(),
        committer: person(),
        message: message.to_vec(),
        provenance: vec![],
        git_envelope: None,
        conflicts: vec![],
    }))
}
fn checkpoint(tree: ObjectId) -> Object {
    Object::Checkpoint(Box::new(Checkpoint {
        workspace: [1; 16],
        tree,
        selected_tree: tree,
        capture: CaptureClass::Observed,
        actor: person(),
        inclusion: vec![],
        base: None,
        conflicts: vec![],
        predecessor: Some(id(99)),
        barrier: None,
        selected_conflicts: vec![],
    }))
}
#[test]
fn view_refs_require_nfc_names_without_silent_rewriting() {
    for name in [
        "branches/é",
        "branches/שלום",
        "branches/one/two",
        "tags/V1",
        "remotes/peer/branches/main",
        "shelves/wip",
    ] {
        let view = with_ref(name, RefState::Deleted);
        let bytes = view.encode().unwrap();
        assert_eq!(View::decode(&bytes).unwrap(), view);
        assert!(kai_object_reference::decode(&bytes).is_ok());
    }
    for name in [
        "branches/e\u{301}",
        "branches/..",
        "branches/./x",
        "branches//x",
        "branches/a/",
        "branches/\0",
        "branches/a\nb",
        "branches/\\x",
        "branches",
        "drafts/123",
        "internal/current",
        "remotes/peer/main",
        "tags/\u{85}",
    ] {
        assert!(
            with_ref(name, RefState::Deleted).encode().is_err(),
            "{name:?}"
        );
    }
    let mut view = with_ref("branches/A", RefState::Deleted);
    view.refs.push(reference("branches/a", RefState::Deleted));
    assert!(view.encode().is_ok()); // case-sensitive, distinct names
}
#[test]
fn view_tombstones_preserve_generations_without_retaining_deleted_content() {
    let store = Store::new();
    let mut view = with_ref("branches/main", RefState::Deleted);
    assert_eq!(verify(&store, view.clone()).unwrap().objects, 1);
    let old = view.id().unwrap();
    view.refs[0].generation = 2;
    assert_ne!(old, view.id().unwrap());
    view.refs[0].generation = 0;
    assert!(view.encode().is_err());
    view.refs[0].generation = u64::MAX;
    assert!(view.encode().is_ok());
}
#[test]
fn view_order_and_alternative_cardinality_are_canonical() {
    let Object::View(full) = sample("view-full") else {
        panic!()
    };
    let mut view = *full;
    view.refs.swap(0, 1);
    assert!(view.encode().is_err());
    view.refs.clear();
    view.drafts.push(view.drafts[0].clone());
    assert!(view.encode().is_err());
    view.drafts.clear();
    view.workspaces.reverse();
    assert!(view.encode().is_err());
    view.workspaces.clear();
    view.retained.reverse();
    assert!(view.encode().is_err());
    for heads in [vec![], vec![id(1)], vec![id(1), id(1)], vec![id(2), id(1)]] {
        assert!(
            with_ref("branches/main", RefState::DraftConflict(heads))
                .encode()
                .is_err()
        );
    }
    for name in ["tags/v1", "shelves/wip", "remotes/origin/branches/main"] {
        assert!(
            with_ref(name, RefState::DraftConflict(vec![id(1), id(2)]))
                .encode()
                .is_err()
        );
    }
}
#[test]
fn view_all_divergent_heads_are_required_and_bound_to_the_change() {
    let mut store = Store::new();
    let tree = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let a = put(&mut store, revision(tree, [1; 16], b"a"));
    let b = put(&mut store, revision(tree, [1; 16], b"b"));
    let mut heads = vec![a, b];
    heads.sort();
    let mut view = with_ref("branches/main", RefState::DraftConflict(heads.clone()));
    view.drafts.push(DraftHeads {
        change_id: [1; 16],
        heads,
    });
    assert_eq!(verify(&store, view.clone()).unwrap().objects, 4);
    view.drafts[0].change_id = [2; 16];
    assert!(
        verify(&store, view.clone())
            .unwrap_err()
            .contains("change ID")
    );
    view.drafts[0].change_id = [1; 16];
    store.remove(&b);
    assert!(verify(&store, view).is_err());
}
#[test]
fn view_targets_are_typed_even_when_already_seen() {
    let mut store = Store::new();
    let tree = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let checkpoint = put(&mut store, checkpoint(tree));
    assert!(
        verify(
            &store,
            with_ref("shelves/wip", RefState::Target(checkpoint))
        )
        .is_ok()
    );
    for name in ["branches/main", "tags/v1", "remotes/origin/branches/main"] {
        let mut view = with_ref(name, RefState::Target(checkpoint));
        view.workspaces.push(WorkspaceRoot {
            workspace: [1; 16],
            base: None,
            checkpoint: Some(checkpoint),
        });
        assert!(verify(&store, view).is_err());
    }
    let rev = put(&mut store, revision(tree, [1; 16], b""));
    let tag = put(
        &mut store,
        Object::Tag(Tag {
            target: rev,
            message: vec![],
            tagger: person(),
            git_envelope: None,
        }),
    );
    assert!(verify(&store, with_ref("tags/v1", RefState::Target(tag))).is_ok());
    assert!(verify(&store, with_ref("branches/main", RefState::Target(tag))).is_err());
}
#[test]
fn view_workspace_checkpoint_is_private_typed_and_workspace_bound() {
    let mut store = Store::new();
    let tree = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let cp = put(&mut store, checkpoint(tree));
    let mut view = View {
        workspaces: vec![WorkspaceRoot {
            workspace: [1; 16],
            base: None,
            checkpoint: Some(cp),
        }],
        ..View::default()
    };
    assert_eq!(verify(&store, view.clone()).unwrap().objects, 3);
    view.workspaces[0].workspace = [2; 16];
    assert!(verify(&store, view).unwrap_err().contains("workspace ID"));
}
#[test]
fn view_explicit_pins_require_targets_without_recursing_through_archival_links() {
    let mut store = Store::new();
    let op = put(&mut store, sample("operation-full")); // unavailable pre/post views and predecessor operations
    let mut view = View {
        retained: vec![RetainedRoot {
            type_code: 7,
            id: op,
        }],
        ..View::default()
    };
    assert_eq!(verify(&store, view.clone()).unwrap().objects, 2);
    view.retained[0].type_code = 6;
    assert!(verify(&store, view.clone()).is_err());
    view.retained[0].type_code = 7;
    store.remove(&op);
    assert!(verify(&store, view).is_err());
}
#[test]
fn view_pin_expiry_is_an_explicit_new_view() {
    let mut store = Store::new();
    let old = put(&mut store, Object::View(Box::default()));
    let mut view = View {
        retained: vec![RetainedRoot {
            type_code: 6,
            id: old,
        }],
        namespace_generation: 1,
        ..View::default()
    };
    let pinned_id = view.id().unwrap();
    assert_eq!(verify(&store, view.clone()).unwrap().objects, 2);
    view.retained.clear();
    assert_ne!(pinned_id, view.id().unwrap());
    store.clear();
    assert_eq!(verify(&store, view).unwrap().objects, 1);
}
#[test]
fn view_closure_limits_apply_to_many_live_roots() {
    let mut store = Store::new();
    let tree = put(&mut store, Object::Tree(Tree::new(vec![]).unwrap()));
    let rev = put(&mut store, revision(tree, [1; 16], b""));
    let view = View {
        refs: (0..10)
            .map(|n| reference(&format!("branches/{n}"), RefState::Target(rev)))
            .collect(),
        ..View::default()
    };
    let root = put(&mut store, Object::View(Box::new(view)));
    assert!(
        verify_closure(
            &[root],
            ClosureLimits {
                max_edges: 5,
                ..ClosureLimits::default()
            },
            |id| Ok(store[&id].clone())
        )
        .is_err()
    );
}

#[test]
fn view_both_decoders_reject_noncanonical_and_invalid_wire_names() {
    let valid = with_ref("branches/x", RefState::Deleted).encode().unwrap();
    let start = valid.windows(10).position(|w| w == b"branches/x").unwrap();
    for name in [
        b"branches/\xff".as_slice(),
        b"branches/..",
        "branches/e\u{301}".as_bytes(),
        b"internal/x",
        b"branches/\0",
        b"branches//x",
    ] {
        assert!(name.len() < 24);
        let mut bytes = valid.clone();
        let mut encoded = vec![0x40 | name.len() as u8];
        encoded.extend_from_slice(name);
        bytes.splice(start - 1..start + 10, encoded);
        let length = (bytes.len() - 23) as u64;
        bytes[15..23].copy_from_slice(&length.to_be_bytes());
        assert!(View::decode(&bytes).is_err(), "{name:?}");
        assert!(kai_object_reference::decode(&bytes).is_err(), "{name:?}");
    }
    let mut unknown_representation = View::default().encode().unwrap();
    *unknown_representation.last_mut().unwrap() = 1;
    assert!(View::decode(&unknown_representation).is_err());
    assert!(kai_object_reference::decode(&unknown_representation).is_err());
}
