//! A parent process releases each real archive boundary or kills this process.
use kai_fixtures::faults::{Boundary, Hook, ObservedWriter, Point};
use kai_object_reference::bundle::Bundle;
use std::{
    fs::File,
    io::{self, Read, Write},
};
struct ParentGate;
impl Hook for ParentGate {
    fn hit(&mut self, b: Boundary) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{} {}", b.point.name(), b.completed_bytes)?;
        stdout.flush()?;
        drop(stdout);
        let mut permit = [0];
        io::stdin().read_exact(&mut permit)?;
        if permit[0] != b'g' {
            return Err(io::Error::other("invalid gate permission"));
        }
        Ok(())
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: kai-fault-child BUNDLE NEW_OUTPUT.tar".into());
    }
    let bundle = Bundle::read(File::open(&args[0]).map_err(|e| e.to_string())?)?;
    let archive = bundle.recovery()?;
    let output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[1])
        .map_err(|e| e.to_string())?;
    let mut sink = ObservedWriter {
        inner: output,
        hook: ParentGate,
        completed_bytes: 0,
    };
    archive.write(&mut sink)?;
    sink.hook
        .hit(Boundary {
            point: Point::BeforeResponse,
            completed_bytes: sink.completed_bytes,
        })
        .map_err(|e| e.to_string())?;
    println!("DONE");
    io::stdout().flush().map_err(|e| e.to_string())?;
    sink.hook
        .hit(Boundary {
            point: Point::AfterResponse,
            completed_bytes: sink.completed_bytes,
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
