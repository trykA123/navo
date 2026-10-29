#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line entry point reports to the terminal"
)]

mod app_menu;
mod bench;
mod cli;
mod clipboard;
mod console;
mod contrast;
mod crash;
mod crash_report;
mod diag;
mod drag;
mod drives;
mod editor;
mod files;
mod gui;
mod heap;
mod index_service;
mod instance;
mod jobs;
mod keymap;
mod launch;
mod logging;
mod metrics;
mod migration;
mod pdf_helper;
mod perf;
mod profile;
mod rename_hooks;
mod rename_log;
mod retained_hooks;
mod search_hooks;
mod settings;
mod shell_menu;
mod splash;
mod startup;
mod terminal;
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
    heap::install();
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
        Ok(Command::PdfHelper) => return pdf_helper::run(),
        Ok(Command::IndexService(action)) => return index_service::run(action),
        Ok(Command::Bench(options)) => return bench::run(&options),
        Ok(Command::BenchIndex { json }) => return bench::index::run(json),
        Ok(command) => command,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };
    let opening = match &command {
        Command::Open(start) => match gui::pane_folders(start) {
            Ok(panes) => Opening { panes },
            Err(message) => {
                console::attach();
                eprintln!("navo: {message}\n{}", cli::USAGE);
                return ExitCode::from(2);
            }
        },
        _ => Opening {
            panes: [None, None],
        },
    };
    execute_command(command, opening, started, detect)
}

struct Opening {
    panes: [Option<Location>; 2],
}

