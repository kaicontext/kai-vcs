#[allow(dead_code)]
#[path = "support/repository.rs"]
mod fixture;
use kai_vcs::*;
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .ok_or("usage: repository_bundle OUTPUT.kai-bundle")?;
    let f = fixture::fixture();
    verify_package(
        &Object::BackupManifest(Box::new(f.backup.clone())),
        &[],
        ClosureLimits::default(),
        |id, _| f.store.get(&id).cloned().ok_or("missing".into()),
    )?;
    let mut out = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)?;
    out.write_all(&fixture::container(&f))?;
    out.flush()?;
    println!(
        "Resolved revision: {}\nUnresolved revision: {}\nCheckpoint: {}\nView: {}",
        f.resolved, f.revision, f.checkpoint, f.view
    );
    Ok(())
}
