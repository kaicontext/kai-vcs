use kai_vcs::*;
use std::collections::HashMap;
mod support;
use support::{id, person, sample};
fn evidence() -> Evidence {
    let Object::Evidence(v) = sample("evidence-execution") else {
        panic!()
    };
    *v
}
fn attestation() -> Attestation {
    let Object::Attestation(v) = sample("attestation-full") else {
        panic!()
    };
    *v
}
fn analysis() -> AnalysisManifest {
    let Object::AnalysisManifest(v) = sample("analysis-partial") else {
        panic!()
    };
    *v
}
fn roundtrip(o: Object) {
    let b = o.encode().unwrap();
    assert_eq!(Object::decode(&b).unwrap(), o);
    assert_eq!(
        kai_object_reference::encode(&kai_object_reference::decode(&b).unwrap()).unwrap(),
        b
    );
}
type Store = HashMap<ObjectId, Vec<u8>>;
fn put(s: &mut Store, o: Object) -> ObjectId {
    let b = o.encode().unwrap();
    let id = ObjectId::hash(&b);
    s.insert(id, b);
    id
}
fn verify(s: &Store, o: Object) -> Result<VerifiedClosure, String> {
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
fn source(s: &mut Store) -> (ObjectId, ObjectId) {
    let tree = put(s, Object::Tree(Tree::new(vec![]).unwrap()));
    let revision = put(
        s,
        Object::Revision(Box::new(Revision {
            tree,
            parents: vec![],
            change_id: [1; 16],
            author: person(),
            committer: person(),
            message: vec![],
            provenance: vec![],
            git_envelope: None,
            conflicts: vec![],
        })),
    );
    (tree, revision)
}
#[test]
fn metadata_all_evidence_outcomes_and_claims_roundtrip_without_promotion() {
    for result in [
        EvidenceResult::Pass,
        EvidenceResult::Fail,
        EvidenceResult::Cancelled,
        EvidenceResult::TimedOut,
        EvidenceResult::Unavailable,
        EvidenceResult::Unknown,
    ] {
        for claimed_trust in [
            ClaimedTrust::Unknown,
            ClaimedTrust::SelfReported,
            ClaimedTrust::RunnerProduced,
        ] {
            let mut v = evidence();
            v.result = result;
            v.claimed_trust = claimed_trust;
            roundtrip(Object::Evidence(Box::new(v)));
        }
    }
    let mut v = evidence();
    v.kind = EvidenceKind::Review;
    v.invocation = vec![b"review:1".to_vec()];
    v.exit_status = None;
    roundtrip(Object::Evidence(Box::new(v.clone())));
    v.exit_status = Some(0);
    assert!(v.encode().is_err());
}
#[test]
fn evidence_input_changes_create_distinct_immutable_records() {
    let v = evidence();
    let original = v.id().unwrap();
    let changes: Vec<fn(&mut Evidence)> = vec![
        |v| v.revision = id(99),
        |v| v.tree = id(99),
        |v| v.inputs.object = id(99),
        |v| v.environment.object = id(99),
        |v| v.scope.selection.push(1),
        |v| v.claimed_trust = ClaimedTrust::RunnerProduced,
        |v| v.result = EvidenceResult::Unknown,
    ];
    for change in changes {
        let mut changed = v.clone();
        change(&mut changed);
        assert_ne!(original, changed.id().unwrap());
    }
}
#[test]
fn evidence_requires_matching_source_and_manifests_but_allows_artifact_expiry() {
    let mut s = Store::new();
    let (tree, revision) = source(&mut s);
    let manifest = put(
        &mut s,
        Object::Blob(Blob {
            data: b"declared manifest bytes".to_vec(),
        }),
    );
    let mut v = evidence();
    v.tree = tree;
    v.revision = revision;
    v.inputs.object = manifest;
    v.environment.object = manifest;
    assert_eq!(
        verify(&s, Object::Evidence(Box::new(v.clone())))
            .unwrap()
            .objects,
        4
    );
    v.source_state = SourceState::Conflicted;
    assert!(
        verify(&s, Object::Evidence(Box::new(v.clone())))
            .unwrap_err()
            .contains("contradicts")
    );
    v.source_state = SourceState::Unknown;
    assert!(verify(&s, Object::Evidence(Box::new(v.clone()))).is_ok());
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
    v.tree = other;
    assert!(
        verify(&s, Object::Evidence(Box::new(v.clone())))
            .unwrap_err()
            .contains("contradicts")
    );
    v.tree = tree;
    s.remove(&manifest);
    assert!(verify(&s, Object::Evidence(Box::new(v))).is_err());
}
#[test]
fn analysis_never_labels_recorded_uncertainty_complete() {
    let mut v = analysis();
    v.completion = AnalysisCompletion::Complete;
    assert!(v.encode().is_err());
    v.unsupported.clear();
    assert!(v.encode().is_err());
    v.parser_failures.clear();
    assert!(v.encode().is_err());
    v.unresolved.clear();
    roundtrip(Object::AnalysisManifest(Box::new(v.clone())));
    v.results.reverse();
    assert!(v.encode().is_err());
    let mut v = analysis();
    v.covered.push(v.covered[0].clone());
    assert!(v.encode().is_err());
    v = analysis();
    v.completion = AnalysisCompletion::Failed;
    roundtrip(Object::AnalysisManifest(Box::new(v)));
}
#[test]
fn analysis_results_can_expire_without_blocking_source_closure() {
    let mut s = Store::new();
    let (tree, _) = source(&mut s);
    let mut v = analysis();
    v.tree = tree;
    assert_eq!(
        verify(&s, Object::AnalysisManifest(Box::new(v)))
            .unwrap()
            .objects,
        2
    );
    assert_eq!(
        verify(&s, Object::Tree(Tree::new(vec![]).unwrap()))
            .unwrap()
            .objects,
        1
    );
}
#[test]
fn attestations_sign_all_context_but_exclude_signature_bytes() {
    let v = attestation();
    let payload = v.signing_payload().unwrap();
    let (_, golden) = include_str!("fixtures/attestation-payloads.tsv")
        .trim()
        .split_once('\t')
        .unwrap();
    let hex: String = payload.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, golden);
    assert!(payload.starts_with(b"KAI-ATTESTATION\0"));
    assert_eq!(
        payload,
        kai_object_reference::attestation_signing_payload(&v.encode().unwrap()).unwrap()
    );
    let changes: Vec<fn(&mut Attestation)> = vec![
        |v| v.format.push(1),
        |v| v.version += 1,
        |v| v.domain.push(1),
        |v| v.subject_type = 9,
        |v| v.subject = id(99),
        |v| v.purpose.push(1),
        |v| v.signer.push(1),
        |v| v.claimed_identity.name.push(1),
        |v| v.issued_seconds = Some(0),
        |v| v.expires_seconds = Some(101),
    ];
    for change in changes {
        let mut changed = v.clone();
        change(&mut changed);
        assert_ne!(payload, changed.signing_payload().unwrap());
    }
    let mut changed = v.clone();
    changed.signature = vec![1, 2, 3];
    assert_eq!(payload, changed.signing_payload().unwrap());
    assert_ne!(v.id().unwrap(), changed.id().unwrap());
    changed.signature.clear();
    assert_eq!(payload, changed.signing_payload().unwrap());
    assert!(changed.encode().is_err());
}
#[test]
fn attestation_expiry_is_signed_and_not_a_decoder_clock_decision() {
    let mut v = attestation();
    v.issued_seconds = Some(i64::MIN);
    v.expires_seconds = Some(-1);
    roundtrip(Object::Attestation(Box::new(v.clone())));
    v.issued_seconds = None;
    roundtrip(Object::Attestation(Box::new(v.clone())));
    v.issued_seconds = Some(2);
    v.expires_seconds = Some(1);
    assert!(v.encode().is_err());
    v.issued_seconds = None;
    v.expires_seconds = None;
    roundtrip(Object::Attestation(Box::new(v.clone())));
    v.version = 0;
    assert!(v.encode().is_err());
}
#[test]
fn attestation_subject_types_and_explicit_metadata_pins_are_verified() {
    let mut s = Store::new();
    let (_, revision) = source(&mut s);
    let mut v = attestation();
    v.subject = revision;
    assert!(verify(&s, Object::Attestation(Box::new(v.clone()))).is_ok());
    v.subject_type = 9;
    assert!(verify(&s, Object::Attestation(Box::new(v.clone()))).is_err());
    v.subject_type = 3;
    let aid = put(&mut s, Object::Attestation(Box::new(v)));
    let view = View {
        retained: vec![RetainedRoot {
            type_code: 10,
            id: aid,
        }],
        ..View::default()
    };
    assert!(verify(&s, Object::View(Box::new(view))).is_ok());
}
