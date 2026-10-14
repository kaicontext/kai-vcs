use kai_object_reference::bundle::Bundle;
use kai_vcs::*;
use std::collections::BTreeMap;
fn person() -> Identity {
    Identity {
        name: b"A".to_vec(),
        email: b"a@example.test".to_vec(),
        timestamp_seconds: 0,
        timezone_minutes: 0,
    }
}
type Store = BTreeMap<ObjectId, Vec<u8>>;
fn put(s: &mut Store, o: Object) -> ObjectId {
    let b = o.encode().unwrap();
    let id = ObjectId::hash(&b);
    s.insert(id, b);
    id
}
fn package(s: &Store, root: ObjectId) -> TransferManifest {
    TransferManifest {
        repository: [1; 16],
        contents: PackageContents {
            completeness: PackageCompleteness::Standalone,
            roots: vec![PackageRoot {
                id: root,
                type_code: Object::decode(&s[&root]).unwrap().type_code(),
            }],
            inventory: s
                .iter()
                .map(|(id, b)| InventoryEntry {
                    id: *id,
                    type_code: Object::decode(b).unwrap().type_code(),
                    framed_length: b.len() as u64,
                    interpretation: Interpretation::Interpreted,
                })
                .collect(),
            prerequisites: vec![],
            required_features: vec![],
            optional_features: vec![],
        },
    }
}
fn container(m: &TransferManifest, s: &Store) -> Vec<u8> {
    let mut b = b"KAI-BUNDLE\0\0\x01".to_vec();
    for data in std::iter::once(m.encode().unwrap())
        .chain(m.contents.inventory.iter().map(|e| s[&e.id].clone()))
    {
        b.extend((data.len() as u64).to_be_bytes());
        b.extend(data);
    }
    b
}
fn tree(s: &mut Store, entries: Vec<(&[u8], EntryKind, ObjectId)>) -> ObjectId {
    put(
        s,
        Object::Tree(
            Tree::new(
                entries
                    .into_iter()
                    .map(|(name, kind, id)| TreeEntry {
                        name: name.to_vec(),
                        kind,
                        target: Target::Native(id),
                    })
                    .collect(),
            )
            .unwrap(),
        ),
    )
}
fn setup() -> (Store, ObjectId) {
    let mut s = Store::new();
    let blob = put(
        &mut s,
        Object::Blob(Blob {
            data: vec![0, 255, 13, 10],
        }),
    );
    let link = put(
        &mut s,
        Object::Blob(Blob {
            data: b"/outside/dangling".to_vec(),
        }),
    );
    let sub = tree(&mut s, vec![(b"binary", EntryKind::Executable, blob)]);
    let root = tree(
        &mut s,
        vec![
            (b"a-link", EntryKind::Symlink, link),
            (b"dir", EntryKind::Directory, sub),
            (b"file", EntryKind::Regular, blob),
        ],
    );
    (s, root)
}
fn archive(s: &Store, root: ObjectId) -> Result<Vec<u8>, String> {
    let bytes = container(&package(s, root), s);
    let b = Bundle::read(bytes.as_slice())?;
    let mut out = Vec::new();
    b.archive(&root.to_string())?.write(&mut out)?;
    Ok(out)
}
#[test]
fn independent_archive_preserves_bytes_modes_and_emits_symlinks_last() {
    let (s, root) = setup();
    let bytes = container(&package(&s, root), &s);
    let b = Bundle::read(bytes.as_slice()).unwrap();
    assert_eq!(b.roots(), vec![root.to_string()]);
    let tar = archive(&s, root).unwrap();
    assert_eq!(&tar[..3], b"dir");
    assert_eq!(tar[156], b'5');
    assert_eq!(&tar[512..522], b"dir/binary");
    assert_eq!(&tar[612..619], b"0000755");
    assert_eq!(&tar[1024..1028], &[0, 255, 13, 10]);
    assert_eq!(&tar[1536..1540], b"file");
    assert_eq!(&tar[1636..1643], b"0000644");
    assert_eq!(&tar[2560..2566], b"a-link");
    assert_eq!(tar[2716], b'2');
    assert_eq!(&tar[2717..2735], b"/outside/dangling\0");
    assert!(b.archive(&format!("k1:b3:{}", "00".repeat(32))).is_err());
}
#[test]
fn rejects_corruption_truncation_trailing_data_and_wrong_magic() {
    let (s, root) = setup();
    let valid = container(&package(&s, root), &s);
    for pos in [0, 12, valid.len() - 1] {
        let mut b = valid.clone();
        b[pos] ^= 1;
        assert!(Bundle::read(b.as_slice()).is_err());
    }
    for n in [0, 12, 20, valid.len() - 1] {
        assert!(Bundle::read(&valid[..n]).is_err());
    }
    let mut b = valid;
    b.push(0);
    assert!(Bundle::read(b.as_slice()).is_err());
}
#[test]
fn rejects_missing_mistyped_or_opaque_required_objects() {
    let (s, root) = setup();
    let base = package(&s, root);
    for case in 0..3 {
        let mut m = base.clone();
        let i = m
            .contents
            .inventory
            .iter()
            .position(|e| e.type_code == 1)
            .unwrap();
        match case {
            0 => {
                m.contents.inventory.remove(i);
            }
            1 => m.contents.inventory[i].type_code = 13,
            _ => m.contents.inventory[i].interpretation = Interpretation::Opaque,
        }
        assert!(Bundle::read(container(&m, &s).as_slice()).is_err());
    }
}
#[test]
fn refuses_incremental_and_unknown_required_features() {
    let (s, root) = setup();
    let mut m = package(&s, root);
    m.contents.required_features.push(b"unknown".to_vec());
    assert!(Bundle::read(container(&m, &s).as_slice()).is_err());
    m.contents.required_features.clear();
    m.contents.completeness = PackageCompleteness::Incremental;
    let i = m
        .contents
        .inventory
        .iter()
        .position(|e| e.type_code == 1)
        .unwrap();
    let e = m.contents.inventory.remove(i);
    m.contents.prerequisites.push(e);
    assert!(Bundle::read(container(&m, &s).as_slice()).is_err());
}
#[test]
fn checks_complete_revision_ancestry_and_tag_selection() {
    let (mut s, tree) = setup();
    let mut revision = Revision {
        tree,
        parents: vec![],
        change_id: [1; 16],
        author: person(),
        committer: person(),
        message: b"parent".to_vec(),
        provenance: vec![],
        git_envelope: None,
        conflicts: vec![],
    };
    let parent = put(&mut s, Object::Revision(Box::new(revision.clone())));
    revision.parents.push(parent);
    revision.message = b"child".to_vec();
    let child = put(&mut s, Object::Revision(Box::new(revision)));
    let tag = put(
        &mut s,
        Object::Tag(Tag {
            target: child,
            message: b"tag".to_vec(),
            tagger: person(),
            git_envelope: None,
        }),
    );
    assert_eq!(archive(&s, tag).unwrap(), archive(&s, tree).unwrap());
    s.remove(&parent);
    assert!(archive(&s, tag).unwrap_err().contains("missing"));
}
#[test]
fn preflight_rejects_aliases_collisions_gitlinks_and_invalid_symlinks() {
    for name in [
        b".git".as_slice(),
        b".KAI",
        b"CON.txt",
        b"file:stream",
        b"trailing.",
        b"back\\slash",
    ] {
        let mut s = Store::new();
        let blob = put(&mut s, Object::Blob(Blob { data: vec![] }));
        let root = tree(&mut s, vec![(name, EntryKind::Regular, blob)]);
        assert!(archive(&s, root).is_err());
    }
    let mut s = Store::new();
    let blob = put(&mut s, Object::Blob(Blob { data: vec![] }));
    let root = tree(
        &mut s,
        vec![
            (b"A", EntryKind::Regular, blob),
            (b"a", EntryKind::Regular, blob),
        ],
    );
    assert!(archive(&s, root).unwrap_err().contains("collision"));
    let root = tree(&mut s, vec![(b"link", EntryKind::Symlink, blob)]);
    assert!(archive(&s, root).is_err());
    let root = put(
        &mut s,
        Object::Tree(
            Tree::new(vec![TreeEntry {
                name: b"sub".to_vec(),
                kind: EntryKind::Gitlink,
                target: Target::GitSha256([3; 32]),
            }])
            .unwrap(),
        ),
    );
    let b = Bundle::read(container(&package(&s, root), &s).as_slice()).unwrap();
    assert!(b.archive(&root.to_string()).is_err());
}
#[test]
fn raw_names_long_paths_and_long_link_extensions_are_lossless() {
    let mut s = Store::new();
    let blob = put(&mut s, Object::Blob(Blob { data: vec![8] }));
    let link = put(
        &mut s,
        Object::Blob(Blob {
            data: vec![b'x'; 101],
        }),
    );
    let name = vec![b'a'; 101];
    let root = tree(
        &mut s,
        vec![
            (&name, EntryKind::Regular, blob),
            (b"link", EntryKind::Symlink, link),
            (&[255], EntryKind::Regular, blob),
        ],
    );
    let tar = archive(&s, root).unwrap();
    assert_eq!(tar[156], b'L');
    assert_eq!(&tar[512..613], &name);
    assert_eq!(tar[613], 0);
    assert!(tar.windows(101).any(|w| w == vec![b'x'; 101]));
    assert!(tar.chunks(512).any(|h| h[0] == 255));
}
#[test]
fn verifies_large_file_assembly_and_refuses_wrong_logical_hash() {
    let mut s = Store::new();
    let chunk = vec![7; 4 * 1024 * 1024];
    let cid = put(&mut s, Object::Chunk(chunk.clone()));
    let tail = put(&mut s, Object::Chunk(vec![8]));
    let mut hash = blake3::Hasher::new();
    for _ in 0..4 {
        hash.update(&chunk);
    }
    hash.update(&[8]);
    let mut large = LargeBlob {
        length: 16 * 1024 * 1024 + 1,
        content_digest: *hash.finalize().as_bytes(),
        chunks: vec![cid, cid, cid, cid, tail],
    };
    let lid = put(&mut s, Object::LargeBlob(large.clone()));
    let root = tree(&mut s, vec![(b"large", EntryKind::Regular, lid)]);
    assert!(archive(&s, root).is_ok());
    large.content_digest[0] ^= 1;
    let bad = put(&mut s, Object::LargeBlob(large));
    let root = tree(&mut s, vec![(b"bad", EntryKind::Regular, bad)]);
    assert!(archive(&s, root).unwrap_err().contains("digest"));
}

