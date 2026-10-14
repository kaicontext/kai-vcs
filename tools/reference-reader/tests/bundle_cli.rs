use kai_object_reference::{Payload, Record, Value, encode};
use std::{collections::BTreeMap, fs, process::Command};
fn n(v: u64) -> Value {
    Value::UInt(v)
}
fn a(v: Vec<Value>) -> Value {
    Value::Array(v)
}
fn b(v: &[u8]) -> Value {
    Value::Bytes(v.to_vec())
}
fn m(v: Vec<(u64, Value)>) -> Value {
    Value::Map(v.into_iter().collect())
}
fn reference(data: &[u8]) -> Value {
    a(vec![n(1), n(1), b(blake3::hash(data).as_bytes())])
}
fn fixture() -> (Vec<u8>, String) {
    let blob = encode(&Record {
        kind: 1,
        payload: Payload::Raw(b"exact\0\xff\r\n".to_vec()),
    })
    .unwrap();
    let tree = encode(&Record {
        kind: 2,
        payload: Payload::Structured(a(vec![m(vec![
            (0, b(b"file")),
            (1, n(2)),
            (2, reference(&blob)),
        ])])),
    })
    .unwrap();
    let root = format!("k1:b3:{}", blake3::hash(&tree).to_hex());
    let mut entries = BTreeMap::new();
    for (kind, data) in [(1, blob), (2, tree.clone())] {
        entries.insert(*blake3::hash(&data).as_bytes(), (kind, data));
    }
    let contents = m(vec![
        (0, n(1)),
        (1, a(vec![a(vec![reference(&tree), n(2)])])),
        (
            2,
            a(entries
                .values()
                .map(|(kind, data)| a(vec![reference(data), n(*kind), n(data.len() as u64), n(1)]))
                .collect()),
        ),
        (3, a(vec![])),
        (4, a(vec![])),
        (5, a(vec![])),
        (6, n(1)),
    ]);
    let manifest = encode(&Record {
        kind: 19,
        payload: Payload::Structured(m(vec![(0, b(&[1; 16])), (1, contents)])),
    })
    .unwrap();
    let mut result = b"KAI-BUNDLE\0\0\x01".to_vec();
    for data in std::iter::once(manifest).chain(entries.into_values().map(|(_, b)| b)) {
        result.extend((data.len() as u64).to_be_bytes());
        result.extend(data);
    }
    (result, root)
}
#[test]
fn cli_verifies_exports_refuses_overwrite_and_corruption_before_output() {
    let dir = std::env::temp_dir().join(format!("kai-bundle-cli-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let (mut data, root) = fixture();
    let input = dir.join("source.bundle");
    let output = dir.join("source.tar");
    fs::write(&input, &data).unwrap();
    let verify = Command::new(env!("CARGO_BIN_EXE_kai-bundle-reader"))
        .arg(&input)
        .output()
        .unwrap();
    assert!(verify.status.success());
    assert_eq!(
        String::from_utf8(verify.stdout).unwrap(),
        format!("{root}\n")
    );
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_kai-bundle-reader"))
            .arg(&input)
            .arg("--archive")
            .arg(&root)
            .arg(&output)
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let archive = fs::read(&output).unwrap();
    assert_eq!(&archive[512..522], b"exact\0\xff\r\n\0");
    assert!(!run().status.success());
    assert_eq!(fs::read(&output).unwrap(), archive);
    let recovery = dir.join("recovery.tar");
    let recover = || {
        Command::new(env!("CARGO_BIN_EXE_kai-bundle-reader"))
            .arg(&input)
            .arg("--recover")
            .arg(&recovery)
            .output()
            .unwrap()
    };
    assert!(recover().status.success());
    let recovered = fs::read(&recovery).unwrap();
    assert!(recovered.windows(data.len()).any(|b| b == data));
    assert!(!recover().status.success());
    assert_eq!(fs::read(&recovery).unwrap(), recovered);
    let index = Command::new(env!("CARGO_BIN_EXE_kai-bundle-reader"))
        .arg(&input)
        .arg("--index")
        .output()
        .unwrap();
    assert!(index.status.success());
    assert!(
        String::from_utf8(index.stdout)
            .unwrap()
            .contains("KAI-RECOVERY-INDEX 1")
    );
    fs::remove_file(&recovery).unwrap();
    fs::remove_file(&output).unwrap();
    *data.last_mut().unwrap() ^= 1;
    fs::write(&input, &data).unwrap();
    let rejected = run();
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    assert!(!output.exists());
    assert!(!recover().status.success());
    assert!(!recovery.exists());
}
