//! Experimental update-refs intent and digest contract; performs no publication.
use crate::{ObjectId, transaction_status::TransactionStatusRequest, view::ref_types};
use serde::{Deserialize, Serialize};
const LIMIT: usize = 1024 * 1024;
const DOMAIN: &[u8] = b"kai.update-refs.request.v1\0";
fn uuid(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn generation(s: &str) -> Result<u64, String> {
    let n = s.parse::<u64>().map_err(|_| "invalid generation")?;
    if n.to_string() != s {
        return Err("noncanonical generation".into());
    }
    Ok(n)
}
fn ids(ids: &[String]) -> Result<(), String> {
    if !ids.windows(2).all(|w| w[0] < w[1]) {
        return Err("IDs must be sorted and unique".into());
    }
    for id in ids {
        id.parse::<ObjectId>()?;
    }
    Ok(())
}
fn text(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}
fn strings(out: &mut Vec<u8>, values: &[String]) {
    out.extend_from_slice(&(values.len() as u64).to_be_bytes());
    for value in values {
        text(out, value);
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefUpdate {
    pub name: String,
    pub expected_generation: String,
    pub expected_old_targets: Vec<String>,
    pub new_targets: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRefsIntent {
    pub schema_version: u16,
    pub transaction_id: String,
    pub repository_id: String,
    pub expected_namespace_generation: String,
    pub operation_id: String,
    pub reason: String,
    pub updates: Vec<RefUpdate>,
    pub required_closure_manifest: String,
    pub policy_input_ids: Vec<String>,
    pub client_capabilities: Vec<String>,
}
impl UpdateRefsIntent {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || !uuid(&self.transaction_id) || !uuid(&self.repository_id) {
            return Err("invalid version or identity".into());
        }
        generation(&self.expected_namespace_generation)?;
        if !uuid(&self.operation_id) {
            return Err("invalid operation ID".into());
        }
        self.required_closure_manifest.parse::<ObjectId>()?;
        if self.reason.is_empty()
            || self.reason.len() > 4096
            || self.reason.chars().any(char::is_control)
        {
            return Err("invalid reason".into());
        }
        if self.updates.is_empty()
            || self.updates.len() > 1000
            || !self.updates.windows(2).all(|w| w[0].name < w[1].name)
        {
            return Err("updates must be nonempty, sorted, unique and bounded".into());
        }
        for update in &self.updates {
            ref_types(&update.name)?;
            let g = generation(&update.expected_generation)?;
            ids(&update.expected_old_targets)?;
            ids(&update.new_targets)?;
            if (g == 0 && !update.expected_old_targets.is_empty())
                || (update.expected_old_targets.is_empty() && update.new_targets.is_empty())
                || (!update.name.starts_with("branches/")
                    && (update.expected_old_targets.len() > 1 || update.new_targets.len() > 1))
            {
                return Err("invalid absent, deleted, or alternative ref state".into());
            }
        }
        ids(&self.policy_input_ids)?;
        if !self.client_capabilities.windows(2).all(|w| w[0] < w[1])
            || self.client_capabilities.iter().any(|s| {
                s.is_empty()
                    || s.len() > 128
                    || !s.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b"-._".contains(&b)
                    })
            })
        {
            return Err("invalid capability set".into());
        }
        // Bound constructed values as well as untrusted wire input.
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > LIMIT - 128 {
            return Err("update-refs size limit".into());
        }
        Ok(())
    }
    /// Versioned binary digest preimage, independent of JSON field order/escaping.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut out = DOMAIN.to_vec();
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        for s in [
            &self.transaction_id,
            &self.repository_id,
            &self.expected_namespace_generation,
            &self.operation_id,
            &self.reason,
        ] {
            text(&mut out, s);
        }
        out.extend_from_slice(&(self.updates.len() as u64).to_be_bytes());
        for u in &self.updates {
            text(&mut out, &u.name);
            text(&mut out, &u.expected_generation);
            strings(&mut out, &u.expected_old_targets);
            strings(&mut out, &u.new_targets);
        }
        text(&mut out, &self.required_closure_manifest);
        strings(&mut out, &self.policy_input_ids);
        strings(&mut out, &self.client_capabilities);
        Ok(out)
    }
    pub fn request_digest(&self) -> Result<String, String> {
        Ok(format!(
            "b3:{}",
            blake3::hash(&self.canonical_bytes()?).to_hex()
        ))
    }
    pub fn into_request(self) -> Result<UpdateRefsRequest, String> {
        let request_digest = self.request_digest()?;
        Ok(UpdateRefsRequest {
            intent: self,
            request_digest,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRefsRequest {
    pub intent: UpdateRefsIntent,
    pub request_digest: String,
}
impl UpdateRefsRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.request_digest != self.intent.request_digest()? {
            return Err("update-refs digest mismatch".into());
        }
        Ok(())
    }
    pub fn from_json(input: &[u8]) -> Result<Self, String> {
        if input.len() > LIMIT {
            return Err("update-refs size limit".into());
        }
        let value: Self = serde_json::from_slice(input).map_err(|e| e.to_string())?;
        value.validate()?;
        Ok(value)
    }
    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > LIMIT {
            return Err("update-refs size limit".into());
        }
        Ok(bytes)
    }
    pub fn status_query(&self, request_id: String) -> Result<TransactionStatusRequest, String> {
        self.validate()?;
        let query = TransactionStatusRequest {
            schema_version: 1,
            request_id,
            repository_id: self.intent.repository_id.clone(),
            transaction_id: self.intent.transaction_id.clone(),
            request_digest: self.request_digest.clone(),
        };
        query.validate()?;
        Ok(query)
    }
}
