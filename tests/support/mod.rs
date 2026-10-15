use kai_vcs::*;
pub fn id(byte: u8) -> ObjectId {
    ObjectId::from_digest([byte; 32])
}
pub fn person() -> Identity {
    Identity {
        name: b"A\xff".to_vec(),
        email: b"a@example.test".to_vec(),
        timestamp_seconds: -1,
        timezone_minutes: -90,
    }
}
fn state(kind: EntryKind, byte: u8) -> Option<FileState> {
    Some(FileState {
        kind,
        target: Target::Native(id(byte)),
    })
}
fn version(label: &[u8], states: Vec<Option<FileState>>, source: Option<u8>) -> ConflictVersion {
    ConflictVersion {
        label: label.to_vec(),
        states,
        source_revision: source.map(id),
    }
}
pub fn sample(name: &str) -> Object {
    if name.starts_with("transfer-") || name.starts_with("backup-") {
        let entry = |byte, type_code, interpretation| InventoryEntry {
            id: id(byte),
            type_code,
            framed_length: 100,
            interpretation,
        };
        let mut contents = PackageContents {
            completeness: PackageCompleteness::Standalone,
            roots: vec![],
            inventory: vec![],
            prerequisites: vec![],
            required_features: vec![b"native/v1".to_vec()],
            optional_features: vec![b"opaque/demo".to_vec()],
        };
        match name {
            "transfer-standalone" => {
                contents.roots = vec![PackageRoot {
                    id: id(0x11),
                    type_code: 3,
                }];
                contents.inventory = vec![
                    entry(0x11, 3, Interpretation::Interpreted),
                    entry(0x22, 500, Interpretation::Opaque),
                ];
            }
            "transfer-incremental" => {
                contents.completeness = PackageCompleteness::Incremental;
                contents.roots = vec![PackageRoot {
                    id: id(0x11),
                    type_code: 3,
                }];
                contents.inventory = vec![entry(0x11, 3, Interpretation::Interpreted)];
                contents.prerequisites = vec![entry(0x22, 2, Interpretation::Interpreted)];
            }
            "backup-initial" => {
                contents.inventory = vec![entry(0x11, 6, Interpretation::Interpreted)];
            }
            "backup-incremental" => {
                contents.completeness = PackageCompleteness::Incremental;
                contents.roots = vec![PackageRoot {
                    id: id(0x33),
                    type_code: 6,
                }];
                contents.inventory = vec![
                    entry(0x11, 6, Interpretation::Interpreted),
                    entry(0x22, 7, Interpretation::Interpreted),
                ];
                contents.prerequisites = vec![entry(0x33, 6, Interpretation::Interpreted)];
            }
            _ => panic!("unknown package sample"),
        }
        return if name.starts_with("transfer-") {
            Object::TransferManifest(Box::new(TransferManifest {
                repository: [1; 16],
                contents,
            }))
        } else {
            Object::BackupManifest(Box::new(BackupManifest {
                repository: [1; 16],
                generation: if name == "backup-initial" { 0 } else { 42 },
                current_view: id(0x11),
                current_operation: if name == "backup-initial" {
                    None
                } else {
                    Some(id(0x22))
                },
                contents,
            }))
        };
    }
    if name.starts_with("git-") {
        let algorithm = if name.ends_with("sha256") { 2 } else { 1 };
        let width = if algorithm == 1 { 20 } else { 32 };
        let oid = "22".repeat(width);
        let (kind, body) = if name.starts_with("git-blob-") {
            (GitKind::Blob, b"hello\0\xff\r\n".to_vec())
        } else if name.starts_with("git-tree-") {
            let mut b = b"100755 file\0".to_vec();
            b.extend(vec![0x11; width]);
            (GitKind::Tree, b)
        } else if name.starts_with("git-commit-") {
            (GitKind::Commit,format!("tree {oid}\nparent {}\nparent {}\nauthor A <a@example.test> 1 +0130\ncommitter B <b@example.test> 2 -0200\nencoding ISO-8859-1\ngpgsig -----BEGIN PGP SIGNATURE-----\n opaque preserved bytes\n -----END PGP SIGNATURE-----\n\nmessage\r\n","33".repeat(width),"44".repeat(width)).into_bytes())
        } else {
            (GitKind::Tag,format!("object {oid}\ntype commit\ntag v1\ntagger A <a@example.test> 1 +0000\n\nrelease\n-----BEGIN PGP SIGNATURE-----\nopaque\n-----END PGP SIGNATURE-----\n").into_bytes())
        };
        return Object::GitEnvelope(GitEnvelope::new(kind, algorithm, body).unwrap());
    }
    match name {
        "barrier-full" => Object::CollaborationBarrier(Box::new(CollaborationBarrier {
            workspace: [1; 16],
            epoch: [2; 16],
            generation: 42,
            tree: id(0x11),
            protocol: b"epoch/v1".to_vec(),
            frontier: vec![([3; 16], 0), ([4; 16], u64::MAX)],
            excluded: vec![([5; 16], b"offline".to_vec())],
            manifest: id(0x22),
            manifest_format: b"namespace/v1".to_vec(),
        })),
        "policy-full" => Object::Policy(Box::new(Policy {
            repository: [1; 16],
            revision: 2,
            protected_refs: vec![b"branches/main".to_vec(), b"tags/v1".to_vec()],
            required_checks: vec![b"build".to_vec(), b"test".to_vec()],
            required_reviewers: 2,
            allowed_principals: vec![b"key:a".to_vec()],
            selective_ci: SelectiveCi::FullOnUnknown,
            rewrite: RewritePolicy::Deny,
            required_features: vec![b"acceptance/v1".to_vec()],
            predecessor: Some(id(0x33)),
        })),
        "evidence-execution" => Object::Evidence(Box::new(Evidence {
            revision: id(0x11),
            tree: id(0x22),
            source_state: SourceState::Resolved,
            kind: EvidenceKind::Execution,
            invocation: vec![b"test".to_vec(), vec![], b"--all".to_vec()],
            cwd: b"src".to_vec(),
            inputs: MetadataManifest {
                format: b"inputs/v1".to_vec(),
                object: id(0x33),
            },
            environment: MetadataManifest {
                format: b"environment/v1".to_vec(),
                object: id(0x44),
            },
            producer: person(),
            claimed_trust: ClaimedTrust::SelfReported,
            result: EvidenceResult::Pass,
            started_seconds: 100,
            finished_seconds: 99,
            artifacts: vec![(b"stdout".to_vec(), id(0x55))],
            scope: EvidenceScope {
                covered: vec![b"unit".to_vec()],
                excluded: vec![b"network".to_vec()],
                selection: b"all/v1".to_vec(),
                fallback_reason: vec![],
            },
            limitations: vec![b"external service unavailable".to_vec()],
            exit_status: Some(0),
        })),
        "attestation-full" => Object::Attestation(Box::new(Attestation {
            format: b"sshsig".to_vec(),
            version: 1,
            domain: b"kai-vcs".to_vec(),
            subject_type: 3,
            subject: id(0x11),
            purpose: b"revision".to_vec(),
            signer: b"test-key".to_vec(),
            claimed_identity: person(),
            signature: b"opaque-signature\0\xff".to_vec(),
            issued_seconds: Some(-1),
            expires_seconds: Some(100),
        })),
        "analysis-partial" => Object::AnalysisManifest(Box::new(AnalysisManifest {
            tree: id(0x22),
            analyzer: b"analyzer/1".to_vec(),
            resolver: b"resolver/1".to_vec(),
            dependency_digest: [0x33; 32],
            build_digest: [0x44; 32],
            platform: b"linux-arm64".to_vec(),
            completion: AnalysisCompletion::Partial,
            covered: vec![b"src/a".to_vec()],
            unsupported: vec![b"src/generated".to_vec()],
            parser_failures: vec![b"src/b".to_vec()],
            unresolved: vec![b"dynamic-call".to_vec()],
            results: vec![id(0x55), id(0x66)],
            reason: b"unsupported inputs remain".to_vec(),
        })),
        "view-empty" => Object::View(Box::default()),
        "view-full" => Object::View(Box::new(View {
            namespace_generation: 24,
            refs: vec![
                ViewRef {
                    name: "branches/deleted".into(),
                    generation: 3,
                    state: RefState::Deleted,
                },
                ViewRef {
                    name: "branches/é".into(),
                    generation: 7,
                    state: RefState::DraftConflict(vec![id(0x11), id(0x22)]),
                },
                ViewRef {
                    name: "remotes/origin/branches/main".into(),
                    generation: 1,
                    state: RefState::Target(id(0x33)),
                },
                ViewRef {
                    name: "shelves/wip".into(),
                    generation: 2,
                    state: RefState::Target(id(0x44)),
                },
                ViewRef {
                    name: "tags/v1".into(),
                    generation: u64::MAX,
                    state: RefState::Target(id(0x55)),
                },
            ],
            drafts: vec![DraftHeads {
                change_id: [1; 16],
                heads: vec![id(0x11), id(0x22)],
            }],
            workspaces: vec![
                WorkspaceRoot {
                    workspace: [2; 16],
                    base: Some(id(0x33)),
                    checkpoint: Some(id(0x44)),
                },
                WorkspaceRoot {
                    workspace: [3; 16],
                    base: None,
                    checkpoint: None,
                },
            ],
            retained: vec![
                RetainedRoot {
                    type_code: 6,
                    id: id(0x66),
                },
                RetainedRoot {
                    type_code: 7,
                    id: id(0x77),
                },
                RetainedRoot {
                    type_code: 9,
                    id: id(0x88),
                },
            ],
        })),
        "conflict-delete" => Object::Conflict(Box::new(Conflict {
            kind: ConflictKind::DeleteModify,
            paths: vec![b"src/a".to_vec()],
            bases: vec![version(
                b"base",
                vec![state(EntryKind::Regular, 0x11)],
                Some(0x22),
            )],
            sides: vec![
                version(b"ours", vec![None], None),
                version(b"theirs", vec![state(EntryKind::Executable, 0x33)], None),
            ],
            algorithm: b"diff3/v1".to_vec(),
            policy: b"conservative/v1".to_vec(),
            renames: vec![],
            resolution_provenance: vec![],
        })),
        "conflict-rename" => Object::Conflict(Box::new(Conflict {
            kind: ConflictKind::Rename,
            paths: vec![b"a".to_vec(), b"b".to_vec()],
            bases: vec![version(
                b"base",
                vec![state(EntryKind::Regular, 0x11), None],
                None,
            )],
            sides: vec![
                version(b"ours", vec![None, state(EntryKind::Regular, 0x11)], None),
                version(b"theirs", vec![state(EntryKind::Regular, 0x33), None], None),
            ],
            algorithm: b"rename/v1".to_vec(),
            policy: b"conservative/v1".to_vec(),
            renames: vec![RenameCandidate {
                identity: [1; 16],
                source: b"a".to_vec(),
                target: b"b".to_vec(),
            }],
            resolution_provenance: vec![id(0x44)],
        })),
        "checkpoint-full" => Object::Checkpoint(Box::new(Checkpoint {
            workspace: [1; 16],
            tree: id(0x22),
            selected_tree: id(0x33),
            capture: CaptureClass::BarrierConsistent,
            actor: person(),
            inclusion: vec![
                (b"ignored".to_vec(), InclusionKind::Ignored),
                (b"untracked".to_vec(), InclusionKind::Untracked),
            ],
            base: Some(id(0x44)),
            conflicts: vec![(b"src/a".to_vec(), id(0x55))],
            predecessor: Some(id(0x66)),
            barrier: Some(id(0x77)),
            selected_conflicts: vec![(b"selected".to_vec(), id(0x88))],
        })),
        "operation-full" => Object::Operation(Box::new(Operation {
            operation_id: [1; 16],
            predecessors: vec![id(0x11), id(0x22)],
            pre_view: id(0x33),
            post_view: id(0x44),
            action: b"checkpoint".to_vec(),
            actor: person(),
            started_seconds: 100,
            finished_seconds: 99,
            workspaces: vec![[1; 16], [2; 16]],
            intent_id: [3; 16],
            external_receipts: vec![b"peer:receipt/1".to_vec()],
        })),
        "change-rewrite" => Object::ChangeEvent(Box::new(ChangeEvent {
            kind: ChangeKind::Rewrite,
            change_ids: vec![[1; 16]],
            prior: vec![id(0x11)],
            resulting: vec![id(0x22)],
            actor: person(),
            reason: b"amend\0\xff".to_vec(),
            operation: Some(id(0x33)),
        })),
        "tag-full" => Object::Tag(Tag {
            target: id(0x11),
            message: b"release\0\xff".to_vec(),
            tagger: person(),
            git_envelope: Some(id(0x22)),
        }),
        _ => panic!("unknown fixture {name}"),
    }
}
