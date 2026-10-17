#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line entry point reports to the terminal"
)]

mod app_menu;
mod cli;
mod console;
mod diag;
mod drives;
mod editor;
mod files;
mod gui;
mod jobs;
mod keymap;
mod launch;
mod logging;
mod settings;
mod shell_menu;
mod startup;
mod watch_file;
#[cfg(windows)]
mod windows_memory;

use std::process::ExitCode;
use std::time::Instant;

use cli::Command;
use rusty_store::StoreDirs;

fn main() -> ExitCode {
    let started = Instant::now();
    let parsed = cli::parse(std::env::args_os().skip(1));
    if parsed.as_ref().map_or(true, Command::uses_console) {
        console::attach();
    }
    let diagnostics =
        logging::install(StoreDirs::detect().as_ref().map(|dirs| dirs.data.as_path()));
    logging::log_start();
    match parsed {
        Ok(Command::Open) => {
            gui::open(None);
            ExitCode::SUCCESS
        }
        Ok(Command::View(path)) => {
            gui::open(Some(path));
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("ferrite {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::DiagReport(path)) => match diag::write(&path, started, &diagnostics) {
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
