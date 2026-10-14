use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
#[test]
fn standalone_reader_accepts_valid_stream_and_never_prints_success_for_a_bad_suffix() {
    let dir = std::env::temp_dir().join(format!(
        "kai-transfer-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let path = dir.join("frames");
    let mut bytes = Vec::new();
    for row in include_str!("../../../tests/fixtures/transfer-frames.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let (_, hex) = row.split_once('\t').unwrap();
        bytes.extend(
            (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()),
        );
    }
    fs::write(&path, &bytes).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_kai-transfer-reader"))
            .arg(&path)
            .current_dir(&dir)
            .env("PATH", "")
            .env("HOME", &dir)
            .env("USERPROFILE", &dir)
            .output()
            .unwrap()
    };
    let good = run();
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    assert!(String::from_utf8_lossy(&good.stdout).contains("Verified 4 object frames"));
    bytes.push(0);
    fs::write(&path, &bytes).unwrap();
    let bad = run();
    assert!(!bad.status.success());
    assert!(bad.stdout.is_empty());
    assert!(!bad.stderr.is_empty());
    fs::remove_dir_all(dir).unwrap();
}
