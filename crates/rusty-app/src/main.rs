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
use rusty_core::Location;
use rusty_store::StoreDirs;

fn main() -> ExitCode {
    execute(std::env::args_os().skip(1), StoreDirs::detect)
}

fn execute(
    args: impl IntoIterator<Item = std::ffi::OsString>,
    detect: impl FnOnce() -> Option<StoreDirs>,
) -> ExitCode {
    let started = Instant::now();
    metrics::mark("main");
    let parsed = cli::parse(args);
    if parsed.as_ref().map_or(true, Command::uses_console) {
        console::attach();
    }
    let command = match parsed {
        Ok(command) => command,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };
    let panes = match &command {
        Command::Open(start) => match gui::pane_folders(start) {
            Ok(panes) => panes,
            Err(message) => {
                console::attach();
                eprintln!("ferrite: {message}\n{}", cli::USAGE);
                return ExitCode::from(2);
            }
        },
        _ => [None, None],
    };
    execute_command(command, panes, started, detect)
}

fn execute_command(
    command: Command,
    panes: [Option<Location>; 2],
    started: Instant,
    detect: impl FnOnce() -> Option<StoreDirs>,
) -> ExitCode {
    let clock = ProcessClock::detect(started, metrics::process_age);
    let profile = match &command {
        Command::Open(start) => profile::choose(start, detect),
        _ => Ok(Profile::User(detect())),
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
    match command {
        Command::Open(start) => open_window(start, panes, &profile, clock),
        Command::Version => {
            println!("ferrite {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::DiagReport(path) => match diag::write(&path, started, &diagnostics) {
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
    }
}

fn open_window(
    start: Start,
    panes: [Option<Location>; 2],
    profile: &Profile,
    clock: ProcessClock,
) -> ExitCode {
    let run = start
        .metrics
        .clone()
        .map(|path| metrics::Run::new(path, clock));
    gui::open(gui::Launch {
        view: start.view,
        panes,
        dirs: profile.dirs().cloned(),
        cleanup: profile.cleanup(),
        start_watch: run.as_ref().map(metrics::Run::watch),
    });
    if let Err(error) = profile.cleanup().run() {
        eprintln!("ferrite: cannot remove the temporary profile: {error}");
        return ExitCode::FAILURE;
    }
    run.map_or(ExitCode::SUCCESS, |run| run.exit_code())
}

#[cfg(test)]
mod tests;
