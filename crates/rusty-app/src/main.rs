#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line entry point reports to the terminal"
)]

mod browse;
mod cli;
mod diag;
mod startup;
#[cfg(windows)]
mod windows_memory;

use std::process::ExitCode;
use std::time::Instant;

use cli::Command;

fn main() -> ExitCode {
    let started = Instant::now();
    match cli::parse(std::env::args_os().skip(1)) {
        Ok(Command::Version) => {
            println!("ferrite {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::DiagReport(path)) => match diag::write(&path, started) {
            Ok(summary) => {
                println!(
                    "ferrite: wrote {} ({} bytes)",
                    path.display(),
                    summary.bytes
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("ferrite: cannot write {}: {error}", path.display());
                ExitCode::FAILURE
            }
        },
        Err(usage) => {
            eprintln!("{usage}");
            ExitCode::from(2)
        }
    }
}
