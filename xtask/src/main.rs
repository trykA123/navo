#![forbid(unsafe_code)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "xtask is a CLI that reports to the terminal"
)]

mod binsize;
mod coverage;
mod gate;
mod size;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("size") => size::run(rest),
        Some("coverage") => coverage::run(rest),
        Some("binsize") => binsize::run(rest),
        Some("gate") => gate::run(rest),
        _ => {
            eprintln!("usage: cargo xtask <size | coverage | binsize <path> | gate [--coverage]>");
            ExitCode::from(2)
        }
    }
}
