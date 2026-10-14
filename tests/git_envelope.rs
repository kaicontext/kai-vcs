use kai_vcs::*;
use std::{
    collections::HashMap,
    fs,
    io::Write,
    process::{Command, Stdio},
};
mod support;
use support::{person, sample};
fn envelope(name: &str) -> GitEnvelope {
    let Object::GitEnvelope(v) = sample(name) else {
        panic!()
    };
    v
}
#[test]
fn git_hashes_include_kind_length_nul_and_algorithm_namespace() {
    let empty = GitEnvelope::new(GitKind::Blob, 1, vec![]).unwrap();
    assert_eq!(
        empty.git_id.hex(),
        "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
    );
    let empty_tree = GitEnvelope::new(GitKind::Tree, 1, vec![]).unwrap();
    assert_eq!(
        empty_tree.git_id.hex(),
        "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
    );
    assert_ne!(empty.id().unwrap(), empty_tree.id().unwrap());
    let sha256 = GitEnvelope::new(GitKind::Blob, 2, vec![]).unwrap();
    assert_ne!(empty.id().unwrap(), sha256.id().unwrap());
    assert_eq!(sha256.git_id.digest().len(), 32);
    assert!(GitEnvelope::new(GitKind::Blob, 3, vec![]).is_err());
}
#[test]
fn git_envelopes_preserve_binary_signatures_and_parent_order() {
    for algorithm in ["sha1", "sha256"] {
        for kind in ["blob", "tree", "commit", "tag"] {
            let v = envelope(&format!("git-{kind}-{algorithm}"));
            let b = v.encode().unwrap();
            assert_eq!(GitEnvelope::decode(&b).unwrap(), v);
            assert_eq!(
                kai_object_reference::encode(&kai_object_reference::decode(&b).unwrap()).unwrap(),
                b
            );
            if kind == "commit" {
                assert!(v.body.windows(8).any(|w| w == b"gpgsig -"));
                assert!(v.body.ends_with(b"message\r\n"));
            }
        }
    }
}
#[test]
fn git_envelopes_reject_body_digest_and_kind_tampering() {
    let v = envelope("git-commit-sha256");
    let mut changed = v.clone();
    changed.body.push(0);
    assert!(changed.encode().is_err());
    changed = v.clone();
    changed.kind = GitKind::Tag;
    assert!(changed.encode().is_err());
    let mut wire = v.encode().unwrap();
    *wire.last_mut().unwrap() ^= 1;
    assert!(GitEnvelope::decode(&wire).is_err());
    assert!(kai_object_reference::decode(&wire).is_err());
    changed = v;
    changed.git_id = GitObjectId::Sha1([0; 20]);
    assert!(changed.encode().is_err());
}
#[test]
fn opaque_archival_bodies_do_not_claim_semantic_validity() {
    let malformed =
        GitEnvelope::new(GitKind::Commit, 1, b"not a valid commit\0\xff".to_vec()).unwrap();
    assert_eq!(
        GitEnvelope::decode(&malformed.encode().unwrap())
            .unwrap()
            .body,
        malformed.body
    );
    assert!(Object::GitEnvelope(malformed).references().is_empty());
}
#[test]
fn closure_requires_correct_envelope_kind_and_retains_original_bytes() {
    let mut store = HashMap::new();
    let tree = Object::Tree(Tree::new(vec![]).unwrap());
    let tree_id = tree.id().unwrap();
    store.insert(tree_id, tree.encode().unwrap());
    let mut revision = Revision {
        tree: tree_id,
        parents: vec![],
        change_id: [1; 16],
        author: person(),
        committer: person(),
        message: vec![],
        provenance: vec![],
        git_envelope: None,
        conflicts: vec![],
    };
    for (kind, allowed) in [
        (GitKind::Blob, false),
        (GitKind::Tree, false),
        (GitKind::Tag, false),
        (GitKind::Commit, true),
    ] {
        let g = GitEnvelope::new(kind, 1, vec![]).unwrap();
        let gid = g.id().unwrap();
        store.insert(gid, g.encode().unwrap());
        revision.git_envelope = Some(gid);
        let bytes = revision.encode().unwrap();
        let root = ObjectId::hash(&bytes);
        store.insert(root, bytes);
        let result = verify_closure(&[root], ClosureLimits::default(), |id| {
            store.get(&id).cloned().ok_or("missing".into())
        });
        assert_eq!(result.is_ok(), allowed);
        store.remove(&gid);
        assert!(
            verify_closure(&[root], ClosureLimits::default(), |id| store
                .get(&id)
                .cloned()
                .ok_or("missing".into()))
            .is_err()
        );
    }
}
#[test]
fn closure_rejects_tag_commit_swaps_and_conflicted_imports() {
    let mut store = HashMap::new();
    let tree = Object::Tree(Tree::new(vec![]).unwrap());
    let tid = tree.id().unwrap();
    store.insert(tid, tree.encode().unwrap());
    let mut rev = Revision {
        tree: tid,
        parents: vec![],
        change_id: [1; 16],
        author: person(),
        committer: person(),
        message: vec![],
        provenance: vec![],
        git_envelope: None,
        conflicts: vec![],
    };
    let rid = rev.id().unwrap();
    store.insert(rid, rev.encode().unwrap());
    for kind in [GitKind::Commit, GitKind::Tag] {
        let env = GitEnvelope::new(kind, 1, vec![]).unwrap();
        let gid = env.id().unwrap();
        store.insert(gid, env.encode().unwrap());
        let tag = Tag {
            target: rid,
            message: vec![],
            tagger: person(),
            git_envelope: Some(gid),
        };
        let root = tag.id().unwrap();
        store.insert(root, tag.encode().unwrap());
        let result = verify_closure(&[root], ClosureLimits::default(), |id| {
            store.get(&id).cloned().ok_or("missing".into())
        });
        assert_eq!(result.is_ok(), kind == GitKind::Tag);
        if kind == GitKind::Commit {
            rev.git_envelope = Some(gid);
            rev.conflicts = vec![(b"file".to_vec(), ObjectId::from_digest([99; 32]))];
            let root = rev.id().unwrap();
            store.insert(root, rev.encode().unwrap());
            let result = verify_closure(&[root], ClosureLimits::default(), |id| {
                store.get(&id).cloned().ok_or("missing".into())
            });
            assert!(result.unwrap_err().contains("conflicted revision"));
        }
    }
}

