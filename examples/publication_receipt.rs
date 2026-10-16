use kai_vcs::{publication_receipt::*, update_refs::UpdateRefsIntent};
fn main() -> Result<(), String> {
    let requests: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/update-refs.json"))
            .map_err(|e| e.to_string())?;
    let intent: UpdateRefsIntent =
        serde_json::from_value(requests[0]["intent"].clone()).map_err(|e| e.to_string())?;
    let request = intent.into_request()?;
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/publication-receipt.json"))
            .map_err(|e| e.to_string())?;
    let expected = PublicationDurability {
        class: kai_vcs::interface::Durability::SingleNodeDurable,
        replication_policy: None,
    };
    let receipt = PublicationReceipt::from_json(
        rows[0]["receipt"]
            .as_str()
            .ok_or("missing fixture")?
            .as_bytes(),
        &request,
        &expected,
    )?;
    println!(
        "Synthetic receipt verified for transaction {}",
        receipt.transaction_id
    );
    println!("Repository generation: {}", receipt.repository_generation);
    for r in &receipt.refs {
        println!(
            "{}: generation {} -> {}, {} new targets",
            r.name,
            r.old_generation,
            r.new_generation,
            r.new_targets.len()
        );
    }
    let mut partial = receipt;
    partial.refs.pop();
    println!(
        "Incomplete batch rejected: {}",
        partial.validate(&request, &expected).unwrap_err()
    );
    println!("Demo only: no refs changed and no durable transaction executed.");
    Ok(())
}
