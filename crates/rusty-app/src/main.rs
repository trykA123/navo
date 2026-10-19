#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line entry point reports to the terminal"
)]

mod app_menu;
mod cli;
mod clipboard;
mod console;
mod diag;
mod drives;
mod editor;
mod files;
mod gui;
mod instance;
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

use std::ops::ControlFlow;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use cli::{Command, Start};
use metrics::ProcessClock;
use profile::Profile;
use rusty_core::Location;
use rusty_store::StoreDirs;
use rusty_ui::window_view::{OpenSender, open_requests};

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
    match command {
        Command::Open(start) => open_window(start, panes, &profile, clock),
        Command::Version => {
            let _diagnostics = install_logging(&profile);
            println!("ferrite {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::DiagReport(path) => write_report(&path, started, &profile),
    }
}

fn install_logging(profile: &Profile) -> logging::Diagnostics {
    let diagnostics = logging::install(profile.dirs().map(|dirs| dirs.data.as_path()));
    logging::log_start();
    metrics::mark("logging ready");
    diagnostics
}

fn write_report(path: &Path, started: Instant, profile: &Profile) -> ExitCode {
    let diagnostics = install_logging(profile);
    match diag::write(path, started, &diagnostics) {
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
    }
}

fn open_window(
    start: Start,
    panes: [Option<Location>; 2],
    profile: &Profile,
    clock: ProcessClock,
) -> ExitCode {
    let (sender, requests) = open_requests();
    let _guard = match prepare_instance(&start, profile, sender) {
        Ok(ControlFlow::Continue(guard)) => guard,
        Ok(ControlFlow::Break(code)) => return code,
        Err(error) => {
            console::attach();
            eprintln!("ferrite: {error}");
            return ExitCode::FAILURE;
        }
    };
    let _diagnostics = install_logging(profile);
    let run = start
        .metrics
        .clone()
        .map(|path| metrics::Run::new(path, clock));
    gui::open(gui::Launch {
        path: start.path,
        view: start.view,
        panes,
        dirs: profile.dirs().cloned(),
        cleanup: profile.cleanup(),
        start_watch: run.as_ref().map(metrics::Run::watch),
        instance_requests: requests,
    });
    if let Err(error) = profile.cleanup().run() {
        eprintln!("ferrite: cannot remove the temporary profile: {error}");
        return ExitCode::FAILURE;
    }
    run.map_or(ExitCode::SUCCESS, |run| run.exit_code())
}

fn prepare_instance(
    start: &Start,
    profile: &Profile,
    sender: OpenSender,
) -> Result<ControlFlow<ExitCode, Option<instance::Guard>>, instance::Error> {
    if start.is_metrics() {
        return Ok(ControlFlow::Continue(None));
    }
    let Some(dirs) = profile.dirs() else {
        return Ok(ControlFlow::Continue(None));
    };
    let Some(acquired) = instance::acquire(&dirs.data)? else {
        return Ok(ControlFlow::Continue(None));
    };
    match acquired {
        instance::Acquired::First(mut guard) => {
            guard.serve(move |request| gui::on_instance_request(&sender, request))?;
            Ok(ControlFlow::Continue(Some(guard)))
        }
        instance::Acquired::Second(client) => {
            let request = instance::Request::from_start(start, &std::env::current_dir()?)?;
            let reply = client.send(&request, std::time::Duration::from_secs(2))?;
            let code = if reply == instance::Reply::Ok {
                ExitCode::SUCCESS
            } else {
                console::attach();
                eprintln!("ferrite: the running Ferrite rejected the request");
                ExitCode::FAILURE
            };
            Ok(ControlFlow::Break(code))
        }
    }
}

#[cfg(test)]
mod tests;
