use kai_vcs::list_refs::*;
fn main() -> Result<(), String> {
    let request = ListRefsRequest {
        schema_version: 1,
        request_id: "01".repeat(16),
        repository_id: "02".repeat(16),
        namespace: RefNamespace::Branches,
        limit: 2,
        after: None,
        generation: None,
    };
    let page = ListRefsReply {
        schema_version: 1,
        request_id: request.request_id.clone(),
        repository_id: request.repository_id.clone(),
        namespace: request.namespace,
        generation: "7".into(),
        after: None,
        has_more: true,
        refs: vec![
            ListedRef {
                name: "branches/deleted".into(),
                generation: "3".into(),
                targets: vec![],
            },
            ListedRef {
                name: "branches/draft".into(),
                generation: "4".into(),
                targets: vec![
                    format!("k1:b3:{}", "11".repeat(32)),
                    format!("k1:b3:{}", "22".repeat(32)),
                ],
            },
        ],
    };
    let checked = ListRefsReply::from_json(&page.to_json(&request)?, &request)?;
    let next = checked
        .continuation(&request, "03".repeat(16))?
        .ok_or("expected another page")?;
    println!("Verified two refs: one tombstone and one branch with two alternatives.");
    println!(
        "Next page: after {}, pinned to generation {}.",
        next.after.as_deref().unwrap(),
        next.generation.as_deref().unwrap()
    );
    let mut changed = ListRefsReply {
        request_id: next.request_id.clone(),
        after: next.after.clone(),
        refs: vec![],
        has_more: false,
        ..checked
    };
    changed.generation = "8".into();
    println!(
        "Changed-generation reply rejected: {}.",
        changed.validate(&next).is_err()
    );
    println!("Local contract demo only; no repository or network access.");
    Ok(())
}
