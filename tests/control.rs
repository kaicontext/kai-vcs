use kai_vcs::*;
use std::collections::HashMap;
mod support;
use support::{person, sample};
fn barrier() -> CollaborationBarrier {
    let Object::CollaborationBarrier(v) = sample("barrier-full") else {
        panic!()
    };
    *v
}
fn policy() -> Policy {
    let Object::Policy(v) = sample("policy-full") else {
        panic!()
    };
    *v
}
fn put(s: &mut HashMap<ObjectId, Vec<u8>>, o: Object) -> ObjectId {
    let b = o.encode().unwrap();
    let id = ObjectId::hash(&b);
    s.insert(id, b);
    id
}
fn verify(s: &HashMap<ObjectId, Vec<u8>>, o: Object) -> Result<VerifiedClosure, String> {
    let b = o.encode()?;
    let root = ObjectId::hash(&b);
    verify_closure(&[root], ClosureLimits::default(), |id| {
        if id == root {
            Ok(b.clone())
        } else {
            s.get(&id).cloned().ok_or("missing".into())
        }
    })
}
#[test]
fn barrier_frontiers_reject_duplicates_overlap_and_silent_exclusion() {
    let mut v = barrier();
    v.frontier.clear();
    assert!(v.encode().is_err());
    v = barrier();
    v.frontier.reverse();
    assert!(v.encode().is_err());
    v = barrier();
    v.frontier.push(v.frontier[0]);
    assert!(v.encode().is_err());
    v = barrier();
    v.excluded[0].0 = v.frontier[0].0;
    assert!(v.encode().is_err());
    v = barrier();
    v.excluded[0].1.clear();
    assert!(v.encode().is_err());
    v = barrier();
    let old = v.id().unwrap();
    v.frontier[0].1 = 1;
    assert_ne!(old, v.id().unwrap());
}
#[test]
fn checkpoint_barrier_requires_matching_workspace_tree_and_frontier_bytes() {
    let mut s = HashMap::new();
    let tree = put(&mut s, Object::Tree(Tree::new(vec![]).unwrap()));
    let manifest = put(
        &mut s,
        Object::Blob(Blob {
            data: b"opaque frontier".to_vec(),
        }),
    );
    let mut v = barrier();
    v.tree = tree;
    v.manifest = manifest;
    let id = put(&mut s, Object::CollaborationBarrier(Box::new(v)));
    let mut cp = Checkpoint {
        workspace: [1; 16],
        tree,
        selected_tree: tree,
        capture: CaptureClass::BarrierConsistent,
        actor: person(),
        inclusion: vec![],
        base: None,
        conflicts: vec![],
        predecessor: None,
        barrier: Some(id),
        selected_conflicts: vec![],
    };
    assert_eq!(
        verify(&s, Object::Checkpoint(Box::new(cp.clone())))
            .unwrap()
            .objects,
        4
    );
    cp.workspace = [2; 16];
    assert!(
        verify(&s, Object::Checkpoint(Box::new(cp.clone())))
            .unwrap_err()
            .contains("mismatch")
    );
    cp.workspace = [1; 16];
    let other = put(
        &mut s,
        Object::Tree(
            Tree::new(vec![TreeEntry {
                name: b"f".to_vec(),
                kind: EntryKind::Regular,
                target: Target::Native(manifest),
            }])
            .unwrap(),
        ),
    );
    cp.tree = other;
    assert!(verify(&s, Object::Checkpoint(Box::new(cp.clone()))).is_err());
    cp.tree = tree;
    cp.selected_tree = other;
    assert!(verify(&s, Object::Checkpoint(Box::new(cp.clone()))).is_ok());
    cp.capture = CaptureClass::Observed;
    assert!(cp.encode().is_err());
    cp.capture = CaptureClass::BarrierConsistent;
    s.remove(&manifest);
    assert!(verify(&s, Object::Checkpoint(Box::new(cp))).is_err());
}
#[test]
fn policies_validate_exact_nfc_refs_and_never_accept_internal_names() {
    for name in ["branches/main", "branches/é", "tags/release/1"] {
        let mut p = policy();
        p.protected_refs = vec![name.as_bytes().to_vec()];
        let b = p.encode().unwrap();
        assert!(kai_object_reference::decode(&b).is_ok());
    }
    for name in [
        "branches/e\u{301}",
        "remotes/origin/branches/main",
        "shelves/wip",
        "branches/..",
        "branches//x",
        "tags/\0",
    ] {
        let mut p = policy();
        p.protected_refs = vec![name.as_bytes().to_vec()];
        assert!(p.encode().is_err());
    }
    let mut p = policy();
    p.required_checks.reverse();
    assert!(p.encode().is_err());
    p = policy();
    p.revision = 0;
    assert!(p.encode().is_err());
}
#[test]
fn policy_modes_roundtrip_without_policy_installation_or_ancestral_retention() {
    for mode in [
        SelectiveCi::Disabled,
        SelectiveCi::FullOnUnknown,
        SelectiveCi::RejectUnknown,
    ] {
        for rewrite in [RewritePolicy::Deny, RewritePolicy::Administrative] {
            let mut p = policy();
            p.selective_ci = mode;
            p.rewrite = rewrite;
            let b = p.encode().unwrap();
            assert_eq!(Policy::decode(&b).unwrap(), p);
            assert!(kai_object_reference::decode(&b).is_ok());
            assert_eq!(
                verify(&HashMap::new(), Object::Policy(Box::new(p)))
                    .unwrap()
                    .objects,
                1
            );
        }
    }
    let mut p = policy();
    p.predecessor = None;
    p.allowed_principals.clear();
    p.required_checks.clear();
    p.protected_refs.clear();
    p.required_features.clear();
    p.required_reviewers = 0;
    assert!(p.encode().is_ok());
}
#[test]
fn immutable_policy_changes_have_distinct_ids() {
    let p = policy();
    let mut changed = p.clone();
    changed.required_reviewers += 1;
    assert_ne!(p.id().unwrap(), changed.id().unwrap());
    changed = p.clone();
    changed.allowed_principals.push(b"key:b".to_vec());
    assert_ne!(p.id().unwrap(), changed.id().unwrap());
}
