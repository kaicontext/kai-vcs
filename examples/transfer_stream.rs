#[allow(dead_code)]
#[path = "support/repository.rs"]
mod fixture;
use kai_vcs::transfer_frame::{FrameLimits, encode_frame};
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 2 {
        return Err("usage: transfer_stream OUTPUT.kai-frames".into());
    }
    let f = fixture::partitioned_fixture();
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    for object in f.store.values() {
        out.write_all(&encode_frame(object, FrameLimits::default())?)?;
    }
    out.flush()?;
    println!(
        "Wrote {} object frames. No refs were published.",
        f.store.len()
    );
    Ok(())
}
