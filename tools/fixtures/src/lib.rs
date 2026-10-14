//! Deterministic P0 fixture inputs; no claim of runtime feature qualification.
use kai_vcs::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
mod git;
mod native;
pub const SEED: u64 = 0x4b41492d50302d31;
pub type Result<T> = std::result::Result<T, String>;
pub struct Generator {
    root: PathBuf,
    artifacts: BTreeMap<String, Value>,
    cases: Vec<Value>,
}
impl Generator {
    fn artifact(&mut self, name: &str, data: &[u8]) -> Result<()> {
        let path = self.root.join(name);
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut f| f.write_all(data))
            .map_err(|e| e.to_string())?;
        self.artifacts.insert(
            name.into(),
            json!({"bytes":data.len(),"sha256":format!("{:x}",Sha256::digest(data))}),
        );
        Ok(())
    }
    fn snapshot(
        &mut self,
        fixture: &str,
        name: &str,
        files: Vec<(Vec<u8>, EntryKind, Vec<u8>)>,
    ) -> Result<(ObjectId, BTreeMap<ObjectId, Vec<u8>>)> {
        let mut objects = BTreeMap::new();
        let mut entries = vec![];
        let mut expected = vec![];
        for (path, kind, data) in files {
            let id = put(&mut objects, Object::Blob(Blob { data: data.clone() }))?;
            expected.push(json!({"path_hex":hex(&path),"kind":kind as u8,"content_id":id.to_string(),"bytes":data.len(),"sha256":format!("{:x}",Sha256::digest(&data))}));
            entries.push(TreeEntry {
                name: path,
                kind,
                target: Target::Native(id),
            });
        }
        expected.sort_by_key(|v| v["path_hex"].as_str().unwrap().to_owned());
        let tree = put(&mut objects, Object::Tree(Tree::new(entries)?))?;
        self.bundle(fixture, name, &objects, tree, 2)?;
        self.cases.push(
            json!({"fixture":fixture,"name":name,"tree":tree.to_string(),"entries":expected}),
        );
        Ok((tree, objects))
    }
    fn bundle(
        &mut self,
        fixture: &str,
        name: &str,
        objects: &BTreeMap<ObjectId, Vec<u8>>,
        root: ObjectId,
        kind: u16,
    ) -> Result<()> {
        let manifest = TransferManifest {
            repository: [0x42; 16],
            contents: PackageContents {
                completeness: PackageCompleteness::Standalone,
                roots: vec![PackageRoot {
                    id: root,
                    type_code: kind,
                }],
                inventory: objects
                    .iter()
                    .map(|(id, b)| {
                        Ok(InventoryEntry {
                            id: *id,
                            type_code: Object::decode(b)?.type_code(),
                            framed_length: b.len() as u64,
                            interpretation: Interpretation::Interpreted,
                        })
                    })
                    .collect::<Result<_>>()?,
                prerequisites: vec![],
                required_features: vec![],
                optional_features: vec![],
            },
        };
        verify_package(
            &Object::TransferManifest(Box::new(manifest.clone())),
            &[],
            ClosureLimits::default(),
            |id, _| {
                objects
                    .get(&id)
                    .cloned()
                    .ok_or("missing fixture object".into())
            },
        )?;
        let mut out = b"KAI-BUNDLE\0\0\x01".to_vec();
        for b in std::iter::once(manifest.encode()?).chain(objects.values().cloned()) {
            out.extend((b.len() as u64).to_be_bytes());
            out.extend(b);
        }
        kai_object_reference::bundle::Bundle::read(out.as_slice())?;
        self.artifact(&format!("{fixture}/{name}.kai-bundle"), &out)
    }
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
fn put(s: &mut BTreeMap<ObjectId, Vec<u8>>, o: Object) -> Result<ObjectId> {
    let b = o.encode()?;
    let id = ObjectId::hash(&b);
    let reference = kai_object_reference::id(&b)?;
    if reference != id.to_string() {
        return Err("independent fixture identity mismatch".into());
    }
    s.insert(id, b);
    Ok(id)
}
fn person(time: i64) -> Identity {
    Identity {
        name: b"Fixture".to_vec(),
        email: b"fixture@example.test".to_vec(),
        timestamp_seconds: time,
        timezone_minutes: 0,
    }
}
fn revision(
    s: &mut BTreeMap<ObjectId, Vec<u8>>,
    tree: ObjectId,
    parents: Vec<ObjectId>,
    change: u8,
    time: i64,
) -> Result<ObjectId> {
    put(
        s,
        Object::Revision(Box::new(Revision {
            tree,
            parents,
            change_id: [change; 16],
            author: person(time),
            committer: person(time),
            message: b"repeated message\n".to_vec(),
            provenance: vec![],
            git_envelope: None,
            conflicts: vec![],
        })),
    )
}
pub fn generate(destination: &Path) -> Result<Value> {
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    let mut g = Generator {
        root: destination.into(),
        artifacts: BTreeMap::new(),
        cases: vec![],
    };
    native::generate(&mut g)?;
    git::generate(&mut g)?;
    g.cases.sort_by_key(|v| {
        (
            v["fixture"].as_str().unwrap().to_owned(),
            v["name"].as_str().unwrap().to_owned(),
        )
    });
    let manifest = json!({"schema_version":1,"seed":SEED.to_string(),"object_format":1,"cases":g.cases,"artifacts":g.artifacts,
        "scope":"P0 reproducible inputs; runtime acceptance remains separate",
        "deferred":["P1 durable object/root commit and power-loss faults","P2 workspace capture/checkout and symlink-race execution","P4 actual merge results","P5 transport authentication/pack decompression","P6 full Git import and signature trust"]});
    fs::write(
        destination.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(manifest)
}

pub mod faults;