#[cfg(unix)]
#[test]
fn capable_tar_restores_raw_long_names_bytes_modes_and_symlink_targets() {
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
        process::Command,
    };
    let dir = std::env::temp_dir().join(format!("kai-bundle-restore-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let mut s = Store::new();
    let content = vec![0, 255, 13, 10];
    let blob = put(
        &mut s,
        Object::Blob(Blob {
            data: content.clone(),
        }),
    );
    let target = vec![b'x'; 120];
    let link = put(
        &mut s,
        Object::Blob(Blob {
            data: target.clone(),
        }),
    );
    let name = vec![b'n'; 120];
    let raw_name: Vec<u8> = if cfg!(target_os = "linux") {
        vec![255]
    } else {
        b"raw.bin".to_vec()
    };
    let sub = tree(&mut s, vec![(&raw_name, EntryKind::Regular, blob)]);
    let root = tree(
        &mut s,
        vec![
            (&name, EntryKind::Executable, blob),
            (b"link", EntryKind::Symlink, link),
            (b"nested", EntryKind::Directory, sub),
        ],
    );
    let archive_path = dir.join("source.tar");
    fs::write(&archive_path, archive(&s, root).unwrap()).unwrap();
    let restored = dir.join("restored");
    fs::create_dir(&restored).unwrap();
    let result = Command::new("tar")
        .arg("-xf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&restored)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let executable = restored.join(std::ffi::OsString::from_vec(name));
    assert_eq!(fs::read(&executable).unwrap(), content);
    assert_eq!(
        fs::metadata(executable).unwrap().permissions().mode() & 0o777,
        0o755
    );
    let raw = restored
        .join("nested")
        .join(std::ffi::OsString::from_vec(raw_name));
    assert_eq!(fs::read(&raw).unwrap(), content);
    assert_eq!(
        fs::metadata(raw).unwrap().permissions().mode() & 0o777,
        0o644
    );
    assert_eq!(
        fs::read_link(restored.join("link"))
            .unwrap()
            .into_os_string(),
        std::ffi::OsString::from_vec(target)
    );
}

#[test]
fn resource_and_unsupported_source_refusals_are_explicit() {
    let (mut s, root) = setup();
    let mut m = package(&s, root);
    let op = put(&mut s, support_object());
    m.contents.inventory.push(InventoryEntry {
        id: op,
        type_code: 18,
        framed_length: s[&op].len() as u64,
        interpretation: Interpretation::Interpreted,
    });
    m.contents.inventory.sort_by_key(|e| e.id);
    assert!(
        Bundle::read(container(&m, &s).as_slice())
            .err()
            .unwrap()
            .contains("unsupported")
    );
    let cid = put(&mut s, Object::Chunk(vec![1]));
    let large = put(
        &mut s,
        Object::LargeBlob(LargeBlob {
            length: 1024 * 1024 * 1024 + 1,
            content_digest: [0; 32],
            chunks: vec![cid; 257],
        }),
    );
    s.remove(&op);
    let root = tree(&mut s, vec![(b"huge", EntryKind::Regular, large)]);
    assert!(
        archive(&s, root)
            .unwrap_err()
            .contains("logical verification limit")
    );
}
fn support_object() -> Object {
    Object::Policy(Box::new(Policy {
        repository: [1; 16],
        revision: 1,
        protected_refs: vec![],
        required_checks: vec![],
        required_reviewers: 0,
        allowed_principals: vec![],
        selective_ci: SelectiveCi::Disabled,
        rewrite: RewritePolicy::Deny,
        required_features: vec![],
        predecessor: None,
    }))
}
