fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        eprintln!("usage: kai-fixtures NEW_OUTPUT_DIRECTORY");
        std::process::exit(2);
    }
    match kai_fixtures::generate(std::path::Path::new(&args[0])) {
        Ok(v) => println!(
            "Generated F01-F07: {} cases, {} recorded artifacts",
            v["cases"].as_array().unwrap().len(),
            v["artifacts"].as_object().unwrap().len()
        ),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
