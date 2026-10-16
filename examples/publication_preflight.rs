use kai_vcs::{
    RefState, View, ViewRef, interface::Durability, publication_preflight::PublicationPlan,
    publication_receipt::PublicationDurability, update_refs::UpdateRefsIntent,
};
fn main() -> Result<(), String> {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/update-refs.json"))
            .map_err(|e| e.to_string())?;
    let intent: UpdateRefsIntent =
        serde_json::from_value(rows[0]["intent"].clone()).map_err(|e| e.to_string())?;
    let request = intent.into_request()?;
    let current = View {
        namespace_generation: 9,
        refs: vec![
            ViewRef {
                name: "branches/main".into(),
                generation: 4,
                state: RefState::Target(format!("k1:b3:{}", "a".repeat(64)).parse()?),
            },
            ViewRef {
                name: "tags/old".into(),
                generation: 7,
                state: RefState::Target(format!("k1:b3:{}", "c".repeat(64)).parse()?),
            },
        ],
        ..View::default()
    };
    let plan = PublicationPlan::prepare(
        &request.intent.repository_id,
        &current,
        &request,
        format!("k1:b3:{}", "f".repeat(64)),
        PublicationDurability {
            class: Durability::SingleNodeDurable,
            replication_policy: None,
        },
    )?;
    println!(
        "Preview: namespace generation {} -> {}",
        current.namespace_generation,
        plan.candidate_view().namespace_generation
    );
    for r in &plan.candidate_view().refs {
        let target = match &r.state {
            RefState::Deleted => "deleted (tombstone retained)".into(),
            RefState::Target(id) => id.to_string(),
            RefState::DraftConflict(ids) => format!("{} explicit alternatives", ids.len()),
        };
        println!("  {}: generation {}, {}", r.name, r.generation, target);
    }
    println!(
        "Complete committed status would use {} bytes",
        plan.committed_status_bytes()
    );
    let mut moved = current.clone();
    moved.namespace_generation += 1;
    println!(
        "Concurrent change: {}",
        plan.recheck_snapshot(&request.intent.repository_id, &moved)
            .unwrap_err()
    );
    println!(
        "Original view remains at generation {}. No writes or commit occurred.",
        current.namespace_generation
    );
    Ok(())
}
