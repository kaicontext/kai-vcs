//! Deterministic cross-platform recovery inputs and negative closure fixtures.
use kai_vcs::*;
use std::{collections::BTreeMap, io::Write, path::Path};
fn put(s: &mut BTreeMap<ObjectId, Vec<u8>>, o: Object) -> ObjectId {
    let b = o.encode().unwrap();
    let id = ObjectId::hash(&b);
    s.insert(id, b);
    id
}
fn write_case(
    dir: &Path,
    name: &str,
    names: Vec<Vec<u8>>,
    mode: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut s = BTreeMap::new();
    let blob = put(
        &mut s,
        Object::Blob(Blob {
            data: vec![0, 255, 13, 10],
        }),
    );
    let tree = put(
        &mut s,
        Object::Tree(Tree::new(
            names
                .into_iter()
                .map(|name| TreeEntry {
                    name,
                    kind: EntryKind::Regular,
                    target: Target::Native(blob),
                })
                .collect(),
        )?),
    );
    let mut inventory: Vec<_> = s
        .iter()
        .map(|(id, b)| InventoryEntry {
            id: *id,
            type_code: Object::decode(b).unwrap().type_code(),
            framed_length: b.len() as u64,
            interpretation: Interpretation::Interpreted,
        })
        .collect();
    let mut prerequisites = vec![];
    if mode == "missing" || mode == "incremental" {
        let i = inventory.iter().position(|e| e.id == blob).unwrap();
        let e = inventory.remove(i);
        if mode == "incremental" {
            prerequisites.push(e);
        }
    }
    let manifest = TransferManifest {
        repository: [7; 16],
        contents: PackageContents {
            completeness: if mode == "incremental" {
                PackageCompleteness::Incremental
            } else {
                PackageCompleteness::Standalone
            },
            roots: vec![PackageRoot {
                id: tree,
                type_code: 2,
            }],
            inventory,
            prerequisites,
            required_features: vec![],
            optional_features: vec![],
        },
    };
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(format!("{name}.kai-bundle")))?;
    out.write_all(b"KAI-BUNDLE\0\0\x01")?;
    for b in std::iter::once(manifest.encode()?)
        .chain(manifest.contents.inventory.iter().map(|e| s[&e.id].clone()))
    {
        out.write_all(&(b.len() as u64).to_be_bytes())?;
        out.write_all(&b)?;
    }
    out.flush()?;
    Ok(tree.to_string())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: recovery_cases EXISTING_OUTPUT_DIRECTORY")?;
    let dir = Path::new(&path);
    let mut roots = BTreeMap::new();
    for (name, names, mode) in [
        (
            "portable",
            vec![
                b"space name.txt".to_vec(),
                vec![b'n'; 120],
                "snowman-\u{2603}.bin".as_bytes().to_vec(),
            ],
            "valid",
        ),
        ("raw-name", vec![vec![255]], "valid"),
        (
            "control-name",
            vec![b"line\nbreak".to_vec(), b"tab\tname".to_vec()],
            "valid",
        ),
        ("reserved", vec![b"CON.txt".to_vec()], "valid"),
        ("metadata-alias", vec![b".GiT".to_vec()], "valid"),
        (
            "case-collision",
            vec![b"A".to_vec(), b"a".to_vec()],
            "valid",
        ),
        (
            "normalization-collision",
            vec![
                "\u{00e9}".as_bytes().to_vec(),
                "e\u{0301}".as_bytes().to_vec(),
            ],
            "valid",
        ),
        ("missing", vec![b"file".to_vec()], "missing"),
        ("incremental", vec![b"file".to_vec()], "incremental"),
    ] {
        roots.insert(name, write_case(dir, name, names, mode)?);
    }
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join("case-roots.json"))?;
    out.write_all(&serde_json::to_vec_pretty(&roots)?)?;
    Ok(())
}
