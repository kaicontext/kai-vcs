//! Pure publication preflight. A plan is not a commit or a durability acknowledgment.
use crate::{
    ObjectId, RefState, View, ViewRef,
    interface::{MutationOutcome, MutationResult},
    publication_receipt::{PublicationDurability, PublicationReceipt, PublishedRef},
    receipt_status::{ReceiptStatusReply, ReceiptStatusRequest},
    transaction_status::{LookupStatus, TransactionStatusReply},
    update_refs::UpdateRefsRequest,
};
use std::collections::BTreeMap;
fn targets(state: &RefState) -> Vec<String> {
    match state {
        RefState::Deleted => vec![],
        RefState::Target(id) => vec![id.to_string()],
        RefState::DraftConflict(ids) => ids.iter().map(ToString::to_string).collect(),
    }
}
fn state(ids: &[String]) -> Result<RefState, String> {
    let ids: Vec<ObjectId> = ids.iter().map(|s| s.parse()).collect::<Result<_, _>>()?;
    Ok(match ids.as_slice() {
        [] => RefState::Deleted,
        [id] => RefState::Target(*id),
        _ => RefState::DraftConflict(ids),
    })
}
/// Immutable preview. Storage, authorization, policy, closure, ancestry and leases
/// must still be checked by a future publisher before any linearization point.
#[derive(Clone, Debug)]
pub struct PublicationPlan {
    repository_id: String,
    before_view_id: ObjectId,
    candidate: View,
    prospective_receipt: PublicationReceipt,
    committed_status_bytes: usize,
}
impl PublicationPlan {
    pub fn prepare(
        repository_id: &str,
        current: &View,
        request: &UpdateRefsRequest,
        policy_outcome_id: String,
        durability: PublicationDurability,
    ) -> Result<Self, String> {
        request.validate()?;
        durability.validate()?;
        let intent = &request.intent;
        if repository_id != intent.repository_id {
            return Err("publication repository mismatch".into());
        }
        let before_view_id = current.id()?;
        if current.namespace_generation.to_string() != intent.expected_namespace_generation {
            return Err("stale publication namespace generation".into());
        }
        let next_generation = current
            .namespace_generation
            .checked_add(1)
            .ok_or("namespace generation exhausted")?;
        // Check the entire read set before building a candidate. No caller state is mutated.
        for update in &intent.updates {
            let existing = current
                .refs
                .binary_search_by(|r| r.name.cmp(&update.name))
                .ok()
                .map(|i| &current.refs[i]);
            match existing {
                None if update.expected_generation == "0"
                    && update.expected_old_targets.is_empty() => {}
                Some(r)
                    if r.generation.to_string() == update.expected_generation
                        && targets(&r.state) == update.expected_old_targets => {}
                _ => return Err(format!("stale publication ref: {}", update.name)),
            }
        }
        let mut refs: BTreeMap<String, ViewRef> = current
            .refs
            .iter()
            .cloned()
            .map(|r| (r.name.clone(), r))
            .collect();
        let mut published = Vec::with_capacity(intent.updates.len());
        for update in &intent.updates {
            let old_generation = update
                .expected_generation
                .parse::<u64>()
                .map_err(|_| "invalid generation")?;
            let next = old_generation
                .checked_add(1)
                .ok_or("ref generation exhausted")?;
            refs.insert(
                update.name.clone(),
                ViewRef {
                    name: update.name.clone(),
                    generation: next,
                    state: state(&update.new_targets)?,
                },
            );
            published.push(PublishedRef {
                name: update.name.clone(),
                old_generation: update.expected_generation.clone(),
                new_generation: next.to_string(),
                old_targets: update.expected_old_targets.clone(),
                new_targets: update.new_targets.clone(),
            });
        }
        let mut candidate = current.clone();
        candidate.namespace_generation = next_generation;
        candidate.refs = refs.into_values().collect();
        candidate.encode()?;
        let prospective_receipt = PublicationReceipt {
            schema_version: 1,
            transaction_id: intent.transaction_id.clone(),
            repository_id: repository_id.into(),
            request_digest: request.request_digest.clone(),
            repository_generation: next_generation.to_string(),
            operation_id: intent.operation_id.clone(),
            policy_outcome_id,
            durability: durability.clone(),
            refs: published,
        };
        prospective_receipt.validate(request, &durability)?;
        // Query IDs are fixed-width. Reserve the complete ordinary committed response,
        // including the envelope, rather than accepting a receipt that only fits alone.
        let query = ReceiptStatusRequest::new(request, "0".repeat(32))?;
        let response = ReceiptStatusReply {
            schema_version: 2,
            status: TransactionStatusReply {
                schema_version: 1,
                request_id: query.lookup.request_id.clone(),
                repository_id: repository_id.into(),
                transaction_id: intent.transaction_id.clone(),
                request_digest: request.request_digest.clone(),
                status: LookupStatus::Known,
                unknown_reason: None,
                result: Some(MutationResult {
                    schema_version: 1,
                    request_id: intent.transaction_id.clone(),
                    operation_id: Some(intent.operation_id.clone()),
                    outcome: MutationOutcome::Committed,
                    exit_code: 0,
                    generation: Some(next_generation.to_string()),
                    refs: vec![],
                    recovery: vec![],
                    external_receipts: vec![],
                    interrupted: false,
                }),
            },
            receipt: Some(prospective_receipt.clone()),
        };
        let committed_status_bytes = response.to_json(&query, request, &durability)?.len();
        Ok(Self {
            repository_id: repository_id.into(),
            before_view_id,
            candidate,
            prospective_receipt,
            committed_status_bytes,
        })
    }
    pub fn candidate_view(&self) -> &View {
        &self.candidate
    }
    /// Proposed data only. It must not be returned as committed before durable publication.
    pub fn prospective_receipt(&self) -> &PublicationReceipt {
        &self.prospective_receipt
    }
    pub fn committed_status_bytes(&self) -> usize {
        self.committed_status_bytes
    }
    pub fn before_view_id(&self) -> ObjectId {
        self.before_view_id
    }
    /// Recheck under the future writer lock; this method itself acquires no lock.
    pub fn recheck_snapshot(&self, repository_id: &str, current: &View) -> Result<(), String> {
        if repository_id != self.repository_id || current.id()? != self.before_view_id {
            return Err("publication snapshot changed; recompute the plan".into());
        }
        Ok(())
    }
}