#[test]
fn upstream_git_roundtrips_both_algorithms_and_all_four_object_kinds() {
    let dir = std::env::temp_dir().join(format!("kai-git-envelope-test-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    fn git() -> Command {
        let mut c = Command::new("git");
        for k in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CONFIG_COUNT",
        ] {
            c.env_remove(k);
        }
        c.env("GIT_CONFIG_NOSYSTEM", "1");
        c.env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        );
        c
    }
    for algorithm in ["sha1", "sha256"] {
        let repo = dir.join(algorithm);
        let init = git()
            .args([
                "init",
                "--bare",
                "--template=",
                &format!("--object-format={algorithm}"),
            ])
            .arg(&repo)
            .output()
            .unwrap();
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        for kind in ["blob", "tree", "commit", "tag"] {
            let v = envelope(&format!("git-{kind}-{algorithm}"));
            let mut child = git()
                .arg("--git-dir")
                .arg(&repo)
                .args(["hash-object", "-w", "--literally", "-t", kind, "--stdin"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(&v.body).unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8(output.stdout).unwrap().trim(),
                v.git_id.hex()
            );
            let output = git()
                .arg("--git-dir")
                .arg(&repo)
                .args(["cat-file", kind, &v.git_id.hex()])
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, v.body);
        }
    }
}
