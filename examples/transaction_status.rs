use kai_vcs::{
    interface::{MutationOutcome, MutationResult},
    transaction_status::*,
};
fn main() -> Result<(), String> {
    let query = TransactionStatusRequest {
        schema_version: 1,
        request_id: "01".repeat(16),
        repository_id: "02".repeat(16),
        transaction_id: "03".repeat(16),
        request_digest: format!("b3:{}", "11".repeat(32)),
    };
    let committed = TransactionStatusReply {
        schema_version: 1,
        request_id: query.request_id.clone(),
        repository_id: query.repository_id.clone(),
        transaction_id: query.transaction_id.clone(),
        request_digest: query.request_digest.clone(),
        status: LookupStatus::Known,
        unknown_reason: None,
        result: Some(MutationResult {
            schema_version: 1,
            request_id: query.transaction_id.clone(),
            operation_id: Some("04".repeat(16)),
            outcome: MutationOutcome::Committed,
            exit_code: 0,
            generation: Some("7".into()),
            refs: vec![],
            recovery: vec![],
            external_receipts: vec![],
            interrupted: true,
        }),
    };
    let checked = TransactionStatusReply::from_json(&committed.to_json(&query)?, &query)?;
    println!(
        "Lost-response example: {:?}; the recorded commit is not repeated.",
        checked.advice(&query)?
    );
    let expired = TransactionStatusReply {
        status: LookupStatus::Unknown,
        unknown_reason: Some(UnknownReason::Expired),
        result: None,
        ..committed
    };
    println!(
        "Expired-result example: {:?}; unknown does not mean never executed.",
        expired.advice(&query)?
    );
    println!("Constructed local examples only; no network, receipt storage or mutation execution.");
    Ok(())
}
