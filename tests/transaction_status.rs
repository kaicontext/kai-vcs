use kai_vcs::transaction_status::*;
use serde_json::Value;
fn corpus() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/transaction-status.json")).unwrap()
}
fn sample() -> (TransactionStatusRequest, TransactionStatusReply) {
    let rows = corpus();
    let r = TransactionStatusRequest::from_json(rows[0]["request"].as_str().unwrap().as_bytes())
        .unwrap();
    let p = TransactionStatusReply::from_json(rows[0]["reply"].as_str().unwrap().as_bytes(), &r)
        .unwrap();
    (r, p)
}
#[test]
fn shared_corpus_matches_rust_and_roundtrips() {
    for row in corpus() {
        let request =
            TransactionStatusRequest::from_json(row["request"].as_str().unwrap().as_bytes());
        assert_eq!(
            request.is_ok(),
            row["request_accepted"].as_bool().unwrap(),
            "request validation: {}",
            row["name"]
        );
        let check = || -> Result<(), String> {
            let r = request?;
            let p =
                TransactionStatusReply::from_json(row["reply"].as_str().unwrap().as_bytes(), &r)?;
            assert_eq!(TransactionStatusRequest::from_json(&r.to_json()?)?, r);
            assert_eq!(TransactionStatusReply::from_json(&p.to_json(&r)?, &r)?, p);
            p.advice(&r)?;
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
fn recorded_outcomes_preserve_commit_truth_and_never_authorize_replay() {
    for (name, expected) in [
        ("committed-after-lost-response", StatusAdvice::Complete),
        (
            "committed_materialization_pending",
            StatusAdvice::RecoverCommitted,
        ),
        ("prepared", StatusAdvice::QueryAgain),
        ("indeterminate", StatusAdvice::Reconcile),
        ("conflicted", StatusAdvice::ResolveConflict),
        ("not_started", StatusAdvice::NewIntentRequired),
        ("rejected", StatusAdvice::NewIntentRequired),
        ("unknown-missing", StatusAdvice::Reconcile),
        ("unknown-expired", StatusAdvice::Reconcile),
        ("digest-mismatch", StatusAdvice::ResolveDigestMismatch),
    ] {
        let rows = corpus();
        let row = rows.iter().find(|r| r["name"] == name).unwrap();
        let r = TransactionStatusRequest::from_json(row["request"].as_str().unwrap().as_bytes())
            .unwrap();
        let p = TransactionStatusReply::from_json(row["reply"].as_str().unwrap().as_bytes(), &r)
            .unwrap();
        assert_eq!(p.advice(&r).unwrap(), expected, "{name}");
    }
    let (r, mut p) = sample();
    p.result.as_mut().unwrap().interrupted = true;
    assert_eq!(p.advice(&r).unwrap(), StatusAdvice::Complete);
    assert_eq!(p.result.unwrap().exit_code, 0);
}
#[test]
fn replayed_reply_wrong_repository_or_digest_cannot_produce_guidance() {
    let (r, p) = sample();
    let wire = p.to_json(&r).unwrap();
    for field in [
        "request_id",
        "repository_id",
        "transaction_id",
        "request_digest",
    ] {
        let mut changed = serde_json::to_value(&r).unwrap();
        changed[field] = Value::String(if field == "request_digest" {
            format!("b3:{}", "ff".repeat(32))
        } else {
            "ff".repeat(16)
        });
        let changed: TransactionStatusRequest = serde_json::from_value(changed).unwrap();
        assert!(TransactionStatusReply::from_json(&wire, &changed).is_err());
        assert!(p.advice(&changed).is_err());
        assert!(p.to_json(&changed).is_err());
    }
    let mut p = p;
    p.result.as_mut().unwrap().request_id = r.request_id.clone();
    assert!(p.advice(&r).is_err());
}
#[test]
fn bounded_json_and_nested_duplicate_fields_fail_closed() {
    let (r, p) = sample();
    let mut input = r.to_json().unwrap();
    input.resize(1024 * 1024 + 1, b' ');
    assert!(TransactionStatusRequest::from_json(&input).is_err());
    let mut input = p.to_json(&r).unwrap();
    input.resize(1024 * 1024 + 1, b' ');
    assert!(TransactionStatusReply::from_json(&input, &r).is_err());
    let raw = String::from_utf8(p.to_json(&r).unwrap()).unwrap();
    let duplicate = raw.replace(
        "\"outcome\":\"committed\"",
        "\"outcome\":\"committed\",\"outcome\":\"committed\"",
    );
    assert!(TransactionStatusReply::from_json(duplicate.as_bytes(), &r).is_err());
    let mut huge = p;
    huge.result.as_mut().unwrap().recovery = vec!["x".repeat(1024 * 1024)];
    assert!(huge.to_json(&r).is_err());
    let mut bad = r;
    bad.request_digest = "wrong".into();
    assert!(bad.to_json().is_err());
}
