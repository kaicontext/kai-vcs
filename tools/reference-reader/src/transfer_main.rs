use kai_object_reference::transfer::{Limits, Stream};
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 2 {
        return Err("usage: kai-transfer-reader STREAM".into());
    }
    let input = std::fs::File::open(&args[1]).map_err(|e| e.to_string())?;
    let mut stream = Stream::new(input, Limits::default())?;
    let mut count = 0;
    let mut bytes = 0;
    while let Some(object) = stream.next_object()? {
        count += 1;
        bytes += object.len();
    }
    println!(
        "Verified {count} object frames ({bytes} canonical bytes). Closure and publication are not verified."
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
