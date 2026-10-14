use kai_vcs::interface::*;
use serde_json::Value;
fn corpus() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/negotiation.json")).unwrap()
}
#[test]
fn shared_corpus_matches_rust_negotiation_and_reply_binding() {
    for row in corpus() {
        let check = || -> Result<(), String> {
            let sender = Capabilities::from_json(&serde_json::to_vec(&row["sender"]).unwrap())?;
            let receiver = Capabilities::from_json(&serde_json::to_vec(&row["receiver"]).unwrap())?;
            let request =
                NegotiationEnvelope::from_json(row["request"].as_str().unwrap().as_bytes())?;
            let reply = request.reply(&sender, &receiver)?;
            if let Some(raw) = row.get("reply") {
                NegotiationReply::verify_json(
                    raw.as_str().unwrap().as_bytes(),
                    &request,
                    &sender,
                    &receiver,
                )?;
            }
            let expected = &row["expected"];
            if !expected.is_null() {
                assert_eq!(
                    serde_json::to_value(&reply).unwrap(),
                    *expected,
                    "{}",
                    row["name"]
                );
                let encoded = reply.to_json(&request, &sender, &receiver)?;
                assert_eq!(
                    NegotiationReply::verify_json(&encoded, &request, &sender, &receiver)?,
                    reply
                );
                assert_eq!(
                    NegotiationEnvelope::from_json(&request.to_json()?)?,
                    request
                );
            }
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
fn oversized_records_and_nested_unknown_duplicate_fields_are_rejected() {
    let rows = corpus();
    let first = &rows[0];
    let sender = Capabilities::from_json(&serde_json::to_vec(&first["sender"]).unwrap()).unwrap();
    let request =
        NegotiationEnvelope::from_json(first["request"].as_str().unwrap().as_bytes()).unwrap();
    let reply = request.reply(&sender, &sender).unwrap();
    let mut raw = request.to_json().unwrap();
    raw.resize(1024 * 1024 + 1, b' ');
    assert!(NegotiationEnvelope::from_json(&raw).is_err());
    let mut raw = reply.to_json(&request, &sender, &sender).unwrap();
    raw.resize(1024 * 1024 + 1, b' ');
    assert!(NegotiationReply::verify_json(&raw, &request, &sender, &sender).is_err());
    let raw = first["request"].as_str().unwrap();
    for replacement in [
        "\"selection\":\"complete\",\"selection\":\"complete\"",
        "\"unknown\":1,\"selection\":\"complete\"",
    ] {
        let bad = raw.replace("\"selection\":\"complete\"", replacement);
        assert!(NegotiationEnvelope::from_json(bad.as_bytes()).is_err());
    }
    let raw = String::from_utf8(reply.to_json(&request, &sender, &sender).unwrap()).unwrap();
    let bad = raw.replace(
        "\"protocol_version\":2",
        "\"protocol_version\":2,\"protocol_version\":2",
    );
    assert!(NegotiationReply::verify_json(bad.as_bytes(), &request, &sender, &sender).is_err());
    let mut forged = reply.clone();
    forged.negotiated.max_object_bytes += 1;
    assert!(forged.to_json(&request, &sender, &sender).is_err());
    let mut stale_offer = sender.clone();
    stale_offer.max_object_bytes -= 1;
    assert!(
        NegotiationReply::verify_json(raw.as_bytes(), &request, &sender, &stale_offer).is_err()
    );
    let mut bad = request.clone();
    bad.repository_id = "not-a-uuid".into();
    assert!(bad.to_json().is_err());
    assert!(bad.reply(&sender, &sender).is_err());
}
#[test]
fn reply_does_not_downgrade_complete_or_expand_limits() {
    let first = &corpus()[0];
    let sender = Capabilities::from_json(&serde_json::to_vec(&first["sender"]).unwrap()).unwrap();
    let mut receiver = sender.clone();
    receiver.max_object_bytes = 500;
    receiver.max_frame_bytes = 800;
    let request =
        NegotiationEnvelope::from_json(first["request"].as_str().unwrap().as_bytes()).unwrap();
    let reply = request.reply(&sender, &receiver).unwrap();
    assert_eq!(reply.negotiated.selection, TransferSelection::Complete);
    assert_eq!(reply.negotiated.max_object_bytes, 500);
    assert_eq!(reply.negotiated.max_frame_bytes, 800);
    let raw = reply.to_json(&request, &sender, &receiver).unwrap();
    assert!(NegotiationReply::verify_json(&raw, &request, &sender, &receiver).is_ok());
    for key in ["request_id", "repository_id"] {
        let mut v = serde_json::to_value(&reply).unwrap();
        v[key] = Value::String("ff".repeat(16));
        assert!(
            NegotiationReply::verify_json(
                &serde_json::to_vec(&v).unwrap(),
                &request,
                &sender,
                &receiver
            )
            .is_err()
        );
    }
}
