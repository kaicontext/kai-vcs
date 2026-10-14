//! Declared collaboration context; does not authenticate or replay operations.
use crate::{ClosureLimits, CollaborationBarrier, EntryKind, Object, ObjectId, Target};
use serde::{Deserialize, Serialize};

pub const FORMAT: &[u8] = b"kai/collaboration-frontier/v1";
const LIMIT: usize = 1024 * 1024;
fn hex(s: &str, n: usize) -> bool {
    s.len() == n
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn to_hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
fn counter(s: &str) -> Result<u64, String> {
    let n: u64 = s.parse().map_err(|_| "invalid frontier counter")?;
    if n.to_string() != s {
        return Err("noncanonical frontier counter".into());
    }
    Ok(n)
}
fn path(s: &str) -> Result<(), String> {
    if !s.len().is_multiple_of(2) || !hex(s, s.len()) {
        return Err("invalid hex path".into());
    }
    let bytes: Vec<_> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect();
    if !crate::recovery::path(&bytes) {
        return Err("invalid frontier path".into());
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplicaFrontier {
    pub replica_hex: String,
    pub sequence: String,
    pub operations_digest: String,
}
fn validate_frontier(v: &[ReplicaFrontier]) -> Result<(), String> {
    if !v.windows(2).all(|w| w[0].replica_hex < w[1].replica_hex) {
        return Err("unordered replica frontier".into());
    }
    for r in v {
        if !hex(&r.replica_hex, 32)
            || !r
                .operations_digest
                .strip_prefix("b3:")
                .is_some_and(|s| hex(s, 64))
        {
            return Err("invalid replica identity or log digest".into());
        }
        counter(&r.sequence)?;
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializedEntry {
    pub path_hex: String,
    pub kind: u8,
    pub target: String,
    pub frontier: Vec<ReplicaFrontier>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationFrontier {
    pub schema_version: u16,
    pub workspace_hex: String,
    pub epoch_hex: String,
    pub generation: String,
    pub tree: String,
    pub protocol_hex: String,
    pub namespace: Vec<ReplicaFrontier>,
    pub entries: Vec<MaterializedEntry>,
}
impl CollaborationFrontier {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || !hex(&self.workspace_hex, 32)
            || !hex(&self.epoch_hex, 32)
            || self.protocol_hex.is_empty()
            || !self.protocol_hex.len().is_multiple_of(2)
            || !hex(&self.protocol_hex, self.protocol_hex.len())
            || self.namespace.is_empty()
            || !self
                .entries
                .windows(2)
                .all(|w| w[0].path_hex < w[1].path_hex)
        {
            return Err("invalid collaboration profile".into());
        }
        counter(&self.generation)?;
        self.tree.parse::<ObjectId>()?;
        validate_frontier(&self.namespace)?;
        for e in &self.entries {
            path(&e.path_hex)?;
            match e.kind {
                1..=4 => {
                    e.target.parse::<ObjectId>()?;
                }
                5 if e.target.strip_prefix("sha1:").is_some_and(|s| hex(s, 40))
                    || e.target.strip_prefix("sha256:").is_some_and(|s| hex(s, 64)) => {}
                _ => return Err("invalid materialized kind/target".into()),
            }
            validate_frontier(&e.frontier)?;
            for r in &e.frontier {
                let i = self
                    .namespace
                    .binary_search_by(|p| p.replica_hex.cmp(&r.replica_hex))
                    .map_err(|_| "file frontier includes nonparticipant")?;
                if counter(&r.sequence)? > counter(&self.namespace[i].sequence)? {
                    return Err("file frontier exceeds barrier".into());
                }
            }
        }
        Ok(())
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let b = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if b.len() > LIMIT {
            return Err("frontier profile size limit exceeded".into());
        }
        Ok(b)
    }
    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        if b.len() > LIMIT {
            return Err("frontier profile size limit exceeded".into());
        }
        let v: Self = serde_json::from_slice(b).map_err(|e| e.to_string())?;
        if v.to_bytes()? != b {
            return Err("noncanonical frontier JSON".into());
        }
        Ok(v)
    }
}
/// Verify immutable claims and exact materialization, not live capture consistency.
pub fn verify_collaboration_frontier(
    barrier: &CollaborationBarrier,
    limits: ClosureLimits,
    mut read: impl FnMut(ObjectId) -> Result<Vec<u8>, String>,
) -> Result<CollaborationFrontier, String> {
    if barrier.manifest_format != FORMAT {
        return Err("unsupported collaboration profile".into());
    }
    let root = barrier.encode()?;
    let root_id = ObjectId::hash(&root);
    let mut reads = 0usize;
    let mut bytes = 0u64;
    let mut bounded = |id| {
        reads = reads.checked_add(1).ok_or("frontier read overflow")?;
        if reads > limits.max_objects {
            return Err("frontier read limit exceeded".into());
        }
        let b = if id == root_id {
            root.clone()
        } else {
            read(id)?
        };
        bytes = bytes
            .checked_add(b.len() as u64)
            .ok_or("frontier byte overflow")?;
        if bytes > limits.max_bytes {
            return Err("frontier byte limit exceeded".into());
        }
        Ok(b)
    };
    crate::verify_closure(&[root_id], limits, &mut bounded)?;
    let Object::Blob(blob) = Object::verify(barrier.manifest, &bounded(barrier.manifest)?)? else {
        return Err("frontier v1 requires ordinary blob".into());
    };
    let profile = CollaborationFrontier::from_bytes(&blob.data)?;
    if profile.workspace_hex != to_hex(&barrier.workspace)
        || profile.epoch_hex != to_hex(&barrier.epoch)
        || counter(&profile.generation)? != barrier.generation
        || profile.tree != barrier.tree.to_string()
        || profile.protocol_hex != to_hex(&barrier.protocol)
        || profile.namespace.len() != barrier.frontier.len()
        || profile
            .namespace
            .iter()
            .zip(&barrier.frontier)
            .any(|(a, (id, n))| a.replica_hex != to_hex(id) || a.sequence != n.to_string())
    {
        return Err("frontier context disagrees with barrier".into());
    }
    let mut stack = vec![(barrier.tree, Vec::<u8>::new())];
    let mut actual = Vec::new();
    let mut work = 0usize;
    let mut path_bytes = 0u64;
    while let Some((id, prefix)) = stack.pop() {
        let Object::Tree(tree) = Object::verify(id, &bounded(id)?)? else {
            return Err("expected tree".into());
        };
        for entry in tree.entries() {
            work = work.checked_add(1).ok_or("frontier path overflow")?;
            if work > limits.max_edges {
                return Err("frontier path limit exceeded".into());
            }
            let mut p = prefix.clone();
            if !p.is_empty() {
                p.push(b'/');
            }
            p.extend(&entry.name);
            path_bytes = path_bytes
                .checked_add(p.len() as u64)
                .ok_or("frontier path bytes overflow")?;
            if path_bytes > limits.max_bytes {
                return Err("frontier path byte limit exceeded".into());
            }
            let target = match &entry.target {
                Target::Native(id) => id.to_string(),
                Target::GitSha1(id) => format!("sha1:{}", to_hex(id)),
                Target::GitSha256(id) => format!("sha256:{}", to_hex(id)),
            };
            actual.push((to_hex(&p), entry.kind as u8, target));
            if let (EntryKind::Directory, Target::Native(child)) = (entry.kind, &entry.target) {
                stack.push((*child, p));
            }
        }
    }
    actual.sort();
    if actual.len() != profile.entries.len()
        || actual
            .iter()
            .zip(&profile.entries)
            .any(|((p, k, t), e)| p != &e.path_hex || k != &e.kind || t != &e.target)
    {
        return Err("frontier materialization disagrees with tree".into());
    }
    Ok(profile)
}
