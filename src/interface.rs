//! Proposed JSON contracts, not a command executor or an authority service.
use crate::ObjectId;
use serde::{Deserialize, Serialize};
const LIMIT: usize = 1024 * 1024;
fn ordered<T: Ord>(a: &[T]) -> bool {
    a.windows(2).all(|w| w[0] < w[1])
}
fn labels(a: &[String]) -> Result<(), String> {
    if !ordered(a)
        || a.iter().any(|s| {
            s.is_empty()
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_./".contains(&b))
        })
    {
        return Err("invalid or unordered capability labels".into());
    }
    Ok(())
}
fn oid(s: &str) -> Result<(), String> {
    s.parse::<ObjectId>().map(|_| ())
}
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
fn parse<T: serde::de::DeserializeOwned>(input: &[u8]) -> Result<T, String> {
    if input.len() > LIMIT {
        return Err("JSON result limit exceeded".into());
    }
    serde_json::from_slice(input).map_err(|e| e.to_string())
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("JSON result limit exceeded".into());
    }
    Ok(bytes)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Durability {
    None,
    SingleNodeDurable,
    ReplicatedDurable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub schema_version: u16,
    pub protocol_versions: Vec<u16>,
    pub object_formats: Vec<String>,
    pub supported_features: Vec<String>,
    pub required_features: Vec<String>,
    pub max_object_bytes: u32,
    pub max_frame_bytes: u32,
    pub compression: Vec<String>,
    pub signature_formats: Vec<String>,
    pub ref_transactions: bool,
    pub partial: bool,
    pub shallow: bool,
    pub durability: Durability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replication_policy: Option<String>,
}
impl Capabilities {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.protocol_versions.is_empty()
            || self.protocol_versions.contains(&0)
            || !ordered(&self.protocol_versions)
            || self.max_object_bytes == 0
            || self.max_frame_bytes < self.max_object_bytes
        {
            return Err("invalid capability version or limits".into());
        }
        for a in [
            &self.object_formats,
            &self.supported_features,
            &self.required_features,
            &self.compression,
            &self.signature_formats,
        ] {
            labels(a)?;
        }
        if self.object_formats.is_empty()
            || self.compression.is_empty()
            || self
                .required_features
                .iter()
                .any(|f| self.supported_features.binary_search(f).is_err())
        {
            return Err("missing local required capability".into());
        }
        if (self.durability == Durability::ReplicatedDurable) != self.replication_policy.is_some() {
            return Err("replication policy/class mismatch".into());
        }
        if let Some(id) = &self.replication_policy {
            oid(id)?;
        }
        Ok(())
    }
    pub fn from_json(input: &[u8]) -> Result<Self, String> {
        let v: Self = parse(input)?;
        v.validate()?;
        Ok(v)
    }
    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        encode(self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferSelection {
    Complete,
    Partial,
    Shallow,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NegotiationRequest {
    pub selection: TransferSelection,
    pub require_ref_transactions: bool,
    pub required_signatures: Vec<String>,
    pub minimum_durability: Durability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replication_policy: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Negotiated {
    pub protocol_version: u16,
    pub object_format: String,
    pub compression: String,
    pub features: Vec<String>,
    pub signature_formats: Vec<String>,
    pub max_object_bytes: u32,
    pub max_frame_bytes: u32,
    pub selection: TransferSelection,
    pub ref_transactions: bool,
    pub receiver_durability: Durability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replication_policy: Option<String>,
}
fn intersection(a: &[String], b: &[String]) -> Vec<String> {
    a.iter()
        .filter(|v| b.binary_search(v).is_ok())
        .cloned()
        .collect()
}
pub fn negotiate(
    sender: &Capabilities,
    receiver: &Capabilities,
    request: &NegotiationRequest,
) -> Result<Negotiated, String> {
    sender.validate()?;
    receiver.validate()?;
    labels(&request.required_signatures)?;
    for (a, b) in [(sender, receiver), (receiver, sender)] {
        if a.required_features
            .iter()
            .any(|f| b.supported_features.binary_search(f).is_err())
        {
            return Err("unsupported required feature".into());
        }
    }
    let protocol_version = *sender
        .protocol_versions
        .iter()
        .rev()
        .find(|v| receiver.protocol_versions.contains(v))
        .ok_or("no common protocol")?;
    let object_format = intersection(&sender.object_formats, &receiver.object_formats)
        .into_iter()
        .next()
        .ok_or("no common object format")?;
    let compression = intersection(&sender.compression, &receiver.compression)
        .into_iter()
        .next()
        .ok_or("no common compression")?;
    let signature_formats = intersection(&sender.signature_formats, &receiver.signature_formats);
    if request
        .required_signatures
        .iter()
        .any(|s| signature_formats.binary_search(s).is_err())
    {
        return Err("unsupported required signature format".into());
    }
    if (request.selection == TransferSelection::Partial && !(sender.partial && receiver.partial))
        || (request.selection == TransferSelection::Shallow
            && !(sender.shallow && receiver.shallow))
        || (request.require_ref_transactions
            && !(sender.ref_transactions && receiver.ref_transactions))
    {
        return Err("unsupported requested transfer semantics".into());
    }
    if (request.minimum_durability == Durability::ReplicatedDurable)
        != request.replication_policy.is_some()
    {
        return Err("requested replication policy/class mismatch".into());
    }
    if let Some(id) = &request.replication_policy {
        oid(id)?;
    }
    if receiver.durability < request.minimum_durability
        || (request.minimum_durability == Durability::ReplicatedDurable
            && request.replication_policy != receiver.replication_policy)
    {
        return Err("receiver durability does not satisfy request".into());
    }
    Ok(Negotiated {
        protocol_version,
        object_format,
        compression,
        features: intersection(&sender.supported_features, &receiver.supported_features),
        signature_formats,
        max_object_bytes: sender.max_object_bytes.min(receiver.max_object_bytes),
        max_frame_bytes: sender.max_frame_bytes.min(receiver.max_frame_bytes),
        selection: request.selection,
        ref_transactions: sender.ref_transactions && receiver.ref_transactions,
        receiver_durability: receiver.durability,
        replication_policy: receiver.replication_policy.clone(),
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationOutcome {
    NotStarted,
    Prepared,
    Committed,
    CommittedMaterializationPending,
    Conflicted,
    Rejected,
    Indeterminate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptState {
    Complete,
    Pending,
    Failed,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalReceipt {
    pub system: String,
    pub receipt_id: String,
    pub state: ReceiptState,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultRef {
    pub name: String,
    pub generation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationResult {
    pub schema_version: u16,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    pub outcome: MutationOutcome,
    pub exit_code: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<String>,
    pub refs: Vec<ResultRef>,
    pub recovery: Vec<String>,
    pub external_receipts: Vec<ExternalReceipt>,
    pub interrupted: bool,
}
impl MutationResult {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || !uuid(&self.request_id)
            || self.operation_id.as_ref().is_some_and(|id| !uuid(id))
        {
            return Err("invalid result version or identity".into());
        }
        if let Some(g) = &self.generation {
            generation(g)?;
        } else if !self.refs.is_empty() {
            return Err("result refs require generation".into());
        }
        if !self.refs.windows(2).all(|w| w[0].name < w[1].name) {
            return Err("unordered result refs".into());
        }
        for r in &self.refs {
            crate::view::ref_types(&r.name)?;
            if generation(&r.generation)? == 0 {
                return Err("zero ref generation".into());
            }
            if let Some(id) = &r.target {
                oid(id)?;
            }
        }
        if self.recovery.iter().any(|s| s.trim().is_empty()) {
            return Err("empty recovery instruction".into());
        }
        let mut receipts = std::collections::HashSet::new();
        for r in &self.external_receipts {
            if r.system.trim().is_empty()
                || r.receipt_id.trim().is_empty()
                || !receipts.insert((&r.system, &r.receipt_id))
            {
                return Err("invalid or duplicate external receipt".into());
            }
        }
        use MutationOutcome::*;
        let failure = matches!(self.exit_code, 1 | 2 | 4 | 5 | 6 | 7 | 8);
        let op = self.operation_id.is_some();
        let gen_present = self.generation.is_some();
        let recovery = !self.recovery.is_empty();
        let valid = match self.outcome {
            NotStarted => failure && !op && !gen_present && self.external_receipts.is_empty(),
            Prepared => {
                self.exit_code == 1
                    && op
                    && !gen_present
                    && recovery
                    && self.external_receipts.is_empty()
            }
            Committed => {
                self.exit_code == 0
                    && op
                    && gen_present
                    && self
                        .external_receipts
                        .iter()
                        .all(|r| r.state == ReceiptState::Complete)
            }
            CommittedMaterializationPending => self.exit_code == 9 && op && gen_present && recovery,
            Conflicted => {
                self.exit_code == 3
                    && op
                    && recovery
                    && self
                        .external_receipts
                        .iter()
                        .all(|r| r.state == ReceiptState::Complete)
            }
            Rejected => failure && !gen_present && self.external_receipts.is_empty(),
            Indeterminate => self.exit_code == 10 && op && !gen_present && recovery,
        };
        if !valid {
            return Err("contradictory mutation outcome, exit code or effects".into());
        }
        Ok(())
    }
    pub fn from_json(input: &[u8]) -> Result<Self, String> {
        let v: Self = parse(input)?;
        v.validate()?;
        Ok(v)
    }
    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        encode(self)
    }
}

/// Versioned negotiation payload; session authorization is the caller's job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NegotiationEnvelope {
    pub schema_version: u16,
    pub request_id: String,
    pub repository_id: String,
    pub policy: NegotiationRequest,
}
impl NegotiationEnvelope {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || !uuid(&self.request_id) || !uuid(&self.repository_id) {
            return Err("invalid negotiation version or identity".into());
        }
        labels(&self.policy.required_signatures)?;
        if (self.policy.minimum_durability == Durability::ReplicatedDurable)
            != self.policy.replication_policy.is_some()
        {
            return Err("requested replication policy/class mismatch".into());
        }
        if let Some(id) = &self.policy.replication_policy {
            oid(id)?;
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
    pub fn reply(
        &self,
        sender: &Capabilities,
        receiver: &Capabilities,
    ) -> Result<NegotiationReply, String> {
        self.validate()?;
        Ok(NegotiationReply {
            schema_version: 1,
            request_id: self.request_id.clone(),
            repository_id: self.repository_id.clone(),
            negotiated: negotiate(sender, receiver, &self.policy)?,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NegotiationReply {
    pub schema_version: u16,
    pub request_id: String,
    pub repository_id: String,
    pub negotiated: Negotiated,
}
impl NegotiationReply {
    /// Require the original request and trusted offers at every acceptance point.
    pub fn verify_json(
        input: &[u8],
        request: &NegotiationEnvelope,
        sender: &Capabilities,
        receiver: &Capabilities,
    ) -> Result<Self, String> {
        let value: Self = parse(input)?;
        if value != request.reply(sender, receiver)? {
            return Err("negotiation reply does not match request and offers".into());
        }
        Ok(value)
    }
    pub fn to_json(
        &self,
        request: &NegotiationEnvelope,
        sender: &Capabilities,
        receiver: &Capabilities,
    ) -> Result<Vec<u8>, String> {
        if self != &request.reply(sender, receiver)? {
            return Err("negotiation reply does not match request and offers".into());
        }
        encode(self)
    }
}
