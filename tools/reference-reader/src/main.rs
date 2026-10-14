use kai_object_reference::{Payload, Value, decode, id};
use std::{
    fs::File,
    io::{Read, Write},
};

fn run() -> Result<(), (i32, String)> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--help" {
        println!("usage: kai-object-reader OBJECT_FILE [--raw|--git-raw]");
        return Ok(());
    }
    if args.is_empty()
        || args.len() > 2
        || (args.len() == 2 && args[1] != "--raw" && args[1] != "--git-raw")
    {
        return Err((
            2,
            "usage: kai-object-reader OBJECT_FILE [--raw|--git-raw]".into(),
        ));
    }
    let mut data = Vec::new();
    File::open(&args[0])
        .map_err(|e| (1, e.to_string()))?
        .take(64 * 1024 * 1024 + 24)
        .read_to_end(&mut data)
        .map_err(|e| (1, e.to_string()))?;
    let object = decode(&data).map_err(|e| (8, e))?;
    if args.len() == 2 {
        let bytes = if args[1] == "--git-raw" {
            if object.kind != 12 {
                return Err((2, "--git-raw requires a Git envelope".into()));
            }
            let Payload::Structured(Value::Map(mut fields)) = object.payload else {
                return Err((8, "invalid Git envelope".into()));
            };
            let Some(Value::Bytes(body)) = fields.remove(&2) else {
                return Err((8, "invalid Git body".into()));
            };
            body
        } else {
            let Payload::Raw(bytes) = object.payload else {
                return Err((2, "--raw requires a blob or chunk".into()));
            };
            bytes
        };
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|e| (1, e.to_string()))?;
    } else {
        println!(
            "{} type={} bytes={}",
            id(&data).map_err(|e| (8, e))?,
            object.kind,
            data.len()
        );
    }
    Ok(())
}

fn main() {
    if let Err((code, err)) = run() {
        eprintln!("{err}");
        std::process::exit(code);
    }
}
