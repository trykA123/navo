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
mod metrics;
mod profile;
mod settings;
mod shell_menu;
mod startup;
mod watch_file;
#[cfg(windows)]
mod windows_memory;

use std::process::ExitCode;
use std::time::Instant;

use cli::{Command, Start};
use metrics::ProcessClock;
use profile::Profile;
use rusty_store::StoreDirs;

fn main() -> ExitCode {
    let started = Instant::now();
    let clock = ProcessClock::detect(started);
    metrics::mark("main");
    let parsed = cli::parse(std::env::args_os().skip(1));
    if parsed.as_ref().map_or(true, Command::uses_console) {
        console::attach();
    }
    let profile = match &parsed {
        Ok(Command::Open(start)) => profile::choose(start, StoreDirs::detect),
        _ => Ok(Profile::User(StoreDirs::detect())),
    };
    let profile = match profile {
        Ok(profile) => profile,
        Err(error) => {
            eprintln!("ferrite: cannot create a temporary profile for --metrics: {error}");
            return ExitCode::FAILURE;
        }
    };
    let diagnostics = logging::install(profile.dirs().map(|dirs| dirs.data.as_path()));
    logging::log_start();
    metrics::mark("logging ready");
    match parsed {
        Ok(Command::Open(start)) => open_window(start, &profile, clock),
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

fn open_window(start: Start, profile: &Profile, clock: ProcessClock) -> ExitCode {
    let panes = match gui::pane_folders(&start) {
        Ok(panes) => panes,
        Err(message) => {
            eprintln!("ferrite: {message}\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    let run = start
        .metrics
        .clone()
        .map(|path| metrics::Run::new(path, clock));
    gui::open(gui::Launch {
        view: start.view,
        panes,
        dirs: profile.dirs().cloned(),
        start_watch: run.as_ref().map(metrics::Run::watch),
    });
    run.map_or(ExitCode::SUCCESS, |run| run.exit_code())
}
