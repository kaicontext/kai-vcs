//! Bounded list-refs payloads, not a repository or authorization service.
use crate::{ObjectId, view::ref_types};
use serde::{Deserialize, Serialize};
const LIMIT: usize = 1024 * 1024;
fn uuid(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn generation(s: &str) -> Result<u64, String> {
    let n = s
        .parse::<u64>()
        .map_err(|_| "invalid ref-list generation")?;
    if n.to_string() != s {
        return Err("noncanonical ref-list generation".into());
    }
    Ok(n)
}
fn parse<T: serde::de::DeserializeOwned>(input: &[u8]) -> Result<T, String> {
    if input.len() > LIMIT {
        return Err("ref-list JSON size limit".into());
    }
    serde_json::from_slice(input).map_err(|e| e.to_string())
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("ref-list JSON size limit".into());
    }
    Ok(bytes)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefNamespace {
    Branches,
    Tags,
    Shelves,
    Remotes,
}
impl RefNamespace {
    fn contains(self, name: &str) -> bool {
        name.starts_with(match self {
            Self::Branches => "branches/",
            Self::Tags => "tags/",
            Self::Shelves => "shelves/",
            Self::Remotes => "remotes/",
        })
    }
    fn name(self, name: &str) -> Result<(), String> {
        ref_types(name)?;
        if !self.contains(name) {
            return Err("ref outside requested namespace".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRefsRequest {
    pub schema_version: u16,
    pub request_id: String,
    pub repository_id: String,
    pub namespace: RefNamespace,
    pub limit: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<String>,
}
impl ListRefsRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || !uuid(&self.request_id)
            || !uuid(&self.repository_id)
            || !(1..=1000).contains(&self.limit)
        {
            return Err("invalid ref-list request identity/version/limit".into());
        }
        if let Some(g) = &self.generation {
            generation(g)?;
        }
        if let Some(after) = &self.after {
            self.namespace.name(after)?;
            if self.generation.is_none() {
                return Err("ref-list cursor requires pinned generation".into());
            }
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListedRef {
    pub name: String,
    pub generation: String,
    pub targets: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRefsReply {
    pub schema_version: u16,
    pub request_id: String,
    pub repository_id: String,
    pub namespace: RefNamespace,
    pub generation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    pub refs: Vec<ListedRef>,
    pub has_more: bool,
}
impl ListRefsReply {
    pub fn validate(&self, request: &ListRefsRequest) -> Result<(), String> {
        request.validate()?;
        if self.schema_version != 1
            || self.request_id != request.request_id
            || self.repository_id != request.repository_id
            || self.namespace != request.namespace
            || self.after != request.after
        {
            return Err("ref-list reply/request binding mismatch".into());
        }
        generation(&self.generation)?;
        if request
            .generation
            .as_ref()
            .is_some_and(|g| g != &self.generation)
        {
            return Err("ref-list snapshot generation changed".into());
        }
        if self.refs.len() > usize::from(request.limit) || (self.has_more && self.refs.is_empty()) {
            return Err("invalid ref-list page length/progress".into());
        }
        let mut previous = request.after.as_deref();
        for r in &self.refs {
            self.namespace.name(&r.name)?;
            if previous.is_some_and(|p| p >= r.name.as_str()) {
                return Err("unordered or repeated ref-list key".into());
            }
            previous = Some(&r.name);
            if generation(&r.generation)? == 0 {
                return Err("zero ref generation".into());
            }
            if r.targets.len() > 1 && self.namespace != RefNamespace::Branches {
                return Err("alternatives require branch namespace".into());
            }
            if !r.targets.windows(2).all(|w| w[0] < w[1]) {
                return Err("unordered or duplicate ref targets".into());
            }
            for id in &r.targets {
                id.parse::<ObjectId>()?;
            }
        }
        Ok(())
    }
    pub fn from_json(input: &[u8], request: &ListRefsRequest) -> Result<Self, String> {
        let v: Self = parse(input)?;
        v.validate(request)?;
        Ok(v)
    }
    pub fn to_json(&self, request: &ListRefsRequest) -> Result<Vec<u8>, String> {
        self.validate(request)?;
        encode(self)
    }
    pub fn continuation(
        &self,
        request: &ListRefsRequest,
        next_request_id: String,
    ) -> Result<Option<ListRefsRequest>, String> {
        self.validate(request)?;
        if !self.has_more {
            return Ok(None);
        }
        if next_request_id == request.request_id {
            return Err("continuation requires fresh request ID".into());
        }
        let next = ListRefsRequest {
            request_id: next_request_id,
            generation: Some(self.generation.clone()),
            after: Some(
                self.refs
                    .last()
                    .ok_or("empty continuation page")?
                    .name
                    .clone(),
            ),
            ..request.clone()
        };
        next.validate()?;
        Ok(Some(next))
    }
}
