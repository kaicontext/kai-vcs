//! Bounded outcome lookup contract; no transaction execution or receipt storage.
use crate::interface::{MutationOutcome, MutationResult};
use serde::{Deserialize, Serialize};
const LIMIT: usize = 1024 * 1024;
fn hex(s: &str, length: usize) -> bool {
    s.len() == length
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn parse<T: serde::de::DeserializeOwned>(input: &[u8]) -> Result<T, String> {
    if input.len() > LIMIT {
        return Err("transaction-status size limit".into());
    }
    serde_json::from_slice(input).map_err(|e| e.to_string())
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("transaction-status size limit".into());
    }
    Ok(bytes)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionStatusRequest {
    pub schema_version: u16,
    pub request_id: String,
    pub repository_id: String,
    pub transaction_id: String,
    pub request_digest: String,
}
impl TransactionStatusRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || !hex(&self.request_id, 32)
            || !hex(&self.repository_id, 32)
            || !hex(&self.transaction_id, 32)
            || !self
                .request_digest
                .strip_prefix("b3:")
                .is_some_and(|s| hex(s, 64))
        {
            return Err("invalid transaction-status version/identity/digest".into());
        }
        Ok(())
    }
    pub fn from_json(input: &[u8]) -> Result<Self, String> {
        let value: Self = parse(input)?;
        value.validate()?;
        Ok(value)
    }
    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        encode(self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookupStatus {
    Known,
    Unknown,
    DigestMismatch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownReason {
    Missing,
    Expired,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionStatusReply {
    pub schema_version: u16,
    pub request_id: String,
    pub repository_id: String,
    pub transaction_id: String,
    pub request_digest: String,
    pub status: LookupStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<MutationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown_reason: Option<UnknownReason>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusAdvice {
    Complete,
    RecoverCommitted,
    ResolveConflict,
    QueryAgain,
    Reconcile,
    NewIntentRequired,
    ResolveDigestMismatch,
}
impl TransactionStatusReply {
    pub fn validate(&self, request: &TransactionStatusRequest) -> Result<(), String> {
        request.validate()?;
        if self.schema_version != 1
            || self.request_id != request.request_id
            || self.repository_id != request.repository_id
            || self.transaction_id != request.transaction_id
            || self.request_digest != request.request_digest
        {
            return Err("transaction-status reply/request mismatch".into());
        }
        match self.status {
            LookupStatus::Known => {
                if self.unknown_reason.is_some() {
                    return Err("known status cannot claim unknown reason".into());
                }
                let result = self.result.as_ref().ok_or("known status requires result")?;
                result.validate()?;
                if result.request_id != request.transaction_id {
                    return Err("result belongs to another transaction".into());
                }
            }
            LookupStatus::Unknown => {
                if self.result.is_some() || self.unknown_reason.is_none() {
                    return Err("unknown status requires reason and forbids result".into());
                }
            }
            LookupStatus::DigestMismatch => {
                if self.result.is_some() || self.unknown_reason.is_some() {
                    return Err("digest mismatch must not disclose a result".into());
                }
            }
        }
        Ok(())
    }
    pub fn from_json(input: &[u8], request: &TransactionStatusRequest) -> Result<Self, String> {
        let value: Self = parse(input)?;
        value.validate(request)?;
        Ok(value)
    }
    pub fn to_json(&self, request: &TransactionStatusRequest) -> Result<Vec<u8>, String> {
        self.validate(request)?;
        encode(self)
    }
    /// Guidance for an authenticated result. No variant authorizes automatic replay.
    pub fn advice(&self, request: &TransactionStatusRequest) -> Result<StatusAdvice, String> {
        self.validate(request)?;
        Ok(match self.status {
            LookupStatus::Unknown => StatusAdvice::Reconcile,
            LookupStatus::DigestMismatch => StatusAdvice::ResolveDigestMismatch,
            LookupStatus::Known => match self.result.as_ref().ok_or("missing result")?.outcome {
                MutationOutcome::Committed => StatusAdvice::Complete,
                MutationOutcome::CommittedMaterializationPending => StatusAdvice::RecoverCommitted,
                MutationOutcome::Conflicted => StatusAdvice::ResolveConflict,
                MutationOutcome::Prepared => StatusAdvice::QueryAgain,
                MutationOutcome::Indeterminate => StatusAdvice::Reconcile,
                MutationOutcome::Rejected | MutationOutcome::NotStarted => {
                    StatusAdvice::NewIntentRequired
                }
            },
        })
    }
}
