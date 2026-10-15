use kai_vcs::interface::*;
fn offer() -> Capabilities {
    Capabilities {
        schema_version: 1,
        protocol_versions: vec![1, 2],
        object_formats: vec!["native-v1-blake3".into()],
        supported_features: vec!["atomic-refs".into()],
        required_features: vec![],
        max_object_bytes: 1024,
        max_frame_bytes: 2048,
        compression: vec!["none".into()],
        signature_formats: vec!["sshsig".into()],
        ref_transactions: true,
        partial: true,
        shallow: true,
        durability: Durability::None,
        replication_policy: None,
    }
}
fn request() -> NegotiationRequest {
    NegotiationRequest {
        selection: TransferSelection::Complete,
        require_ref_transactions: true,
        required_signatures: vec!["sshsig".into()],
        minimum_durability: Durability::None,
        replication_policy: None,
    }
}
fn committed() -> MutationResult {
    MutationResult {
        schema_version: 1,
        request_id: "01".repeat(16),
        operation_id: Some("02".repeat(16)),
        outcome: MutationOutcome::Committed,
        exit_code: 0,
        generation: Some(u64::MAX.to_string()),
        refs: vec![],
        recovery: vec![],
        external_receipts: vec![],
        interrupted: false,
    }
}
#[test]
fn negotiation_selects_mutual_limits_without_completeness_downgrade() {
    let a = offer();
    let mut b = offer();
    b.protocol_versions = vec![1];
    b.max_object_bytes = 512;
    b.max_frame_bytes = 1024;
    let n = negotiate(&a, &b, &request()).unwrap();
    assert_eq!(n.protocol_version, 1);
    assert_eq!(n.max_object_bytes, 512);
    assert_eq!(n.max_frame_bytes, 1024);
    assert_eq!(n.selection, TransferSelection::Complete);
}
#[test]
fn required_features_and_transfer_semantics_fail_closed() {
    let a = offer();
    let mut b = offer();
    b.supported_features.push("unknown".into());
    b.required_features.push("unknown".into());
    assert!(negotiate(&a, &b, &request()).is_err());
    b = offer();
    b.partial = false;
    let mut r = request();
    r.selection = TransferSelection::Partial;
    assert!(negotiate(&a, &b, &r).is_err());
    r.selection = TransferSelection::Complete;
    assert!(negotiate(&a, &b, &r).is_ok());
    b.ref_transactions = false;
    assert!(negotiate(&a, &b, &r).is_err());
}
#[test]
fn algorithms_signatures_and_durability_cannot_silently_downgrade() {
    let a = offer();
    let mut b = offer();
    b.signature_formats.clear();
    assert!(negotiate(&a, &b, &request()).is_err());
    b = offer();
    b.object_formats = vec!["other".into()];
    assert!(negotiate(&a, &b, &request()).is_err());
    b = offer();
    let mut r = request();
    r.minimum_durability = Durability::SingleNodeDurable;
    assert!(negotiate(&a, &b, &r).is_err());
    b.durability = Durability::ReplicatedDurable;
    assert!(b.validate().is_err());
    let p = format!("k1:b3:{}", "11".repeat(32));
    b.replication_policy = Some(p.clone());
    r.minimum_durability = Durability::ReplicatedDurable;
    r.replication_policy = Some(p);
    assert!(negotiate(&a, &b, &r).is_ok());
    r.replication_policy = Some(format!("k1:b3:{}", "22".repeat(32)));
    assert!(negotiate(&a, &b, &r).is_err());
}
#[test]
fn interrupted_commits_keep_truthful_success_or_pending_status() {
    let mut v = committed();
    v.interrupted = true;
    let b = v.to_json().unwrap();
    assert_eq!(MutationResult::from_json(&b).unwrap(), v);
    assert_eq!(v.exit_code, 0);
    v.external_receipts.push(ExternalReceipt {
        system: "mirror".into(),
        receipt_id: "r1".into(),
        state: ReceiptState::Pending,
    });
    assert!(v.validate().is_err());
    v.outcome = MutationOutcome::CommittedMaterializationPending;
    v.exit_code = 9;
    v.recovery
        .push("Resume mirror delivery for this operation.".into());
    assert!(v.to_json().is_ok());
}
#[test]
fn prepared_and_indeterminate_never_report_success_or_resulting_refs() {
    let mut v = committed();
    v.outcome = MutationOutcome::Prepared;
    assert!(v.validate().is_err());
    v.exit_code = 1;
    v.generation = None;
    v.recovery
        .push("Query the operation status before retrying.".into());
    assert!(v.validate().is_ok());
    v.outcome = MutationOutcome::Indeterminate;
    v.exit_code = 10;
    assert!(v.validate().is_ok());
    v.generation = Some("1".into());
    assert!(v.validate().is_err());
}
#[test]
fn json_roundtrip_preserves_large_generations_and_ref_tombstones() {
    let mut v = committed();
    v.refs.push(ResultRef {
        name: "branches/main".into(),
        generation: u64::MAX.to_string(),
        target: None,
    });
    let b = v.to_json().unwrap();
    assert!(
        std::str::from_utf8(&b)
            .unwrap()
            .contains("\"18446744073709551615\"")
    );
    assert_eq!(MutationResult::from_json(&b).unwrap(), v);
    v.refs[0].generation = "01".into();
    assert!(v.validate().is_err());
}
#[test]
fn json_rejects_unknown_duplicate_trailing_and_oversized_inputs() {
    let v = committed().to_json().unwrap();
    let s = String::from_utf8(v).unwrap();
    for changed in [
        s.replacen("{", "{\"schema_version\":1,", 1),
        s.replacen("{", "{\"secret_extra\":1,", 1),
        format!("{s} {{}}"),
    ] {
        assert!(MutationResult::from_json(changed.as_bytes()).is_err());
    }
    assert!(MutationResult::from_json(&vec![b' '; 1024 * 1024 + 1]).is_err());
    let mut c = offer();
    c.supported_features.push("atomic-refs".into());
    assert!(c.validate().is_err());
}
#[test]
fn published_interface_vectors_match_validation() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/interface.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let input = row["json"].as_str().unwrap().as_bytes();
        let ok = if row["kind"] == "capabilities" {
            Capabilities::from_json(input).is_ok()
        } else {
            MutationResult::from_json(input).is_ok()
        };
        assert_eq!(ok, row["accepted"].as_bool().unwrap(), "{}", row["name"]);
    }
}
