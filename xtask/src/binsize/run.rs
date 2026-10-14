use std::fs;
use std::process::ExitCode;

use super::{RUSTY_EXE_LIMIT, check_size};

pub fn run(args: &[String]) -> ExitCode {
    let [path] = args else {
        eprintln!("usage: cargo xtask binsize <path>");
        return ExitCode::from(2);
    };
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let bytes = metadata.len();
    println!("{path}: {bytes} bytes (limit {RUSTY_EXE_LIMIT})");
    if check_size(bytes, RUSTY_EXE_LIMIT).is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
