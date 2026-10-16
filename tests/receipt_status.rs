use kai_vcs::{
    interface::Durability, publication_receipt::PublicationDurability, receipt_status::*,
    transaction_status::StatusAdvice, update_refs::*,
};
use serde_json::Value;
fn original() -> UpdateRefsRequest {
    let rows: Value = serde_json::from_str(include_str!("fixtures/update-refs.json")).unwrap();
    serde_json::from_value::<UpdateRefsIntent>(rows[0]["intent"].clone())
        .unwrap()
        .into_request()
        .unwrap()
}
fn rows() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/receipt-status.json")).unwrap()
}
fn durability() -> PublicationDurability {
    PublicationDurability {
        class: Durability::SingleNodeDurable,
        replication_policy: None,
    }
}
fn sample() -> (ReceiptStatusRequest, ReceiptStatusReply) {
    let row = rows().remove(0);
    let query =
        ReceiptStatusRequest::from_json(row["query"].as_str().unwrap().as_bytes(), &original())
            .unwrap();
    let reply = ReceiptStatusReply::from_json(
        row["reply"].as_str().unwrap().as_bytes(),
        &query,
        &original(),
        &durability(),
    )
    .unwrap();
    (query, reply)
}
#[test]
fn independent_corpus_checks_query_separately_and_roundtrips() {
    let original = original();
    let durability = durability();
    for row in rows() {
        let query =
            ReceiptStatusRequest::from_json(row["query"].as_str().unwrap().as_bytes(), &original);
        assert_eq!(
            query.is_ok(),
            row["query_accepted"].as_bool().unwrap(),
            "query: {}",
            row["name"]
        );
        let check = || -> Result<(), String> {
            let query = query?;
            assert_eq!(
                ReceiptStatusRequest::from_json(&query.to_json(&original)?, &original)?,
                query
            );
            let reply = ReceiptStatusReply::from_json(
                row["reply"].as_str().unwrap().as_bytes(),
                &query,
                &original,
                &durability,
            )?;
            assert_eq!(
                ReceiptStatusReply::from_json(
                    &reply.to_json(&query, &original, &durability)?,
                    &query,
                    &original,
                    &durability
                )?,
                reply
            );
            reply.advice(&query, &original, &durability)?;
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
fn guidance_requires_receipt_before_reporting_completion() {
    for (name, advice) in [
        ("lost-committed-response", StatusAdvice::Complete),
        ("interrupted-after-commit", StatusAdvice::Complete),
        ("committed-needs-recovery", StatusAdvice::RecoverCommitted),
        ("unknown-expired", StatusAdvice::Reconcile),
        ("unknown-missing", StatusAdvice::Reconcile),
        ("digest_mismatch-None", StatusAdvice::ResolveDigestMismatch),
        ("prepared", StatusAdvice::QueryAgain),
        ("indeterminate", StatusAdvice::Reconcile),
        ("rejected", StatusAdvice::NewIntentRequired),
    ] {
        let row = rows().into_iter().find(|r| r["name"] == name).unwrap();
        let q =
            ReceiptStatusRequest::from_json(row["query"].as_str().unwrap().as_bytes(), &original())
                .unwrap();
        let r = ReceiptStatusReply::from_json(
            row["reply"].as_str().unwrap().as_bytes(),
            &q,
            &original(),
            &durability(),
        )
        .unwrap();
        assert_eq!(r.advice(&q, &original(), &durability()).unwrap(), advice);
    }
    let (q, mut r) = sample();
    r.receipt = None;
    assert!(r.advice(&q, &original(), &durability()).is_err());
}
#[test]
fn alternatives_survive_recovery_without_singular_target_projection() {
    let mut original = original();
    original.intent.updates[0]
        .new_targets
        .push(format!("k1:b3:{}", "c".repeat(64)));
    original = original.intent.into_request().unwrap();
    let q = ReceiptStatusRequest::new(&original, "4".repeat(32)).unwrap();
    let (_, mut r) = sample();
    r.status.request_digest = original.request_digest.clone();
    let receipt = r.receipt.as_mut().unwrap();
    receipt.request_digest = original.request_digest.clone();
    receipt.refs[0].new_targets = original.intent.updates[0].new_targets.clone();
    let wire = r.to_json(&q, &original, &durability()).unwrap();
    let recovered = ReceiptStatusReply::from_json(&wire, &q, &original, &durability()).unwrap();
    assert_eq!(recovered.receipt.unwrap().refs[0].new_targets.len(), 2);
}
#[test]
fn entire_envelope_is_bounded_and_original_is_revalidated() {
    let (q, mut r) = sample();
    // This is valid result content, not a receipt/request mismatch masking the size check.
    r.status
        .result
        .as_mut()
        .unwrap()
        .recovery
        .push("x".repeat(1024 * 1024));
    assert_eq!(
        r.to_json(&q, &original(), &durability()).unwrap_err(),
        "receipt-status envelope size limit"
    );
    assert!(
        ReceiptStatusReply::from_json(&vec![b' '; 1024 * 1024 + 1], &q, &original(), &durability())
            .is_err()
    );
    let mut tampered = original();
    tampered.intent.reason = "changed".into();
    assert!(q.validate(&tampered).is_err());
    assert!(ReceiptStatusRequest::new(&tampered, "4".repeat(32)).is_err());
}
