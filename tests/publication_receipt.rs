use kai_vcs::{interface::Durability, publication_receipt::*, update_refs::*};
use serde_json::Value;
fn request() -> UpdateRefsRequest {
    let rows: Value = serde_json::from_str(include_str!("fixtures/update-refs.json")).unwrap();
    let intent: UpdateRefsIntent = serde_json::from_value(rows[0]["intent"].clone()).unwrap();
    intent.into_request().unwrap()
}
fn rows() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/publication-receipt.json")).unwrap()
}
fn expected() -> PublicationDurability {
    PublicationDurability {
        class: Durability::SingleNodeDurable,
        replication_policy: None,
    }
}
fn receipt() -> PublicationReceipt {
    PublicationReceipt::from_json(
        rows()[0]["receipt"].as_str().unwrap().as_bytes(),
        &request(),
        &expected(),
    )
    .unwrap()
}
#[test]
fn shared_corpus_matches_and_accepted_receipts_roundtrip() {
    for row in rows() {
        let check = || -> Result<(), String> {
            let expected: PublicationDurability =
                serde_json::from_value(row["expected_durability"].clone())
                    .map_err(|e| e.to_string())?;
            let request = request();
            let r = PublicationReceipt::from_json(
                row["receipt"].as_str().unwrap().as_bytes(),
                &request,
                &expected,
            )?;
            assert_eq!(
                PublicationReceipt::from_json(
                    &r.to_json(&request, &expected)?,
                    &request,
                    &expected
                )?,
                r
            );
            Ok(())
        };
        assert_eq!(
            check().is_ok(),
            row["accepted"].as_bool().unwrap(),
            "{}",
            row["name"]
        );
    }
}
#[test]
fn creation_recreation_deletion_and_alternatives_preserve_exact_state() {
    for old in ["0", "8"] {
        let mut i = request().intent;
        i.updates[0].expected_generation = old.into();
        i.updates[0].expected_old_targets.clear();
        i.updates[0]
            .new_targets
            .push(format!("k1:b3:{}", "c".repeat(64)));
        let q = i.into_request().unwrap();
        let mut r = receipt();
        r.request_digest = q.request_digest.clone();
        r.refs[0].old_generation = old.into();
        r.refs[0].new_generation = (old.parse::<u64>().unwrap() + 1).to_string();
        r.refs[0].old_targets.clear();
        r.refs[0].new_targets = q.intent.updates[0].new_targets.clone();
        assert!(r.validate(&q, &expected()).is_ok());
        assert!(r.refs[1].new_targets.is_empty()); // deletion still carries advancing generation
        r.refs[0].new_targets.pop();
        assert!(r.validate(&q, &expected()).is_err()); // cannot hide an alternative
    }
}
#[test]
fn counters_never_wrap_and_allow_last_representable_generation() {
    for namespace in [true, false] {
        let mut q = request();
        let mut r = receipt();
        if namespace {
            q.intent.expected_namespace_generation = (u64::MAX - 1).to_string();
            r.repository_generation = u64::MAX.to_string();
        } else {
            q.intent.updates[0].expected_generation = (u64::MAX - 1).to_string();
            r.refs[0].old_generation = (u64::MAX - 1).to_string();
            r.refs[0].new_generation = u64::MAX.to_string();
        }
        q = q.intent.into_request().unwrap();
        r.request_digest = q.request_digest.clone();
        assert!(r.validate(&q, &expected()).is_ok());
        if namespace {
            q.intent.expected_namespace_generation = u64::MAX.to_string();
            r.repository_generation = "0".into();
        } else {
            q.intent.updates[0].expected_generation = u64::MAX.to_string();
            r.refs[0].old_generation = u64::MAX.to_string();
            r.refs[0].new_generation = "0".into();
        }
        q = q.intent.into_request().unwrap();
        r.request_digest = q.request_digest.clone();
        assert!(r.validate(&q, &expected()).is_err());
    }
}
#[test]
fn original_request_is_revalidated_and_wire_is_bounded() {
    let r = receipt();
    let mut q = request();
    q.intent.reason = "tampered".into();
    assert!(r.validate(&q, &expected()).is_err());
    assert!(
        PublicationReceipt::from_json(&vec![b' '; 1024 * 1024 + 1], &request(), &expected())
            .is_err()
    );
}
#[test]
fn matching_receipt_exceeds_output_bound_only() {
    let mut intent = request().intent;
    intent.updates = (0..1000)
        .map(|i| RefUpdate {
            name: format!("branches/{i:04}"),
            expected_generation: "0".into(),
            expected_old_targets: vec![],
            new_targets: vec![format!("k1:b3:{}", "a".repeat(64))],
        })
        .collect();
    let padding = 1024 * 1024 - 128 - serde_json::to_vec(&intent).unwrap().len();
    intent
        .updates
        .last_mut()
        .unwrap()
        .name
        .push_str(&"x".repeat(padding));
    let request = intent.into_request().unwrap();
    request.validate().unwrap();
    assert!(request.to_json().unwrap().len() <= 1024 * 1024);
    let mut oversized = receipt();
    oversized.request_digest = request.request_digest.clone();
    oversized.refs = request
        .intent
        .updates
        .iter()
        .map(|u| PublishedRef {
            name: u.name.clone(),
            old_generation: "0".into(),
            new_generation: "1".into(),
            old_targets: u.expected_old_targets.clone(),
            new_targets: u.new_targets.clone(),
        })
        .collect();
    assert!(serde_json::to_vec(&oversized).unwrap().len() > 1024 * 1024);
    assert_eq!(
        oversized.to_json(&request, &expected()).unwrap_err(),
        "publication receipt size limit"
    );
}
