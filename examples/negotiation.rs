use kai_vcs::{interface::*, transfer_frame::FrameLimits};
fn main() -> Result<(), String> {
    let offer = Capabilities {
        schema_version: 1,
        protocol_versions: vec![1],
        object_formats: vec!["native-v1-blake3".into()],
        supported_features: vec![],
        required_features: vec![],
        max_object_bytes: 1024 * 1024,
        max_frame_bytes: 1024 * 1024 + 95,
        compression: vec!["none".into()],
        signature_formats: vec![],
        ref_transactions: false,
        partial: false,
        shallow: false,
        durability: Durability::None,
        replication_policy: None,
    };
    let request = NegotiationEnvelope {
        schema_version: 1,
        request_id: "01".repeat(16),
        repository_id: "02".repeat(16),
        policy: NegotiationRequest {
            selection: TransferSelection::Complete,
            require_ref_transactions: false,
            required_signatures: vec![],
            minimum_durability: Durability::None,
            replication_policy: None,
        },
    };
    let wire = request.to_json()?;
    let received = NegotiationEnvelope::from_json(&wire)?;
    let reply = received
        .reply(&offer, &offer)?
        .to_json(&received, &offer, &offer)?;
    let checked = NegotiationReply::verify_json(&reply, &request, &offer, &offer)?;
    let limits = FrameLimits::negotiated(&checked.negotiated, 100, 10 * 1024 * 1024)?;
    println!(
        "Verified complete-transfer negotiation for repository {}.",
        checked.repository_id
    );
    println!(
        "Object limit: {} bytes; frame limit: {} bytes.",
        limits.max_object_bytes, limits.max_frame_bytes
    );
    println!("Local contract demo only; no authenticated session or repository mutation.");
    Ok(())
}