fn execute_command(
    command: Command,
    opening: Opening,
    started: Instant,
    detect: impl FnOnce() -> Option<StoreDirs>,
) -> ExitCode {
    let clock = ProcessClock::detect(started, metrics::process_age);
    let profile = match &command {
        Command::Open(start) => profile::choose(start, detect),
        Command::CrashTest(root) => Ok(Profile::Folder(StoreDirs::in_profile(root))),
        _ => Ok(Profile::User(detect())),
    };
    let profile = match profile {
        Ok(profile) => profile,
        Err(error) => {
            eprintln!("navo: cannot create a temporary profile for --metrics: {error}");
            return ExitCode::FAILURE;
        }
    };
    match command {
        Command::Open(start) => open_window(start, (opening, clock), &profile, gui::open),
        Command::Version => {
            let _diagnostics = install_logging(&profile);
            println!("navo {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::DiagReport(path) => write_report(&path, started, &profile),
        Command::CrashTest(_) => crash_test(&profile),
        Command::PdfHelper => pdf_helper::run(),
        Command::IndexService(action) => index_service::run(action),
        Command::Bench(options) => bench::run(&options),
        Command::BenchIndex { json } => bench::index::run(json),
    }
}

#[inline(never)]
#[allow(clippy::panic, reason = "an opt-in CI probe exercises the panic hook")]
fn crash_test(profile: &Profile) -> ExitCode {
    if std::env::var_os("NAVO_CRASH_TEST").as_deref() != Some(std::ffi::OsStr::new("1")) {
        eprintln!("navo: --crash-test requires NAVO_CRASH_TEST=1");
        return ExitCode::from(2);
    }
    let diagnostics = install_logging(profile);
    crash::install(profile.dirs().map(|dirs| dirs.data.as_path()), &diagnostics);
    panic!("Navo CI crash test");
}

fn install_logging(profile: &Profile) -> logging::Diagnostics {
    let diagnostics = logging::install(profile.dirs().map(|dirs| dirs.data.as_path()));
    logging::log_start();
    metrics::mark("logging ready");
    diagnostics
}

fn write_report(path: &Path, started: Instant, profile: &Profile) -> ExitCode {
    let diagnostics = install_logging(profile);
    let data = profile.dirs().map(|dirs| dirs.data.as_path());
    match diag::write_on_demand(path, started, &diagnostics, data) {
        Ok(summary) => {
            println!("navo: wrote {} ({} bytes)", path.display(), summary.bytes);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("navo: cannot write {}: {error}", path.display());
            ExitCode::FAILURE
        }
    }
}

fn open_window(
    start: Start,
    opening: (Opening, ProcessClock),
    profile: &Profile,
    open: impl FnOnce(gui::Launch),
) -> ExitCode {
    let (Opening { panes }, clock) = opening;
    let (sender, requests) = open_requests();
    let (guard, mut splash) = match prepare_instance(&start, profile, sender) {
        Ok(ControlFlow::Continue(first)) => first,
        Ok(ControlFlow::Break(code)) => return code,
        Err(error) => {
            console::attach();
            eprintln!("navo: {error}");
            return ExitCode::FAILURE;
        }
    };
    let migrated = match migrate(&start, profile, guard.is_some(), &mut splash) {
        ControlFlow::Continue(report) => report,
        ControlFlow::Break(code) => return code,
    };
    let diagnostics = install_logging(profile);
    start.log_deprecated_flags();
    migrated.iter().for_each(migration::Report::log);
    install_reporters(profile, &diagnostics);
    let (pending_crash, running) = prepare_crash(profile, guard.as_ref());
    let store = check_store(profile, pending_crash.is_some(), &start);
    let (run, trace) = match instrumentation(&start, clock) {
        Ok(started) => started,
        Err(code) => return code,
    };
    open(gui::Launch {
        trace,
        experimental_retention: start.retention,
        path: start.path,
        view: start.view,
        panes,
        splash,
        dirs: profile.dirs().cloned(),
        cleanup: profile.cleanup(),
        start_watch: run.as_ref().map(metrics::Run::watch),
        instance_requests: requests,
        running,
        store,
        crash: crash_report::CrashStart {
            pending: pending_crash,
            started: Instant::now(),
            diagnostics,
            recovery: None,
        },
    });
    if let Err(error) = profile.cleanup().run() {
        eprintln!("navo: cannot remove the temporary profile: {error}");
        return ExitCode::FAILURE;
    }
    run.map_or(ExitCode::SUCCESS, |run| run.exit_code())
}

fn instrumentation(
    start: &Start,
    clock: ProcessClock,
) -> Result<
    (
        Option<metrics::Run>,
        Option<rusty_ui::window_view::InputTrace>,
    ),
    ExitCode,
> {
    let trace = start
        .trace
        .clone()
        .map(rusty_ui::window_view::InputTrace::new)
        .transpose()
        .map_err(|error| {
            console::attach();
            eprintln!("navo: cannot start input tracing: {error}");
            ExitCode::FAILURE
        })?;
    let run = start
        .metrics
        .clone()
        .map(|path| metrics::Run::new(path, clock));
    Ok((run, trace))
}

fn install_reporters(profile: &Profile, diagnostics: &logging::Diagnostics) {
    let data = profile.dirs().map(|dirs| dirs.data.as_path());
    crash::install(data, diagnostics);
    perf::install(Instant::now(), diagnostics, data);
}

fn check_store(profile: &Profile, unclean: bool, start: &Start) -> crash::store::StoreStart {
    crash::store::check_store(
        profile.dirs().map(|dirs| dirs.data.as_path()),
        unclean,
        start.retention,
    )
}

fn migrates(start: &Start, profile: &Profile, guarded: bool) -> bool {
    !start.is_metrics() && (guarded || matches!(profile, Profile::User(Some(_))))
}

fn migrate(
    start: &Start,
    profile: &Profile,
    guarded: bool,
    splash: &mut splash::Splash,
) -> ControlFlow<ExitCode, Option<migration::Report>> {
    if !migrates(start, profile, guarded) {
        return ControlFlow::Continue(None);
    }
    let report = migration::start(profile, &mut migration::Dialogs { splash });
    if matches!(report.settled, migration::Settled::Quit) {
        return ControlFlow::Break(ExitCode::FAILURE);
    }
    ControlFlow::Continue(Some(report))
}

fn prepare_crash(
    profile: &Profile,
    guard: Option<&instance::Guard>,
) -> (Option<crash::Pending>, Option<crash::Running>) {
    if guard.is_none() {
        return (None, None);
    }
    let Some(dirs) = profile.dirs() else {
        return (None, None);
    };
    let pending = crash::pending(&dirs.data);
    let running = match crash::Running::start(&dirs.data) {
        Ok(running) => Some(running),
        Err(error) => {
            tracing::warn!(%error, "cannot create the running marker");
            None
        }
    };
    (pending, running)
}

type First = (Option<instance::Guard>, splash::Splash);

fn first(guard: Option<instance::Guard>) -> ControlFlow<ExitCode, First> {
    let splash = splash::Splash::start();
    metrics::mark("splash started");
    ControlFlow::Continue((guard, splash))
}

fn prepare_instance(
    start: &Start,
    profile: &Profile,
    sender: OpenSender,
) -> Result<ControlFlow<ExitCode, First>, instance::Error> {
    prepare_instance_with_notice(start, profile, sender, console::show_mode_mismatch)
}

fn prepare_instance_with_notice(
    start: &Start,
    profile: &Profile,
    sender: OpenSender,
    show_notice: impl FnOnce(&str),
) -> Result<ControlFlow<ExitCode, First>, instance::Error> {
    if start.is_metrics() {
        return Ok(ControlFlow::Continue((None, splash::Splash::none())));
    }
    let Some(dirs) = profile.dirs() else {
        return Ok(first(None));
    };
    let Some(acquired) = instance::acquire(&dirs.data)? else {
        return Ok(first(None));
    };
    match acquired {
        instance::Acquired::First(mut guard) => {
            let retention = start.retention;
            guard.serve(move |request| {
                if request.experimental_retention != retention {
                    return instance::Reply::Error;
                }
                gui::on_instance_request(&sender, request)
            })?;
            Ok(first(Some(guard)))
        }
        instance::Acquired::Second(client) => {
            if start.trace.is_some() {
                console::attach();
                eprintln!("navo: close the running Navo before starting --trace");
                return Ok(ControlFlow::Break(ExitCode::FAILURE));
            }
            if start.legacy_retention_flag {
                let _diagnostics = install_logging(profile);
                start.log_deprecated_flags();
            }
            let request = instance::Request::from_start(start, &std::env::current_dir()?)?;
            let reply = client.send(&request, std::time::Duration::from_secs(2))?;
            let code = if reply == instance::Reply::Ok {
                ExitCode::SUCCESS
            } else {
                console::attach();
                eprintln!("navo: the running Navo rejected the request");
                show_notice(&rusty_l10n::tr("instance-mode-mismatch"));
                ExitCode::FAILURE
            };
            Ok(ControlFlow::Break(code))
        }
    }
}

#[cfg(test)]
mod tests;
