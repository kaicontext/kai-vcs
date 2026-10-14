//! Deterministic complete source fixture, built with production object encoders.
use kai_vcs::*;
use std::{collections::BTreeMap, io::Write};
fn put(store: &mut BTreeMap<ObjectId, Vec<u8>>, object: Object) -> ObjectId {
    let data = object.encode().unwrap();
    let id = ObjectId::hash(&data);
    store.insert(id, data);
    id
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: source_bundle OUTPUT.kai-bundle")?;
    let mut s = BTreeMap::new();
    let hello = put(
        &mut s,
        Object::Blob(Blob {
            data: b"Hello from a verified Kai bundle.\n".to_vec(),
        }),
    );
    let script = put(
        &mut s,
        Object::Blob(Blob {
            data: b"#!/bin/sh\nprintf 'saved executable bytes\\n'\n".to_vec(),
        }),
    );
    let link = put(
        &mut s,
        Object::Blob(Blob {
            data: b"hello.txt".to_vec(),
        }),
    );
    let binary = put(
        &mut s,
        Object::Blob(Blob {
            data: vec![0, 255, 13, 10],
        }),
    );
    let sub = put(
        &mut s,
        Object::Tree(Tree::new(vec![TreeEntry {
            name: b"bytes.bin".to_vec(),
            kind: EntryKind::Regular,
            target: Target::Native(binary),
        }])?),
    );
    let tree = put(
        &mut s,
        Object::Tree(Tree::new(vec![
            TreeEntry {
                name: b"hello.txt".to_vec(),
                kind: EntryKind::Regular,
                target: Target::Native(hello),
            },
            TreeEntry {
                name: b"run.sh".to_vec(),
                kind: EntryKind::Executable,
                target: Target::Native(script),
            },
            TreeEntry {
                name: b"hello-link".to_vec(),
                kind: EntryKind::Symlink,
                target: Target::Native(link),
            },
            TreeEntry {
                name: b"data".to_vec(),
                kind: EntryKind::Directory,
                target: Target::Native(sub),
            },
        ])?),
    );
    let manifest = TransferManifest {
        repository: [1; 16],
        contents: PackageContents {
            completeness: PackageCompleteness::Standalone,
            roots: vec![PackageRoot {
                id: tree,
                type_code: 2,
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
    };
    verify_package(
        &Object::TransferManifest(Box::new(manifest.clone())),
        &[],
        ClosureLimits::default(),
        |id, _| s.get(&id).cloned().ok_or("missing".into()),
    )?;
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    out.write_all(b"KAI-BUNDLE\0\0\x01")?;
    for b in std::iter::once(manifest.encode()?).chain(s.into_values()) {
        out.write_all(&(b.len() as u64).to_be_bytes())?;
        out.write_all(&b)?;
    }
    out.flush()?;
    println!("{tree}");
    Ok(())
}
