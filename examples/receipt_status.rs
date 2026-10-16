use kai_vcs::{
    interface::Durability, publication_receipt::PublicationDurability, receipt_status::*,
    update_refs::UpdateRefsIntent,
};
fn main() -> Result<(), String> {
    let originals: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/update-refs.json"))
            .map_err(|e| e.to_string())?;
    let original = serde_json::from_value::<UpdateRefsIntent>(originals[0]["intent"].clone())
        .map_err(|e| e.to_string())?
        .into_request()?;
    let query = ReceiptStatusRequest::new(&original, "4".repeat(32))?;
    let durability = PublicationDurability {
        class: Durability::SingleNodeDurable,
        replication_policy: None,
    };
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../tests/fixtures/receipt-status.json"))
            .map_err(|e| e.to_string())?;
    for name in [
        "lost-committed-response",
        "committed-needs-recovery",
        "unknown-expired",
        "digest_mismatch-None",
    ] {
        let row = rows
            .iter()
            .find(|r| r["name"] == name)
            .ok_or("missing fixture")?;
        let reply = ReceiptStatusReply::from_json(
            row["reply"].as_str().ok_or("missing reply")?.as_bytes(),
            &query,
            &original,
            &durability,
        )?;
        println!(
            "{name}: {:?}",
            reply.advice(&query, &original, &durability)?
        );
        if let Some(receipt) = reply.receipt {
            println!(
                "  Recovered {} ref transitions at repository generation {}",
                receipt.refs.len(),
                receipt.repository_generation
            );
        }
    }
    println!("Synthetic replies only; no transaction is executed or replayed.");
    Ok(())
}
