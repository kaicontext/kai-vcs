use crate::{
    Blob, ChangeEvent, Checkpoint, Conflict, LargeBlob, ObjectId, Operation, Revision, Tag, Tree,
    codec,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Object {
    View(Box<crate::View>),
    PartitionedView(Box<crate::PartitionedView>),
    Evidence(Box<crate::Evidence>),
    Attestation(Box<crate::Attestation>),
    AnalysisManifest(Box<crate::AnalysisManifest>),
    GitEnvelope(crate::GitEnvelope),
    BackupManifest(Box<crate::BackupManifest>),
    TransferManifest(Box<crate::TransferManifest>),
    CollaborationBarrier(Box<crate::CollaborationBarrier>),
    Policy(Box<crate::Policy>),
    Blob(Blob),
    Tree(Tree),
    Revision(Box<Revision>),
    Conflict(Box<Conflict>),
    Checkpoint(Box<Checkpoint>),
    Operation(Box<Operation>),
    ChangeEvent(Box<ChangeEvent>),
    Tag(Tag),
    Chunk(Vec<u8>),
    LargeBlob(LargeBlob),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosureClass {
    Required,
    RetainedMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub id: ObjectId,
    pub class: ClosureClass,
    pub expected_types: &'static [u16],
}

impl Object {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        match self {
            Self::View(view) => view.encode(),
            Self::PartitionedView(view) => view.encode(),
            Self::Evidence(v) => v.encode(),
            Self::Attestation(v) => v.encode(),
            Self::AnalysisManifest(v) => v.encode(),
            Self::GitEnvelope(v) => v.encode(),
            Self::BackupManifest(v) => v.encode(),
            Self::TransferManifest(v) => v.encode(),
            Self::CollaborationBarrier(v) => v.encode(),
            Self::Policy(v) => v.encode(),
            Self::Blob(blob) => blob.encode(),
            Self::Tree(tree) => tree.encode(),
            Self::Revision(revision) => revision.encode(),
            Self::Conflict(value) => value.encode(),
            Self::Checkpoint(value) => value.encode(),
            Self::Operation(value) => value.encode(),
            Self::ChangeEvent(value) => value.encode(),
            Self::Tag(value) => value.encode(),
            Self::Chunk(data) => {
                if data.is_empty() || data.len() > codec::CHUNK_SIZE {
                    return Err("chunk length must be 1 byte to 4 MiB".into());
                }
                Ok(codec::frame(13, data))
            }
            Self::LargeBlob(manifest) => manifest.encode(),
        }
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (kind, payload) = codec::unframe(input)?;
        match kind {
            16 => Ok(Self::BackupManifest(Box::new(
                crate::BackupManifest::decode(input)?,
            ))),
            19 => Ok(Self::TransferManifest(Box::new(
                crate::TransferManifest::decode(input)?,
            ))),
            17 => Ok(Self::CollaborationBarrier(Box::new(
                crate::CollaborationBarrier::decode(input)?,
            ))),
            18 => Ok(Self::Policy(Box::new(crate::Policy::decode(input)?))),
            12 => Ok(Self::GitEnvelope(crate::GitEnvelope::decode(input)?)),
            9 => Ok(Self::Evidence(Box::new(crate::Evidence::decode(input)?))),
            10 => Ok(Self::Attestation(Box::new(crate::Attestation::decode(
                input,
            )?))),
            11 => Ok(Self::AnalysisManifest(Box::new(
                crate::AnalysisManifest::decode(input)?,
            ))),
            6 if payload.first() == Some(&0xa3) => Ok(Self::PartitionedView(Box::new(
                crate::PartitionedView::decode(input)?,
            ))),
            6 => Ok(Self::View(Box::new(crate::View::decode(input)?))),
            1 => Ok(Self::Blob(Blob::decode(input)?)),
            2 => Ok(Self::Tree(Tree::decode(input)?)),
            3 => Ok(Self::Revision(Box::new(Revision::decode(input)?))),
            4 => Ok(Self::ChangeEvent(Box::new(ChangeEvent::decode(input)?))),
            5 => Ok(Self::Conflict(Box::new(Conflict::decode(input)?))),
            7 => Ok(Self::Operation(Box::new(Operation::decode(input)?))),
            8 => Ok(Self::Checkpoint(Box::new(Checkpoint::decode(input)?))),
            15 => Ok(Self::Tag(Tag::decode(input)?)),
            13 if !payload.is_empty() => Ok(Self::Chunk(payload.to_vec())),
            14 => Ok(Self::LargeBlob(LargeBlob::decode(input)?)),
            _ => Err("unsupported object type or empty chunk".into()),
        }
    }
    /// Verify the claimed ID before trusting any decoded fields.
    pub fn verify(id: ObjectId, input: &[u8]) -> Result<Self, String> {
        // Bound the input before spending CPU hashing untrusted payloads.
        codec::unframe(input)?;
        if ObjectId::hash(input) != id {
            return Err("object ID mismatch".into());
        }
        Self::decode(input)
    }
    pub fn type_code(&self) -> u16 {
        match self {
            Self::View(_) | Self::PartitionedView(_) => 6,
            Self::Evidence(_) => 9,
            Self::Attestation(_) => 10,
            Self::AnalysisManifest(_) => 11,
            Self::GitEnvelope(_) => 12,
            Self::BackupManifest(_) => 16,
            Self::TransferManifest(_) => 19,
            Self::CollaborationBarrier(_) => 17,
            Self::Policy(_) => 18,
            Self::Blob(_) => 1,
            Self::Tree(_) => 2,
            Self::Revision(_) => 3,
            Self::ChangeEvent(_) => 4,
            Self::Conflict(_) => 5,
            Self::Operation(_) => 7,
            Self::Checkpoint(_) => 8,
            Self::Tag(_) => 15,
            Self::Chunk(_) => 13,
            Self::LargeBlob(_) => 14,
        }
    }
    /// Native outgoing references only. Gitlinks remain typed external targets
    /// on the Tree and do not imply available native closure.
    pub fn references(&self) -> Vec<Reference> {
        use crate::{EntryKind, Target};
        use ClosureClass::{Required, RetainedMetadata};
        let mut refs = Vec::new();
        let mut add = |id, class, expected_types| {
            refs.push(Reference {
                id,
                class,
                expected_types,
            })
        };
        match self {
            Self::BackupManifest(v) => {
                add(v.current_view, Required, &[6]);
                if let Some(id) = v.current_operation {
                    add(id, Required, &[7]);
                }
                for r in &v.contents.roots {
                    add(r.id, Required, crate::view::pin_types(r.type_code));
                }
            }
            Self::TransferManifest(v) => {
                for r in &v.contents.roots {
                    add(r.id, Required, crate::view::pin_types(r.type_code));
                }
            }
            Self::CollaborationBarrier(v) => {
                add(v.tree, Required, &[2]);
                add(v.manifest, Required, &[1, 14]);
            }
            Self::Policy(v) => {
                if let Some(id) = v.predecessor {
                    add(id, RetainedMetadata, &[18]);
                }
            }
            Self::Evidence(v) => {
                add(v.revision, Required, &[3]);
                add(v.tree, Required, &[2]);
                add(v.inputs.object, Required, &[1, 14]);
                add(v.environment.object, Required, &[1, 14]);
                for (_, id) in &v.artifacts {
                    add(*id, RetainedMetadata, &[1, 14]);
                }
            }
            Self::Attestation(v) => {
                add(v.subject, Required, crate::view::pin_types(v.subject_type))
            }
            Self::AnalysisManifest(v) => {
                add(v.tree, Required, &[2]);
                for id in &v.results {
                    add(*id, RetainedMetadata, &[1, 14]);
                }
            }
            Self::PartitionedView(view) => {
                for (_, id) in &view.submaps {
                    add(*id, Required, &[6]);
                }
            }
            Self::View(view) => {
                for r in &view.refs {
                    let types = crate::view::ref_types(&r.name).unwrap_or(&[]);
                    match &r.state {
                        crate::RefState::Deleted => {}
                        crate::RefState::Target(id) => add(*id, Required, types),
                        crate::RefState::DraftConflict(ids) => {
                            for id in ids {
                                add(*id, Required, types);
                            }
                        }
                    }
                }
                for d in &view.drafts {
                    for id in &d.heads {
                        add(*id, Required, &[3]);
                    }
                }
                for w in &view.workspaces {
                    if let Some(id) = w.base {
                        add(id, Required, &[3]);
                    }
                    if let Some(id) = w.checkpoint {
                        add(id, Required, &[8]);
                    }
                }
                for p in &view.retained {
                    add(p.id, Required, crate::view::pin_types(p.type_code));
                }
            }
            Self::Tree(tree) => {
                for entry in tree.entries() {
                    if let Target::Native(id) = entry.target {
                        let expected: &'static [u16] = match entry.kind {
                            EntryKind::Regular | EntryKind::Executable => &[1, 14],
                            EntryKind::Symlink => &[1],
                            EntryKind::Directory => &[2],
                            EntryKind::Gitlink => &[],
                        };
                        add(id, Required, expected);
                    }
                }
            }
            Self::Revision(r) => {
                add(r.tree, Required, &[2]);
                for id in &r.parents {
                    add(*id, Required, &[3]);
                }
                for id in &r.provenance {
                    add(*id, RetainedMetadata, &[]);
                }
                if let Some(id) = r.git_envelope {
                    add(id, Required, &[12]);
                }
                for (_, id) in &r.conflicts {
                    add(*id, Required, &[5]);
                }
            }
            Self::LargeBlob(blob) => {
                for id in &blob.chunks {
                    add(*id, Required, &[13]);
                }
            }
            Self::Conflict(conflict) => {
                for version in conflict.bases.iter().chain(&conflict.sides) {
                    if let Some(id) = version.source_revision {
                        add(id, Required, &[3]);
                    }
                    for state in version.states.iter().flatten() {
                        if let Target::Native(id) = state.target {
                            let expected: &'static [u16] = match state.kind {
                                EntryKind::Regular | EntryKind::Executable => &[1, 14],
                                EntryKind::Symlink => &[1],
                                EntryKind::Directory => &[2],
                                EntryKind::Gitlink => &[],
                            };
                            add(id, Required, expected);
                        }
                    }
                }
                for id in &conflict.resolution_provenance {
                    add(*id, RetainedMetadata, &[]);
                }
            }
            Self::Checkpoint(c) => {
                add(c.tree, Required, &[2]);
                add(c.selected_tree, Required, &[2]);
                if let Some(id) = c.base {
                    add(id, Required, &[3]);
                }
                for (_, id) in c.conflicts.iter().chain(&c.selected_conflicts) {
                    add(*id, Required, &[5]);
                }
                if let Some(id) = c.predecessor {
                    add(id, RetainedMetadata, &[8]);
                }
                if let Some(id) = c.barrier {
                    add(id, Required, &[17]);
                }
            }
            Self::Operation(op) => {
                for id in &op.predecessors {
                    add(*id, RetainedMetadata, &[7]);
                }
                add(op.pre_view, RetainedMetadata, &[6]);
                add(op.post_view, RetainedMetadata, &[6]);
            }
            Self::ChangeEvent(event) => {
                for id in event.prior.iter().chain(&event.resulting) {
                    add(*id, RetainedMetadata, &[3]);
                }
                if let Some(id) = event.operation {
                    add(id, RetainedMetadata, &[7]);
                }
            }
            Self::Tag(tag) => {
                add(tag.target, Required, &[3]);
                if let Some(id) = tag.git_envelope {
                    add(id, Required, &[12]);
                }
            }
            _ => {}
        }
        refs
    }
}
