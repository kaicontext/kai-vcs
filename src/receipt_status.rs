//! Explicit v2 transaction-status envelope for update-refs receipt recovery.
use crate::{
    interface::MutationOutcome,
    publication_receipt::{PublicationDurability, PublicationReceipt},
    transaction_status::{StatusAdvice, TransactionStatusReply, TransactionStatusRequest},
    update_refs::UpdateRefsRequest,
};
use serde::{Deserialize, Serialize};
const LIMIT: usize = 1024 * 1024;
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("receipt-status envelope size limit".into());
    }
    Ok(bytes)
}
fn parse<T: serde::de::DeserializeOwned>(input: &[u8]) -> Result<T, String> {
    if input.len() > LIMIT {
        return Err("receipt-status envelope size limit".into());
    }
    serde_json::from_slice(input).map_err(|e| e.to_string())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptStatusRequest {
    pub schema_version: u16,
    pub lookup: TransactionStatusRequest,
}
impl ReceiptStatusRequest {
    pub fn new(original: &UpdateRefsRequest, request_id: String) -> Result<Self, String> {
        Ok(Self {
            schema_version: 2,
            lookup: original.status_query(request_id)?,
        })
    }
    pub fn validate(&self, original: &UpdateRefsRequest) -> Result<(), String> {
        self.lookup.validate()?;
        if self.schema_version != 2
            || self.lookup != original.status_query(self.lookup.request_id.clone())?
        {
            return Err("receipt-status query does not bind original publication".into());
        }
        Ok(())
    }
    pub fn from_json(input: &[u8], original: &UpdateRefsRequest) -> Result<Self, String> {
        let value: Self = parse(input)?;
        value.validate(original)?;
        Ok(value)
    }
    pub fn to_json(&self, original: &UpdateRefsRequest) -> Result<Vec<u8>, String> {
        self.validate(original)?;
        encode(self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptStatusReply {
    pub schema_version: u16,
    pub status: TransactionStatusReply,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<PublicationReceipt>,
}
impl ReceiptStatusReply {
    pub fn validate(
        &self,
        query: &ReceiptStatusRequest,
        original: &UpdateRefsRequest,
        durability: &PublicationDurability,
    ) -> Result<(), String> {
        query.validate(original)?;
        durability.validate()?;
        if self.schema_version != 2 {
            return Err("receipt-status requires envelope version 2".into());
        }
        self.status.validate(&query.lookup)?;
        let mut committed = false;
        if let Some(result) = &self.status.result {
            // The complete ref states live in the receipt, never a lossy singular-target summary.
            if !result.refs.is_empty()
                || result
                    .operation_id
                    .as_ref()
                    .is_some_and(|id| id != &original.intent.operation_id)
            {
                return Err(
                    "receipt-status result has conflicting refs or operation identity".into(),
                );
            }
            committed = matches!(
                result.outcome,
                MutationOutcome::Committed | MutationOutcome::CommittedMaterializationPending
            );
            if !committed && result.generation.is_some() {
                return Err("noncommitted publication cannot claim a generation".into());
            }
        }
        if committed {
            let receipt = self
                .receipt
                .as_ref()
                .ok_or("committed status requires full receipt")?;
            receipt.validate(original, durability)?;
            let result = self
                .status
                .result
                .as_ref()
                .ok_or("missing committed result")?;
            if result.generation.as_ref() != Some(&receipt.repository_generation) {
                return Err("status/receipt generation mismatch".into());
            }
        } else if self.receipt.is_some() {
            return Err("noncommitted, unknown or digest-mismatch status forbids receipt".into());
        }
        encode(self)?;
        Ok(())
    }
    pub fn from_json(
        input: &[u8],
        query: &ReceiptStatusRequest,
        original: &UpdateRefsRequest,
        durability: &PublicationDurability,
    ) -> Result<Self, String> {
        let value: Self = parse(input)?;
        value.validate(query, original, durability)?;
        Ok(value)
    }
    pub fn to_json(
        &self,
        query: &ReceiptStatusRequest,
        original: &UpdateRefsRequest,
        durability: &PublicationDurability,
    ) -> Result<Vec<u8>, String> {
        self.validate(query, original, durability)?;
        encode(self)
    }
    /// No action authorizes automatically repeating the mutation.
    pub fn advice(
        &self,
        query: &ReceiptStatusRequest,
        original: &UpdateRefsRequest,
        durability: &PublicationDurability,
    ) -> Result<StatusAdvice, String> {
        self.validate(query, original, durability)?;
        self.status.advice(&query.lookup)
    }
}
