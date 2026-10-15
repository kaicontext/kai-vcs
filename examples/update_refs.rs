use kai_vcs::update_refs::{UpdateRefsIntent, UpdateRefsRequest};
fn main() -> Result<(), String> {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/update-refs.json"))
            .map_err(|e| e.to_string())?;
    let intent: UpdateRefsIntent =
        serde_json::from_value(rows[0]["intent"].clone()).map_err(|e| e.to_string())?;
    let request = intent.into_request()?;
    println!("Publication intent digest: {}", request.request_digest);
    println!(
        "{}",
        String::from_utf8(request.to_json()?).map_err(|e| e.to_string())?
    );
    let query = request.status_query("4".repeat(32))?;
    println!(
        "After a lost response, query transaction {} with digest {}",
        query.transaction_id, query.request_digest
    );
    let mut altered = request.clone();
    altered.intent.reason = "changed after signing intent".into();
    assert!(UpdateRefsRequest::from_json(&serde_json::to_vec(&altered).unwrap()).is_err());
    println!("Changed intent with stale digest: rejected. No refs were changed.");
    Ok(())
}
