use kai_object_reference::bundle::Bundle;
use std::fs::{File, OpenOptions};
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let usage = "usage: kai-bundle-reader BUNDLE [--archive ROOT OUTPUT.tar]";
    if args.len() == 1 && args[0] == "--help" {
        println!("{usage}");
        return Ok(());
    }
    if args.len() != 1 && !(args.len() == 4 && args[1] == "--archive") {
        return Err(usage.into());
    }
    let bundle = Bundle::read(File::open(&args[0]).map_err(|e| e.to_string())?)?;
    if args.len() == 1 {
        for root in bundle.roots() {
            println!("{root}");
        }
    } else {
        let archive = bundle.archive(args[2].to_str().ok_or("root must be ASCII native ID")?)?;
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[3])
            .map_err(|e| e.to_string())?;
        archive.write(output)?;
        println!(
            "Verified source snapshot exported; external Gitlink/LFS payloads are not included."
        );
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
