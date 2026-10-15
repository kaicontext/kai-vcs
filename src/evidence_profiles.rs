//! Canonical embedded profiles and bounded evidence-context verification.
use crate::{Blob, ClosureLimits, EntryKind, Evidence, EvidenceKind, Object, ObjectId, Target};
use serde::{Deserialize, Serialize};
const LIMIT: usize = 1024 * 1024;
pub const INPUT_FORMAT: &[u8] = b"kai/evidence-inputs/v1";
pub const ENVIRONMENT_FORMAT: &[u8] = b"kai/evidence-environment/v1";
fn hex(s: &str, n: usize) -> bool {
    s.len() == n
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn digest(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix).is_some_and(|s| hex(s, 64))
}
fn oid(s: &str) -> Result<ObjectId, String> {
    s.parse()
}
fn nonempty(s: &str) -> bool {
    !s.trim().is_empty()
}
fn sorted<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut last = None;
    for v in values {
        if !nonempty(v) || last.is_some_and(|p| p >= v) {
            return false;
        }
        last = Some(v);
    }
    true
}
fn path(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) || !hex(s, s.len()) {
        return Err("invalid hex path".into());
    }
    let b: Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect();
    if !crate::recovery::path(&b) {
        return Err("invalid submodule path".into());
    }
    Ok(b)
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let b = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if b.len() > LIMIT {
        return Err("profile size limit exceeded".into());
    }
    Ok(b)
}
fn decode<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, String> {
    if bytes.len() > LIMIT {
        return Err("profile size limit exceeded".into());
    }
    let v: T = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if encode(&v)? != bytes {
        return Err("noncanonical profile JSON".into());
    }
    Ok(v)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyInput {
    pub name: String,
    pub digest: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubmoduleAlgorithm {
    Sha1,
    Sha256,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmoduleInput {
    pub path_hex: String,
    pub algorithm: SubmoduleAlgorithm,
    pub object_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewInput {
    pub identifier: String,
    pub target_revision: String,
    pub policy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceInputs {
    pub schema_version: u16,
    pub revision: String,
    pub tree: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty_overlay_digest: Option<String>,
    pub dependencies: Vec<DependencyInput>,
    pub submodules: Vec<SubmoduleInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewInput>,
}
impl EvidenceInputs {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self
                .dirty_overlay_digest
                .as_ref()
                .is_some_and(|s| !digest(s, "b3:"))
            || !sorted(self.dependencies.iter().map(|v| v.name.as_str()))
            || self.dependencies.iter().any(|v| !digest(&v.digest, "b3:"))
        {
            return Err("invalid input profile".into());
        }
        oid(&self.revision)?;
        oid(&self.tree)?;
        let mut last: Option<Vec<u8>> = None;
        for s in &self.submodules {
            let p = path(&s.path_hex)?;
            if last.as_ref().is_some_and(|last| *last >= p)
                || !hex(
                    &s.object_id,
                    match s.algorithm {
                        SubmoduleAlgorithm::Sha1 => 40,
                        SubmoduleAlgorithm::Sha256 => 64,
                    },
                )
            {
                return Err("invalid or unordered submodule identity".into());
            }
            last = Some(p);
        }
        if let Some(r) = &self.review {
            if !nonempty(&r.identifier) {
                return Err("empty review identifier".into());
            }
            oid(&r.target_revision)?;
            oid(&r.policy)?;
            if let Some(id) = &r.base_revision {
                oid(id)?;
            }
        }
        Ok(())
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        encode(self)
    }
    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        let v: Self = decode(b)?;
        v.validate()?;
        Ok(v)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainInput {
    pub name: String,
    pub version: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableKind {
    Public,
    OpaqueSecretVersion,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentVariable {
    pub name: String,
    pub kind: VariableKind,
    pub value: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceEnvironment {
    pub schema_version: u16,
    pub platform: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_digest: Option<String>,
    pub toolchains: Vec<ToolchainInput>,
    pub variables: Vec<EnvironmentVariable>,
    pub limitations: Vec<String>,
}
impl EvidenceEnvironment {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || !nonempty(&self.platform)
            || self
                .container_digest
                .as_ref()
                .is_some_and(|s| !digest(s, "sha256:"))
            || !sorted(self.toolchains.iter().map(|v| v.name.as_str()))
            || self.toolchains.iter().any(|v| !nonempty(&v.version))
            || !sorted(self.variables.iter().map(|v| v.name.as_str()))
            || !sorted(self.limitations.iter().map(String::as_str))
        {
            return Err("invalid environment profile".into());
        }
        for v in &self.variables {
            if v.name.contains(['\0', '='])
                || (v.kind == VariableKind::OpaqueSecretVersion && !nonempty(&v.value))
            {
                return Err("invalid environment token".into());
            }
        }
        Ok(())
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        encode(self)
    }
    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        let v: Self = decode(b)?;
        v.validate()?;
        Ok(v)
    }
}
/// Validates declared immutable context, not execution, trust or policy acceptance.
pub fn verify_evidence_context(
    e: &Evidence,
    limits: ClosureLimits,
    mut read: impl FnMut(ObjectId) -> Result<Vec<u8>, String>,
) -> Result<(EvidenceInputs, EvidenceEnvironment), String> {
    if e.inputs.format != INPUT_FORMAT || e.environment.format != ENVIRONMENT_FORMAT {
        return Err("unsupported evidence profile".into());
    }
    let root = e.encode()?;
    let root_id = ObjectId::hash(&root);
    let mut total = 0u64;
    let mut reads = 0usize;
    let mut bounded = |id| {
        reads = reads.checked_add(1).ok_or("profile read overflow")?;
        if reads > limits.max_objects {
            return Err("profile read limit exceeded".into());
        }
        let bytes = if id == root_id {
            root.clone()
        } else {
            read(id)?
        };
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or("profile byte overflow")?;
        if total > limits.max_bytes {
            return Err("profile byte limit exceeded".into());
        }
        Ok(bytes)
    };
    crate::verify_closure(&[root_id], limits, &mut bounded)?;
    let input = match Object::verify(e.inputs.object, &bounded(e.inputs.object)?)? {
        Object::Blob(Blob { data }) => EvidenceInputs::from_bytes(&data)?,
        _ => return Err("v1 inputs require an ordinary blob".into()),
    };
    let environment = match Object::verify(e.environment.object, &bounded(e.environment.object)?)? {
        Object::Blob(Blob { data }) => EvidenceEnvironment::from_bytes(&data)?,
        _ => return Err("v1 environment requires an ordinary blob".into()),
    };
    if oid(&input.revision)? != e.revision || oid(&input.tree)? != e.tree {
        return Err("input profile source mismatch".into());
    }
    match (e.kind, &input.review) {
        (EvidenceKind::Execution, None) => {}
        (EvidenceKind::Review, Some(r))
            if e.invocation.len() == 1 && e.invocation[0] == r.identifier.as_bytes() => {}
        _ => return Err("review context mismatch".into()),
    }
    if environment
        .limitations
        .iter()
        .any(|s| !e.limitations.iter().any(|b| b == s.as_bytes()))
    {
        return Err("environment limitations hidden by evidence".into());
    }
    let mut stack = vec![(e.tree, Vec::<u8>::new())];
    let mut actual = Vec::new();
    let mut work = 0usize;
    let mut path_bytes = 0u64;
    while let Some((id, prefix)) = stack.pop() {
        let Object::Tree(tree) = Object::verify(id, &bounded(id)?)? else {
            return Err("expected source tree".into());
        };
        for entry in tree.entries() {
            work = work.checked_add(1).ok_or("profile path work overflow")?;
            if work > limits.max_edges {
                return Err("profile path work limit exceeded".into());
            }
            let mut p = prefix.clone();
            if !p.is_empty() {
                p.push(b'/');
            }
            p.extend(&entry.name);
            path_bytes = path_bytes
                .checked_add(p.len() as u64)
                .ok_or("profile path bytes overflow")?;
            if path_bytes > limits.max_bytes {
                return Err("profile path byte limit exceeded".into());
            }
            match (entry.kind, &entry.target) {
                (EntryKind::Directory, Target::Native(child)) => stack.push((*child, p)),
                (EntryKind::Gitlink, Target::GitSha1(id)) => actual.push(SubmoduleInput {
                    path_hex: to_hex(&p),
                    algorithm: SubmoduleAlgorithm::Sha1,
                    object_id: to_hex(id),
                }),
                (EntryKind::Gitlink, Target::GitSha256(id)) => actual.push(SubmoduleInput {
                    path_hex: to_hex(&p),
                    algorithm: SubmoduleAlgorithm::Sha256,
                    object_id: to_hex(id),
                }),
                _ => {}
            }
        }
    }
    actual.sort_by(|a, b| a.path_hex.cmp(&b.path_hex));
    if actual != input.submodules {
        return Err("submodule inputs disagree with captured tree".into());
    }
    Ok((input, environment))
}
fn to_hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
