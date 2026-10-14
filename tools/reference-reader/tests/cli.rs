use std::{fs, process::Command};

#[test]
fn standalone_reader_extracts_exact_binary_without_production_crate() {
    let dir = std::env::temp_dir().join(format!("kai-object-reader-test-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let path = dir.join("object");
    let raw = b"\0\xff\r\n";
    let mut framed = b"KAI-OBJECT\0\0\x01\0\x01".to_vec();
    framed.extend_from_slice(&4u64.to_be_bytes());
    framed.extend_from_slice(raw);
    fs::write(&path, &framed).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_kai-object-reader"))
        .arg(&path)
        .arg("--raw")
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, raw);
    assert!(result.stderr.is_empty());
    let wrong_mode = Command::new(env!("CARGO_BIN_EXE_kai-object-reader"))
        .arg(&path)
        .arg("--git-raw")
        .output()
        .unwrap();
    assert_eq!(wrong_mode.status.code(), Some(2));
    assert!(wrong_mode.stdout.is_empty());
    framed[0] = b'X';
    fs::write(&path, &framed).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_kai-object-reader"))
        .arg(&path)
        .arg("--raw")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(8));
    assert!(result.stdout.is_empty());
    let result = Command::new(env!("CARGO_BIN_EXE_kai-object-reader"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
}

#[test]
fn standalone_reader_extracts_verified_git_bodies_and_refuses_corruption() {
    let dir = std::env::temp_dir().join(format!("kai-git-reader-test-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let path = dir.join("object");
    for line in include_str!("../../../tests/fixtures/objects.tsv")
        .lines()
        .filter(|l| l.starts_with("git-"))
    {
        let hex = line.split('\t').nth(1).unwrap();
        let mut framed: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let kai_object_reference::Payload::Structured(kai_object_reference::Value::Map(fields)) =
            kai_object_reference::decode(&framed).unwrap().payload
        else {
            panic!()
        };
        let kai_object_reference::Value::Bytes(expected) = &fields[&2] else {
            panic!()
        };
        fs::write(&path, &framed).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_kai-object-reader"))
            .arg(&path)
            .arg("--git-raw")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(&output.stdout, expected);
        assert!(output.stderr.is_empty());
        let wrong_mode = Command::new(env!("CARGO_BIN_EXE_kai-object-reader"))
            .arg(&path)
            .arg("--raw")
            .output()
            .unwrap();
        assert_eq!(wrong_mode.status.code(), Some(2));
        assert!(wrong_mode.stdout.is_empty());
        *framed.last_mut().unwrap() ^= 1;
        fs::write(&path, &framed).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_kai-object-reader"))
            .arg(&path)
            .arg("--git-raw")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(8));
        assert!(output.stdout.is_empty());
    }
}
