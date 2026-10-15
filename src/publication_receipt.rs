//! Committed publication receipt contract. Claims require an authenticated server.
use crate::{ObjectId, interface::Durability, update_refs::UpdateRefsRequest};
use serde::{Deserialize, Serialize};
const LIMIT: usize = 1024 * 1024;
fn generation(s: &str) -> Result<u64, String> {
    let n = s.parse::<u64>().map_err(|_| "invalid receipt generation")?;
    if n.to_string() != s {
        return Err("noncanonical receipt generation".into());
    }
    Ok(n)
}
fn successor(old: &str, new: &str) -> Result<(), String> {
    if generation(old)?.checked_add(1) != Some(generation(new)?) {
        return Err("receipt generation must advance exactly once without overflow".into());
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationDurability {
    pub class: Durability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replication_policy: Option<String>,
}
impl PublicationDurability {
    pub fn validate(&self) -> Result<(), String> {
        if self.class == Durability::None
            || (self.class == Durability::ReplicatedDurable) != self.replication_policy.is_some()
        {
            return Err("publication requires a durable class and matching policy presence".into());
        }
        if let Some(id) = &self.replication_policy {
            id.parse::<ObjectId>()?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedRef {
    pub name: String,
    pub old_generation: String,
    pub new_generation: String,
    pub old_targets: Vec<String>,
    pub new_targets: Vec<String>,
}
/// A successful committed batch, never a prepared/rejected/unknown outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationReceipt {
    pub schema_version: u16,
    pub transaction_id: String,
    pub repository_id: String,
    pub request_digest: String,
    pub repository_generation: String,
    pub operation_id: String,
    pub policy_outcome_id: String,
    pub durability: PublicationDurability,
    pub refs: Vec<PublishedRef>,
}
impl PublicationReceipt {
    /// `expected_durability` must come from the trusted session, not this receipt.
    pub fn validate(
        &self,
        request: &UpdateRefsRequest,
        expected_durability: &PublicationDurability,
    ) -> Result<(), String> {
        request.validate()?;
        expected_durability.validate()?;
        self.durability.validate()?;
        let intent = &request.intent;
        if self.schema_version != 1
            || self.transaction_id != intent.transaction_id
            || self.repository_id != intent.repository_id
            || self.request_digest != request.request_digest
            || self.operation_id != intent.operation_id
            || &self.durability != expected_durability
        {
            return Err("publication receipt/request/session mismatch".into());
        }
        self.policy_outcome_id.parse::<ObjectId>()?;
        successor(
            &intent.expected_namespace_generation,
            &self.repository_generation,
        )?;
        if self.refs.len() != intent.updates.len() {
            return Err("receipt must cover the whole batch".into());
        }
        for (published, update) in self.refs.iter().zip(&intent.updates) {
            if published.name != update.name
                || published.old_generation != update.expected_generation
                || published.old_targets != update.expected_old_targets
                || published.new_targets != update.new_targets
            {
                return Err("receipt ref state differs from request".into());
            }
            successor(&published.old_generation, &published.new_generation)?;
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > LIMIT {
            return Err("publication receipt size limit".into());
        }
        Ok(())
    }
    pub fn from_json(
        input: &[u8],
        request: &UpdateRefsRequest,
        expected_durability: &PublicationDurability,
    ) -> Result<Self, String> {
        if input.len() > LIMIT {
            return Err("publication receipt size limit".into());
        }
        let value: Self = serde_json::from_slice(input).map_err(|e| e.to_string())?;
        value.validate(request, expected_durability)?;
        Ok(value)
    }
    pub fn to_json(
        &self,
        request: &UpdateRefsRequest,
        expected_durability: &PublicationDurability,
    ) -> Result<Vec<u8>, String> {
        self.validate(request, expected_durability)?;
        serde_json::to_vec(self).map_err(|e| e.to_string())
    }
}
