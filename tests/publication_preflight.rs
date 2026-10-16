use kai_vcs::{
    DraftHeads, ObjectId, RefState, RetainedRoot, View, ViewRef, WorkspaceRoot,
    interface::Durability, publication_preflight::PublicationPlan,
    publication_receipt::PublicationDurability, update_refs::*,
};
fn id(c: char) -> ObjectId {
    format!("k1:b3:{}", c.to_string().repeat(64))
        .parse()
        .unwrap()
}
fn request() -> UpdateRefsRequest {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/update-refs.json")).unwrap();
    serde_json::from_value::<UpdateRefsIntent>(rows[0]["intent"].clone())
        .unwrap()
        .into_request()
        .unwrap()
}
fn current() -> View {
    View {
        namespace_generation: 9,
        refs: vec![
            ViewRef {
                name: "branches/main".into(),
                generation: 4,
                state: RefState::Target(id('a')),
            },
            ViewRef {
                name: "tags/old".into(),
                generation: 7,
                state: RefState::Target(id('c')),
            },
        ],
        drafts: vec![DraftHeads {
            change_id: [1; 16],
            heads: vec![id('a')],
        }],
        workspaces: vec![WorkspaceRoot {
            workspace: [2; 16],
            base: Some(id('a')),
            checkpoint: None,
        }],
        retained: vec![RetainedRoot {
            type_code: 3,
            id: id('a'),
        }],
    }
}
fn durability() -> PublicationDurability {
    PublicationDurability {
        class: Durability::SingleNodeDurable,
        replication_policy: None,
    }
}
fn prepare(view: &View, request: &UpdateRefsRequest) -> Result<PublicationPlan, String> {
    PublicationPlan::prepare(
        &request.intent.repository_id,
        view,
        request,
        id('f').to_string(),
        durability(),
    )
}
#[test]
fn batch_is_a_pure_preview_with_tombstone_and_preserved_unrelated_roots() {
    let view = current();
    let before = view.encode().unwrap();
    let request = request();
    let plan = prepare(&view, &request).unwrap();
    assert_eq!(view.encode().unwrap(), before);
    let next = plan.candidate_view();
    assert_eq!(next.namespace_generation, 10);
    assert_eq!(next.refs[0].generation, 5);
    assert_eq!(next.refs[0].state, RefState::Target(id('b')));
    assert_eq!(next.refs[1].generation, 8);
    assert_eq!(next.refs[1].state, RefState::Deleted);
    assert_eq!(next.drafts, view.drafts);
    assert_eq!(next.workspaces, view.workspaces);
    assert_eq!(next.retained, view.retained);
    assert_eq!(View::decode(&next.encode().unwrap()).unwrap(), *next);
    plan.prospective_receipt()
        .validate(&request, &durability())
        .unwrap();
    assert!(plan.committed_status_bytes() <= 1024 * 1024);
    plan.recheck_snapshot(&request.intent.repository_id, &view)
        .unwrap();
    assert!(
        plan.recheck_snapshot(&request.intent.repository_id, next)
            .is_err()
    );
}
#[test]
fn stale_batch_rejects_every_change_and_detects_aba() {
    let view = current();
    let before = view.clone();
    let mut q = request();
    q.intent.updates[1].expected_generation = "6".into();
    q = q.intent.into_request().unwrap();
    assert_eq!(
        prepare(&view, &q).unwrap_err(),
        "stale publication ref: tags/old"
    );
    assert_eq!(view, before);
    let mut aba = view.clone();
    aba.refs[0].generation = 6; // A -> B -> A; same target, later counter
    assert_eq!(
        prepare(&aba, &request()).unwrap_err(),
        "stale publication ref: branches/main"
    );
    let mut q = request();
    q.intent.updates[0].expected_old_targets = vec![id('b').to_string()];
    q = q.intent.into_request().unwrap();
    assert_eq!(
        prepare(&view, &q).unwrap_err(),
        "stale publication ref: branches/main"
    );
    let mut newer = view;
    newer.namespace_generation += 1;
    assert_eq!(
        prepare(&newer, &request()).unwrap_err(),
        "stale publication namespace generation"
    );
}
#[test]
fn absent_creation_and_tombstone_recreation_are_distinct_and_keep_alternatives() {
    let mut q = request();
    q.intent.updates.truncate(1);
    q.intent.updates[0].expected_generation = "0".into();
    q.intent.updates[0].expected_old_targets.clear();
    q.intent.updates[0].new_targets.push(id('c').to_string());
    q = q.intent.into_request().unwrap();
    let mut view = current();
    view.refs.remove(0);
    let plan = prepare(&view, &q).unwrap();
    assert_eq!(plan.candidate_view().refs[0].generation, 1);
    assert_eq!(
        plan.candidate_view().refs[0].state,
        RefState::DraftConflict(vec![id('b'), id('c')])
    );
    view.refs.insert(
        0,
        ViewRef {
            name: "branches/main".into(),
            generation: 8,
            state: RefState::Deleted,
        },
    );
    assert!(prepare(&view, &q).is_err());
    q.intent.updates[0].expected_generation = "8".into();
    q = q.intent.into_request().unwrap();
    assert_eq!(
        prepare(&view, &q).unwrap().candidate_view().refs[0].generation,
        9
    );
    view.refs.remove(0);
    assert!(prepare(&view, &q).is_err());
}
#[test]
fn recheck_covers_full_view_and_repository_not_just_ref_generation() {
    let view = current();
    let q = request();
    let plan = prepare(&view, &q).unwrap();
    let mut changed = view.clone();
    changed.workspaces[0].checkpoint = Some(id('b'));
    assert!(
        plan.recheck_snapshot(&q.intent.repository_id, &changed)
            .is_err()
    );
    assert!(plan.recheck_snapshot(&"0".repeat(32), &view).is_err());
    assert!(
        PublicationPlan::prepare(
            &"0".repeat(32),
            &view,
            &q,
            id('f').to_string(),
            durability()
        )
        .is_err()
    );
    let mut invalid = view;
    invalid.refs.reverse();
    assert!(prepare(&invalid, &q).is_err());
    let mut tampered = q;
    tampered.intent.reason = "changed".into();
    assert!(prepare(&current(), &tampered).is_err());
}
#[test]
fn overflow_refuses_a_candidate_before_any_write() {
    let mut view = current();
    let mut q = request();
    view.namespace_generation = u64::MAX;
    q.intent.expected_namespace_generation = u64::MAX.to_string();
    q = q.intent.into_request().unwrap();
    assert_eq!(
        prepare(&view, &q).unwrap_err(),
        "namespace generation exhausted"
    );
    view = current();
    q = request();
    view.refs[0].generation = u64::MAX;
    q.intent.updates[0].expected_generation = u64::MAX.to_string();
    q = q.intent.into_request().unwrap();
    assert_eq!(prepare(&view, &q).unwrap_err(), "ref generation exhausted");
}
#[test]
fn fitting_receipt_but_oversized_status_is_rejected_before_publication() {
    let mut intent = request().intent;
    intent.updates = (0..1000)
        .map(|i| RefUpdate {
            name: format!("branches/{i:04}"),
            expected_generation: "0".into(),
            expected_old_targets: vec![],
            new_targets: vec![id('a').to_string()],
        })
        .collect();
    let q = intent.clone().into_request().unwrap();
    let plan = prepare(&current(), &q).unwrap();
    let receipt_size = serde_json::to_vec(plan.prospective_receipt())
        .unwrap()
        .len();
    let padding = 1024 * 1024 - 100 - receipt_size;
    intent
        .updates
        .last_mut()
        .unwrap()
        .name
        .push_str(&"x".repeat(padding));
    let q = intent.into_request().unwrap();
    assert!(q.to_json().unwrap().len() <= 1024 * 1024);
    let mut matching = plan.prospective_receipt().clone();
    matching.request_digest = q.request_digest.clone();
    matching.refs.last_mut().unwrap().name = q.intent.updates.last().unwrap().name.clone();
    assert_eq!(
        matching.to_json(&q, &durability()).unwrap().len(),
        1024 * 1024 - 100
    );
    assert_eq!(
        prepare(&current(), &q).unwrap_err(),
        "receipt-status envelope size limit"
    );
}
