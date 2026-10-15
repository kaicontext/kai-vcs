use kai_vcs::evidence_profiles::*;
use kai_vcs::*;
use std::collections::HashMap;
mod support;
use support::{person, sample};
type Store = HashMap<ObjectId, Vec<u8>>;
fn put(s: &mut Store, o: Object) -> ObjectId {
    let b = o.encode().unwrap();
    let id = ObjectId::hash(&b);
    s.insert(id, b);
    id
}
fn attach(s: &mut Store, e: &mut Evidence, i: &EvidenceInputs, env: &EvidenceEnvironment) {
    e.inputs = MetadataManifest {
        format: INPUT_FORMAT.to_vec(),
        object: put(
            s,
            Object::Blob(Blob {
                data: i.to_bytes().unwrap(),
            }),
        ),
    };
    e.environment = MetadataManifest {
        format: ENVIRONMENT_FORMAT.to_vec(),
        object: put(
            s,
            Object::Blob(Blob {
                data: env.to_bytes().unwrap(),
            }),
        ),
    };
}
fn fixture() -> (Store, Evidence, EvidenceInputs, EvidenceEnvironment) {
    let mut s = Store::new();
    let child = put(
        &mut s,
        Object::Tree(
            Tree::new(vec![TreeEntry {
                name: vec![0xff],
                kind: EntryKind::Gitlink,
                target: Target::GitSha256([3; 32]),
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
            ])
            .unwrap(),
        ),
    );
    let rev = put(
        &mut s,
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
    let Object::Evidence(mut e) = sample("evidence-execution") else {
        panic!()
    };
    e.revision = rev;
    e.tree = tree;
    e.limitations = vec![b"network service unavailable".to_vec()];
    let i = EvidenceInputs {
        schema_version: 1,
        revision: rev.to_string(),
        tree: tree.to_string(),
        dirty_overlay_digest: None,
        dependencies: vec![DependencyInput {
            name: "lockfile".into(),
            digest: format!("b3:{}", "11".repeat(32)),
        }],
        submodules: vec![
            SubmoduleInput {
                path_hex: "612fff".into(),
                algorithm: SubmoduleAlgorithm::Sha256,
                object_id: "03".repeat(32),
            },
            SubmoduleInput {
                path_hex: "622fff".into(),
                algorithm: SubmoduleAlgorithm::Sha256,
                object_id: "03".repeat(32),
            },
        ],
        review: None,
    };
    let env = EvidenceEnvironment {
        schema_version: 1,
        platform: "linux-arm64/test".into(),
        container_digest: None,
        toolchains: vec![ToolchainInput {
            name: "rustc".into(),
            version: "1.99.0".into(),
        }],
        variables: vec![EnvironmentVariable {
            name: "SERVICE_VERSION".into(),
            kind: VariableKind::OpaqueSecretVersion,
            value: "runner-token-1".into(),
        }],
        limitations: vec!["network service unavailable".into()],
    };
    attach(&mut s, &mut e, &i, &env);
    (s, *e, i, env)
}
fn verify(s: &Store, e: &Evidence) -> Result<(EvidenceInputs, EvidenceEnvironment), String> {
    verify_evidence_context(e, ClosureLimits::default(), |id| {
        s.get(&id).cloned().ok_or("missing".into())
    })
}
#[test]
fn profiles_bind_exact_source_and_all_nested_raw_gitlink_paths() {
    let (s, e, i, env) = fixture();
    assert_eq!(verify(&s, &e).unwrap(), (i, env));
    assert_eq!(e.claimed_trust, ClaimedTrust::SelfReported);
}
#[test]
fn changed_profile_source_cannot_reuse_evidence_identity() {
    let (mut s, mut e, mut i, env) = fixture();
    let old = e.id().unwrap();
    i.revision = i.tree.clone();
    attach(&mut s, &mut e, &i, &env);
    assert_ne!(e.id().unwrap(), old);
    assert!(verify(&s, &e).unwrap_err().contains("source mismatch"));
}
#[test]
fn submodules_cannot_be_omitted_substituted_or_invented() {
    for change in 0..3 {
        let (mut s, mut e, mut i, env) = fixture();
        match change {
            0 => {
                i.submodules.pop();
            }
            1 => {
                i.submodules[0].object_id = "04".repeat(32);
            }
            _ => {
                i.submodules[1].path_hex = "63".into();
            }
        }
        attach(&mut s, &mut e, &i, &env);
        assert!(verify(&s, &e).unwrap_err().contains("submodule"));
    }
}
#[test]
fn nonreproducible_environment_limitations_must_be_visible() {
    let (s, mut e, _, _) = fixture();
    e.limitations.clear();
    assert!(verify(&s, &e).unwrap_err().contains("hidden"));
}
#[test]
fn unknown_profiles_missing_blobs_and_corrupted_bytes_fail_closed() {
    let (mut s, mut e, _, _) = fixture();
    e.inputs.format = b"future/v2".to_vec();
    assert!(
        verify_evidence_context(&e, ClosureLimits::default(), |_| panic!(
            "unknown profile read"
        ))
        .is_err()
    );
    e.inputs.format = INPUT_FORMAT.to_vec();
    s.get_mut(&e.inputs.object).unwrap()[0] ^= 1;
    assert!(verify(&s, &e).is_err());
    s.remove(&e.inputs.object);
    assert!(verify(&s, &e).is_err());
}
#[test]
fn canonical_profiles_reject_alternate_encodings_and_invalid_paths() {
    let (_, _, mut i, env) = fixture();
    let b = i.to_bytes().unwrap();
    let mut whitespace = b.clone();
    whitespace.push(b'\n');
    assert!(EvidenceInputs::from_bytes(&whitespace).is_err());
    let text = String::from_utf8(b).unwrap();
    assert!(
        EvidenceInputs::from_bytes(text.replacen("{", "{\"schema_version\":1,", 1).as_bytes())
            .is_err()
    );
    i.submodules[0].path_hex = "2e2e".into();
    assert!(i.to_bytes().is_err());
    assert_eq!(
        EvidenceEnvironment::from_bytes(&env.to_bytes().unwrap()).unwrap(),
        env
    );
}
#[test]
fn environment_values_are_explicit_and_never_auto_collected() {
    let (_, _, _, mut env) = fixture();
    env.variables[0].value.clear();
    assert!(env.to_bytes().is_err());
    env.variables[0].kind = VariableKind::Public;
    assert!(env.to_bytes().is_ok());
    env.variables[0].name = "BAD=NAME".into();
    assert!(env.to_bytes().is_err());
}
#[test]
fn review_profiles_are_required_only_for_matching_review_records() {
    let (mut s, mut e, mut i, env) = fixture();
    e.kind = EvidenceKind::Review;
    e.invocation = vec![b"review-1".to_vec()];
    e.exit_status = None;
    assert!(verify(&s, &e).is_err());
    i.review = Some(ReviewInput {
        identifier: "review-1".into(),
        target_revision: i.revision.clone(),
        policy: ObjectId::from_digest([9; 32]).to_string(),
        base_revision: None,
    });
    attach(&mut s, &mut e, &i, &env);
    assert!(verify(&s, &e).is_ok());
    e.invocation[0] = b"review-2".to_vec();
    assert!(verify(&s, &e).is_err());
}
#[test]
fn context_verification_budgets_include_manifest_and_tree_rereads() {
    let (s, e, _, _) = fixture();
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
        assert!(verify_evidence_context(&e, limits, |id| Ok(s[&id].clone())).is_err());
    }
}
#[test]
fn published_profile_vectors_have_stable_canonical_bytes() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/evidence-profile-cases.json")).unwrap();
    for row in cases.as_array().unwrap() {
        let bytes = row["json"].as_str().unwrap().as_bytes();
        let accepted = if row["kind"] == "inputs" {
            EvidenceInputs::from_bytes(bytes).is_ok()
        } else {
            EvidenceEnvironment::from_bytes(bytes).is_ok()
        };
        assert_eq!(
            accepted,
            row["accepted"].as_bool().unwrap(),
            "{}",
            row["name"]
        );
    }
    let i = include_bytes!("fixtures/evidence-inputs-v1.json");
    assert_eq!(
        EvidenceInputs::from_bytes(i).unwrap().to_bytes().unwrap(),
        i
    );
    let e = include_bytes!("fixtures/evidence-environment-v1.json");
    assert_eq!(
        EvidenceEnvironment::from_bytes(e)
            .unwrap()
            .to_bytes()
            .unwrap(),
        e
    );
}
