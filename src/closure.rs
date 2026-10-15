//! Typed, iterative source-closure verification. This is preparation, not a
//! repository transaction or a promise that a concurrently mutable store is safe.
use crate::{ClosureClass, ConflictManifest, EntryKind, Object, ObjectId, Target, Tree};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, Debug)]
pub struct ClosureLimits {
    pub max_objects: usize,
    pub max_bytes: u64,
    pub max_edges: usize,
}
impl Default for ClosureLimits {
    fn default() -> Self {
        Self {
            max_objects: 100_000,
            max_bytes: 1024 * 1024 * 1024,
            max_edges: 1_000_000,
        }
    }
}

#[derive(Debug)]
pub struct VerifiedClosure {
    pub objects: usize,
    pub bytes: u64,
}

enum Visit {
    Enter(ObjectId, &'static [u16], bool),
    Leave(ObjectId),
}

/// Verify canonical hashes, required edge types, nonempty directory targets,
/// and chunk assembly. Retained metadata/external references are excluded.
/// The provider must supply immutable, allocation-bounded objects under a lease.
pub fn verify_closure(
    roots: &[ObjectId],
    limits: ClosureLimits,
    mut read: impl FnMut(ObjectId) -> Result<Vec<u8>, String>,
) -> Result<VerifiedClosure, String> {
    if roots.len() > limits.max_edges {
        return Err("closure root limit exceeded".into());
    }
    let mut stack: Vec<_> = roots
        .iter()
        .map(|id| Visit::Enter(*id, &[], false))
        .collect();
    let mut seen = HashMap::<ObjectId, (u16, bool)>::new();
    let mut active = HashSet::new();
    let mut total = 0u64;
    let mut edges = roots.len();
    let mut trees = HashMap::new();
    let mut conflicts = HashMap::new();
    let mut manifests = Vec::new();
    let mut revisions = HashMap::new();
    let mut checkpoints = HashMap::new();
    let mut views = Vec::new();
    let mut evidence = Vec::new();
    let mut revision_sources = HashMap::new();
    let mut git_envelopes = HashMap::new();
    let mut git_links = Vec::new();
    let mut barriers = HashMap::new();
    let mut barrier_links = Vec::new();
    while let Some(visit) = stack.pop() {
        let (id, expected, directory) = match visit {
            Visit::Leave(id) => {
                active.remove(&id);
                continue;
            }
            Visit::Enter(id, expected, directory) => (id, expected, directory),
        };
        if active.contains(&id) {
            return Err(format!("required closure cycle at {id}"));
        }
        if let Some((kind, empty)) = seen.get(&id) {
            check_target(id, *kind, *empty, expected, directory)?;
            continue;
        }
        if seen.len() >= limits.max_objects {
            return Err("closure object limit exceeded".into());
        }
        let bytes = read(id).map_err(|e| format!("missing/unavailable object {id}: {e}"))?;
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or("closure byte overflow")?;
        if total > limits.max_bytes {
            return Err("closure byte limit exceeded".into());
        }
        let object = Object::verify(id, &bytes).map_err(|e| format!("invalid object {id}: {e}"))?;
        let empty = matches!(&object, Object::Tree(t) if t.entries().is_empty());
        check_target(id, object.type_code(), empty, expected, directory)?;
        if let Object::LargeBlob(manifest) = &object {
            // Count rereads in the IO budget: repeated chunk IDs cannot force
            // unlimited hashing work while appearing to be one distinct object.
            manifest.verify_content(|id| {
                let chunk = read(id)?;
                total = total
                    .checked_add(chunk.len() as u64)
                    .ok_or("closure byte overflow")?;
                if total > limits.max_bytes {
                    return Err("closure byte limit exceeded".into());
                }
                Ok(chunk)
            })?;
        }
        let refs = object.references();
        let source_type = object.type_code();
        edges = edges
            .checked_add(refs.len())
            .ok_or("closure edge overflow")?;
        if edges > limits.max_edges {
            return Err("closure edge limit exceeded".into());
        }
        seen.insert(id, (object.type_code(), empty));
        active.insert(id);
        stack.push(Visit::Leave(id));
        for reference in refs {
            if reference.class == ClosureClass::Required {
                let directory = matches!(source_type, 2 | 5) && reference.expected_types == [2];
                stack.push(Visit::Enter(
                    reference.id,
                    reference.expected_types,
                    directory,
                ));
            }
        }
        match object {
            Object::Tree(tree) => {
                trees.insert(id, tree);
            }
            Object::Conflict(conflict) => {
                conflicts.insert(id, conflict.paths);
            }
            Object::GitEnvelope(v) => {
                git_envelopes.insert(id, v.kind);
            }
            Object::Tag(v) => {
                if let Some(id) = v.git_envelope {
                    git_links.push((id, crate::GitKind::Tag));
                }
            }
            Object::CollaborationBarrier(v) => {
                barriers.insert(id, (v.workspace, v.tree));
            }
            Object::View(view) => views.push(view),
            Object::Evidence(value) => evidence.push(value),
            Object::Revision(revision) => {
                if let Some(id) = revision.git_envelope {
                    if !revision.conflicts.is_empty() {
                        return Err("conflicted revision cannot carry a Git commit envelope".into());
                    }
                    git_links.push((id, crate::GitKind::Commit));
                }
                revisions.insert(id, revision.change_id);
                revision_sources.insert(id, (revision.tree, !revision.conflicts.is_empty()));
                if !revision.conflicts.is_empty() {
                    manifests.push((revision.tree, revision.conflicts));
                }
            }
            Object::Checkpoint(checkpoint) => {
                checkpoints.insert(id, checkpoint.workspace);
                if let Some(barrier) = checkpoint.barrier {
                    barrier_links.push((barrier, checkpoint.workspace, checkpoint.tree));
                }
                if !checkpoint.conflicts.is_empty() {
                    manifests.push((checkpoint.tree, checkpoint.conflicts));
                }
                if !checkpoint.selected_conflicts.is_empty() {
                    manifests.push((checkpoint.selected_tree, checkpoint.selected_conflicts));
                }
            }
            _ => {}
        }
    }
    for (id, workspace, tree) in barrier_links {
        if barriers.get(&id) != Some(&(workspace, tree)) {
            return Err("checkpoint/barrier workspace or tree mismatch".into());
        }
    }
    for (id, kind) in git_links {
        if git_envelopes.get(&id) != Some(&kind) {
            return Err("Git envelope kind does not match native owner".into());
        }
    }
    for value in evidence {
        let Some((tree, conflicted)) = revision_sources.get(&value.revision) else {
            return Err("evidence revision unavailable".into());
        };
        if *tree != value.tree
            || matches!(
                (value.source_state, *conflicted),
                (crate::SourceState::Resolved, true) | (crate::SourceState::Conflicted, false)
            )
        {
            return Err("evidence source claim contradicts revision".into());
        }
    }
    for view in views {
        for draft in &view.drafts {
            for head in &draft.heads {
                if revisions.get(head) != Some(&draft.change_id) {
                    return Err("draft head change ID mismatch".into());
                }
            }
        }
        for workspace in &view.workspaces {
            if let Some(checkpoint) = workspace.checkpoint
                && checkpoints.get(&checkpoint) != Some(&workspace.workspace)
            {
                return Err("checkpoint workspace ID mismatch".into());
            }
        }
    }
    for (tree, manifest) in manifests {
        validate_manifest(
            tree,
            &manifest,
            &trees,
            &conflicts,
            &mut edges,
            limits.max_edges,
        )?;
    }
    Ok(VerifiedClosure {
        objects: seen.len(),
        bytes: total,
    })
}

fn validate_manifest(
    tree: ObjectId,
    manifest: &ConflictManifest,
    trees: &HashMap<ObjectId, Tree>,
    conflicts: &HashMap<ObjectId, Vec<Vec<u8>>>,
    steps: &mut usize,
    limit: usize,
) -> Result<(), String> {
    let mut bound = BTreeMap::<ObjectId, Vec<&[u8]>>::new();
    for (path, id) in manifest {
        bound.entry(*id).or_default().push(path);
    }
    for (id, paths) in bound {
        let declared = conflicts
            .get(&id)
            .ok_or_else(|| format!("missing conflict definition {id}"))?;
        if paths.len() != declared.len() || !paths.iter().zip(declared).all(|(a, b)| *a == b) {
            return Err(format!("conflict manifest/path disagreement at {id}"));
        }
    }
    for (path, _) in manifest {
        let mut tree_id = tree;
        let mut components = path.split(|b| *b == b'/').peekable();
        while let Some(component) = components.next() {
            *steps = steps.checked_add(1).ok_or("closure work overflow")?;
            if *steps > limit {
                return Err("closure path work limit exceeded".into());
            }
            let current = trees
                .get(&tree_id)
                .ok_or_else(|| format!("missing tree index {tree_id}"))?;
            let Ok(index) = current
                .entries()
                .binary_search_by(|entry| entry.name.as_slice().cmp(component))
            else {
                break;
            };
            let entry = &current.entries()[index];
            if components.peek().is_none() {
                return Err(format!(
                    "unresolved path also exists in ordinary tree: {path:?}"
                ));
            }
            if entry.kind != EntryKind::Directory {
                return Err(format!(
                    "unresolved path has a non-directory ancestor: {path:?}"
                ));
            }
            let Target::Native(child) = entry.target else {
                return Err("directory has external target".into());
            };
            tree_id = child;
        }
    }
    Ok(())
}

fn check_target(
    id: ObjectId,
    kind: u16,
    empty: bool,
    expected: &[u16],
    directory: bool,
) -> Result<(), String> {
    if !expected.is_empty() && !expected.contains(&kind) {
        return Err(format!(
            "wrong object type at {id}: got {kind}, expected {expected:?}"
        ));
    }
    if directory && empty {
        return Err(format!("empty directory target at {id}"));
    }
    Ok(())
}
