use kai_vcs::update_refs::*;
use serde_json::{Value, json};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/update-refs.json")).unwrap()
}
fn intent() -> UpdateRefsIntent {
    serde_json::from_value(fixture()[0]["intent"].clone()).unwrap()
}
#[test]
fn independent_preimage_and_json_roundtrip() {
    let intent = intent();
    let bytes = intent.canonical_bytes().unwrap();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, fixture()[0]["canonical_hex"].as_str().unwrap());
    let r = intent.into_request().unwrap();
    assert_eq!(
        r.request_digest,
        fixture()[0]["request_digest"].as_str().unwrap()
    );
    let v = serde_json::to_value(&r).unwrap();
    // Value's sorted keys and pretty whitespace differ from the typed serializer.
    let parsed = UpdateRefsRequest::from_json(&serde_json::to_vec_pretty(&v).unwrap()).unwrap();
    assert_eq!(parsed, r);
    assert_eq!(
        UpdateRefsRequest::from_json(&r.to_json().unwrap()).unwrap(),
        r
    );
    let query = r.status_query("4".repeat(32)).unwrap();
    assert_eq!(query.transaction_id, r.intent.transaction_id);
    assert_eq!(query.request_digest, r.request_digest);
    assert_eq!(query.repository_id, r.intent.repository_id);
}
#[test]
fn every_intent_field_is_bound_and_stale_digest_is_rejected() {
    let r = intent().into_request().unwrap();
    for (pointer, replacement) in [
        ("/transaction_id", json!("5".repeat(32))),
        ("/repository_id", json!("6".repeat(32))),
        ("/expected_namespace_generation", json!("10")),
        ("/operation_id", json!("7".repeat(32))),
        ("/reason", json!("different intent")),
        ("/updates/0/name", json!("branches/next")),
        ("/updates/0/expected_generation", json!("5")),
        (
            "/updates/0/expected_old_targets/0",
            json!(format!("k1:b3:{}", "f".repeat(64))),
        ),
        (
            "/updates/0/new_targets/0",
            json!(format!("k1:b3:{}", "f".repeat(64))),
        ),
        (
            "/required_closure_manifest",
            json!(format!("k1:b3:{}", "f".repeat(64))),
        ),
        ("/policy_input_ids", json!([])),
        ("/client_capabilities", json!([])),
    ] {
        let mut v = serde_json::to_value(&r.intent).unwrap();
        *v.pointer_mut(pointer).unwrap() = replacement;
        let changed: UpdateRefsIntent = serde_json::from_value(v).unwrap();
        assert_ne!(
            changed.request_digest().unwrap(),
            r.request_digest,
            "{pointer}"
        );
        let stale = UpdateRefsRequest {
            intent: changed,
            request_digest: r.request_digest.clone(),
        };
        assert!(stale.validate().is_err(), "{pointer}");
        assert!(stale.status_query("8".repeat(32)).is_err());
    }
}
#[test]
fn malformed_and_ambiguous_intents_fail_before_hashing() {
    for (pointer, replacement) in [
        ("/schema_version", json!(2)),
        ("/transaction_id", json!("A".repeat(32))),
        ("/expected_namespace_generation", json!("01")),
        (
            "/expected_namespace_generation",
            json!("18446744073709551616"),
        ),
        ("/operation_id", json!("bad")),
        ("/reason", json!("")),
        ("/updates", json!([])),
        ("/updates/0/name", json!("branches/../main")),
        ("/updates/0/name", json!("branches/cafe\u{301}")),
        ("/updates/0/expected_generation", json!("0")),
        ("/updates/0/new_targets", json!(["bad"])),
        ("/required_closure_manifest", json!("bad")),
        ("/policy_input_ids", json!(["bad"])),
        ("/client_capabilities", json!(["z", "a"])),
        ("/client_capabilities", json!(["a", "a"])),
    ] {
        let mut v = serde_json::to_value(intent()).unwrap();
        *v.pointer_mut(pointer).unwrap() = replacement;
        let changed: UpdateRefsIntent = serde_json::from_value(v).unwrap();
        assert!(changed.canonical_bytes().is_err(), "{pointer}");
    }
    let mut i = intent();
    i.updates.push(i.updates[0].clone());
    assert!(i.validate().is_err());
    let mut i = intent();
    i.updates[1].new_targets = i.updates[0]
        .expected_old_targets
        .iter()
        .chain(i.updates[0].new_targets.iter())
        .cloned()
        .collect();
    assert!(i.validate().is_err());
}
#[test]
fn creation_tombstone_recreation_and_branch_alternatives() {
    let mut i = intent();
    i.updates.truncate(1);
    i.updates[0].expected_generation = "0".into();
    i.updates[0].expected_old_targets.clear();
    assert!(i.validate().is_ok());
    let creation = i.request_digest().unwrap();
    i.updates[0].expected_generation = "8".into();
    assert_ne!(creation, i.request_digest().unwrap());
    i.updates[0]
        .new_targets
        .push(format!("k1:b3:{}", "c".repeat(64)));
    assert!(i.validate().is_ok());
    i.updates[0].new_targets.clear();
    assert!(i.validate().is_err());
}
#[test]
fn wire_bound_unknown_duplicate_missing_and_trailing_fields() {
    let r = intent().into_request().unwrap();
    let mut v = serde_json::to_value(&r).unwrap();
    v["intent"]["extra"] = json!(true);
    assert!(UpdateRefsRequest::from_json(&serde_json::to_vec(&v).unwrap()).is_err());
    let wire = String::from_utf8(r.to_json().unwrap()).unwrap();
    for bad in [
        wire.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
            1,
        ),
        format!("{wire} false"),
        wire.replacen("\"new_targets\":", "\"wrong\":", 1),
    ] {
        assert!(UpdateRefsRequest::from_json(bad.as_bytes()).is_err());
    }
    assert!(UpdateRefsRequest::from_json(&vec![b' '; 1024 * 1024 + 1]).is_err());
    let mut i = intent();
    i.reason = "a".repeat(4097);
    assert!(i.into_request().is_err());
}
