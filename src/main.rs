use std::env;
use std::fs;
use std::io::{self, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let (path, seed) = match (
        args.next(),
        args.next(),
        args.next(),
        args.next(),
        args.next(),
    ) {
        (Some(command), Some(path), Some(flag), Some(seed), None)
            if command == "generate" && flag == "--seed" =>
        {
            (path, seed)
        }
        _ => return Err("usage: atcgen generate <file> --seed <u64>".into()),
    };
    let seed = seed
        .to_str()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or("invalid seed: expected an unsigned 64-bit integer")?;
    let path = PathBuf::from(path);
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("{}: failed to read DSL file: {error}", path.display()))?;
    let output =
        atcgen::generate(&source, seed).map_err(|error| format!("{}:{error}", path.display()))?;
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(output.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("failed to write generated input: {error}"))
}
