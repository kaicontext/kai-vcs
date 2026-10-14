use kai_fixtures::faults::{InjectedWriter, WriteFault};
use std::{
    fs,
    io::{ErrorKind, Write},
};

#[test]
fn deterministic_manifest_and_no_overwrite() {
    let root = std::env::temp_dir().join(format!("kai-fixtures-test-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/acceptance/manifest.json"
    ))
    .unwrap();
    for name in ["first", "second"] {
        let output = root.join(name);
        assert_eq!(kai_fixtures::generate(&output).unwrap(), expected);
        assert!(kai_fixtures::generate(&output).is_err());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn short_and_interrupted_writes_retry_without_losing_bytes() {
    for action in [WriteFault::Short(2), WriteFault::InterruptOnce] {
        let mut writer = InjectedWriter {
            inner: Vec::new(),
            call: 0,
            at: 1,
            action,
        };
        writer.write_all(b"prefix").unwrap();
        writer.write_all(b"payload").unwrap();
        assert_eq!(writer.inner, b"prefixpayload");
    }
}

#[test]
fn failed_writes_preserve_only_completed_prefix() {
    for (action, kind) in [
        (WriteFault::Zero, ErrorKind::WriteZero),
        (
            WriteFault::Error(ErrorKind::StorageFull),
            ErrorKind::StorageFull,
        ),
        (WriteFault::Error(ErrorKind::Other), ErrorKind::Other),
        (
            WriteFault::Error(ErrorKind::PermissionDenied),
            ErrorKind::PermissionDenied,
        ),
    ] {
        let mut writer = InjectedWriter {
            inner: Vec::new(),
            call: 0,
            at: 1,
            action,
        };
        writer.write_all(b"prefix").unwrap();
        assert_eq!(writer.write_all(b"payload").unwrap_err().kind(), kind);
        assert_eq!(writer.inner, b"prefix");
    }
}
