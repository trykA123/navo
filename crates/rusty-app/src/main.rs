#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line entry point reports to the terminal"
)]

mod cli;
mod console;
mod diag;
mod gui;
mod jobs;
mod startup;
#[cfg(windows)]
mod windows_memory;

use std::process::ExitCode;
use std::time::Instant;

use cli::Command;

fn main() -> ExitCode {
    let started = Instant::now();
    let parsed = cli::parse(std::env::args_os().skip(1));
    if parsed.as_ref().map_or(true, Command::uses_console) {
        console::attach();
    }
    match parsed {
        Ok(Command::Open) => {
            gui::open();
            ExitCode::SUCCESS
        }
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
