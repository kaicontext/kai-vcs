use kai_vcs::list_refs::*;
use serde_json::Value;
fn request() -> ListRefsRequest {
    ListRefsRequest {
        schema_version: 1,
        request_id: "01".repeat(16),
        repository_id: "02".repeat(16),
        namespace: RefNamespace::Branches,
        limit: 2,
        after: None,
        generation: None,
    }
}
fn page(r: &ListRefsRequest, name: &str, more: bool) -> ListRefsReply {
    ListRefsReply {
        schema_version: 1,
        request_id: r.request_id.clone(),
        repository_id: r.repository_id.clone(),
        namespace: r.namespace,
        generation: "7".into(),
        after: r.after.clone(),
        refs: vec![ListedRef {
            name: name.into(),
            generation: "1".into(),
            targets: vec![],
        }],
        has_more: more,
    }
}
#[test]
fn shared_corpus_matches_rust_and_roundtrips() {
    let rows: Vec<Value> = serde_json::from_str(include_str!("fixtures/list-refs.json")).unwrap();
    for row in rows {
        let result = || -> Result<(), String> {
            let r = ListRefsRequest::from_json(row["request"].as_str().unwrap().as_bytes())?;
            let p = ListRefsReply::from_json(row["reply"].as_str().unwrap().as_bytes(), &r)?;
            assert_eq!(ListRefsRequest::from_json(&r.to_json()?)?, r);
            assert_eq!(ListRefsReply::from_json(&p.to_json(&r)?, &r)?, p);
            Ok(())
        };
        assert_eq!(
            result().is_ok(),
            row["accepted"].as_bool().unwrap(),
            "{}",
            row["name"]
        );
    }
}
#[test]
fn continuation_pins_snapshot_and_advances_without_losing_tombstones() {
    let r = request();
    let p = page(&r, "branches/a", true);
    assert!(p.refs[0].targets.is_empty());
    let next = p.continuation(&r, "03".repeat(16)).unwrap().unwrap();
    assert_eq!(next.after.as_deref(), Some("branches/a"));
    assert_eq!(next.generation.as_deref(), Some("7"));
    assert_eq!(next.namespace, r.namespace);
    assert_eq!(next.repository_id, r.repository_id);
    assert_eq!(next.limit, r.limit);
    let mut end = page(&next, "branches/b", false);
    assert!(end.continuation(&next, "04".repeat(16)).unwrap().is_none());
    end.generation = "8".into();
    assert!(end.to_json(&next).is_err());
    assert!(end.continuation(&next, "04".repeat(16)).is_err());
    assert!(p.continuation(&r, r.request_id.clone()).is_err());
    assert!(p.continuation(&r, "invalid".into()).is_err());
    let repeated = page(&next, "branches/a", true);
    assert!(repeated.validate(&next).is_err());
    let before = page(&next, "branches/0", true);
    assert!(before.validate(&next).is_err());
    let mut empty = page(&next, "branches/b", false);
    empty.refs.clear();
    assert!(empty.validate(&next).is_ok());
    empty.has_more = true;
    assert!(empty.validate(&next).is_err());
}
#[test]
fn preserves_all_branch_alternatives_and_max_generations() {
    let mut r = request();
    r.generation = Some(u64::MAX.to_string());
    let mut p = page(&r, "branches/draft", false);
    p.generation = u64::MAX.to_string();
    p.refs[0].generation = u64::MAX.to_string();
    p.refs[0].targets = (0..100).map(|n| format!("k1:b3:{n:064x}")).collect();
    let wire = p.to_json(&r).unwrap();
    let decoded = ListRefsReply::from_json(&wire, &r).unwrap();
    assert_eq!(decoded.refs[0].targets, p.refs[0].targets);
    assert_eq!(decoded.generation, u64::MAX.to_string());
    p.refs[0].targets.reverse();
    assert!(p.to_json(&r).is_err());
}
#[test]
fn refuses_oversized_and_nested_duplicate_records_without_truncating_entries() {
    let r = request();
    let p = page(&r, "branches/a", false);
    let mut input = r.to_json().unwrap();
    input.resize(1024 * 1024 + 1, b' ');
    assert!(ListRefsRequest::from_json(&input).is_err());
    let mut input = p.to_json(&r).unwrap();
    input.resize(1024 * 1024 + 1, b' ');
    assert!(ListRefsReply::from_json(&input, &r).is_err());
    let raw = String::from_utf8(p.to_json(&r).unwrap()).unwrap();
    let bad = raw.replace("\"targets\":[]", "\"targets\":[],\"targets\":[]");
    assert!(ListRefsReply::from_json(bad.as_bytes(), &r).is_err());
    let mut large = p;
    large.refs[0].name = format!("branches/{}", "x".repeat(1024 * 1024));
    assert!(large.to_json(&r).is_err());
}
