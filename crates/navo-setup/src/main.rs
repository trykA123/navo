// Navo Setup entry point: hidden tester commands, the version line, or exit 2.

#![allow(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "the command-line entry point prints results and errors"
)]

use std::ffi::OsString;
use std::process::ExitCode;

pub mod cli;
#[cfg(any(windows, test))]
pub mod default_root;
pub mod flow;
pub mod flow_outcome;
pub mod host;
pub mod licence;
pub mod marker;
pub mod mode;
#[cfg(test)]
mod mode_tests;
#[cfg(any(windows, test))]
mod parent_acl_rule;
pub mod payload;
pub mod placement;
pub mod progress_log;
pub mod runner;
#[cfg(test)]
mod runner_fake;
#[cfg(windows)]
pub mod shortcuts;
pub mod syntax;

#[cfg(windows)]
pub mod probe;

pub mod upgrade;
pub mod window;
pub mod window_core;
mod window_input;
#[cfg(test)]
mod window_input_tests;
#[cfg(test)]
mod window_keys_tests;
#[cfg(test)]
mod window_recovery_tests;
#[cfg(all(test, windows))]
mod window_start_tests;
#[cfg(test)]
mod window_tests;
pub mod window_view;
pub mod window_worker;
#[cfg(test)]
mod window_worker_tests;

#[cfg(feature = "tester")]
mod tester;

/// Runs Navo Setup with the process arguments.
fn main() -> ExitCode {
    if !restrict_dll_search() {
        eprintln!("navo-setup: cannot restrict DLL loading to System32");
        return ExitCode::from(5);
    }
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.is_empty() {
        return open_window();
    }
    attach_console();
    run(args)
}

/// Opens the wizard: reads the install state, starts the worker, then runs the window.
#[cfg(windows)]
fn open_window() -> ExitCode {
    use window_worker::{Probe, WindowsProbe};
    hide_own_console();
    let payload = payload::Payload::embedded().ok();
    let logs = payload::Logs::create().ok();
    let velopack_log = logs.as_ref().map_or_else(
        || std::env::temp_dir().join("navo-setup-velopack.log"),
        |logs| logs.velopack.clone(),
    );
    let default = cli::default_root().unwrap_or_default();
    let flow = window_core::initial_flow(&mut host::WindowsHost, payload.as_ref(), default);
    let facts = window_view::Facts {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        space_needed: payload.as_ref().map(|payload| {
            let space = payload.space();
            space
                .payload
                .saturating_mul(4)
                .max(space.velopack_requirement())
        }),
        wizard_log: logs.map(|logs| logs.wizard),
        log_paths: vec![velopack_log.clone()],
    };
    let make = || -> (Box<dyn host::Host>, Box<dyn Probe>) {
        (Box::new(host::WindowsHost), Box::new(WindowsProbe))
    };
    let link = match window_worker::spawn(payload, velopack_log, make) {
        Ok(link) => link,
        Err(error) => {
            eprintln!("navo-setup: cannot start the Setup worker: {error}");
            return ExitCode::from(5);
        }
    };
    match window::launch(window_core::Shell::new(flow, facts), link) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("navo-setup: {message}");
            ExitCode::from(5)
        }
    }
}

/// Refuses the window off Windows, where Setup has no host.
#[cfg(not(windows))]
fn open_window() -> ExitCode {
    eprintln!("navo-setup: the Setup window runs only on Windows");
    ExitCode::from(2)
}

/// Frees the console Windows made for a double-click; a console shared with a shell stays.
#[cfg(windows)]
fn hide_own_console() {
    use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
    let mut ids = [0u32; 2];
    // SAFETY: the buffer is valid for two u32 entries, the length passed.
    let count = unsafe { GetConsoleProcessList(ids.as_mut_ptr(), 2) };
    if count == 1 {
        // SAFETY: FreeConsole takes no arguments and only detaches this process.
        let _ = unsafe { FreeConsole() };
    }
}

/// Attaches to the parent console so silent-mode messages are visible (IW17).
#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: AttachConsole takes a process id and touches no memory we own; no console is fine.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

/// Does nothing off Windows, where there is no parent console.
#[cfg(not(windows))]
fn attach_console() {}

/// Limits later DLL loads, including delay-loaded imports, to System32 (IW24 DLL planting defence).
///
/// Setup runs from Downloads, where a planted DLL may sit beside the exe.
#[cfg(windows)]
fn restrict_dll_search() -> bool {
    use windows_sys::Win32::System::LibraryLoader::{
        LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
    };
    // SAFETY: the call takes a flag value only and touches no memory we own.
    unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) != 0 }
}

/// Reports success off Windows, where Setup never runs for real.
#[cfg(not(windows))]
fn restrict_dll_search() -> bool {
    true
}

/// Runs one invocation: a hidden tester probe, then IW17's command line.
fn run(args: Vec<OsString>) -> ExitCode {
    #[cfg(feature = "tester")]
    {
        if let Some(code) = tester::dispatch(&args) {
            return code;
        }
    }
    #[cfg(windows)]
    let logs = || {
        crate::payload::Logs::create().map(|logs| cli::LogPaths {
            wizard: logs.wizard,
            velopack: logs.velopack,
        })
    };
    #[cfg(not(windows))]
    let logs = || {
        let base = std::env::temp_dir();
        Ok(cli::LogPaths {
            wizard: base.join("navo-setup.log"),
            velopack: base.join("navo-setup-velopack.log"),
        })
    };
    #[cfg(windows)]
    let mut windows_host = crate::host::WindowsHost;
    #[cfg(windows)]
    let host: Option<&mut dyn crate::cli::CliHost> = Some(&mut windows_host);
    #[cfg(not(windows))]
    let host: Option<&mut dyn crate::cli::CliHost> = None;
    cli::run(&args, None, host, &logs)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::process::ExitCode;

    use super::run;

    #[test]
    fn unknown_arguments_exit_with_code_2() {
        assert_eq!(run(vec![OsString::from("--bogus")]), ExitCode::from(2));
    }

    #[test]
    fn an_empty_argument_list_exits_with_code_2() {
        assert_eq!(run(Vec::new()), ExitCode::from(2));
    }

    #[test]
    fn version_arguments_succeed() {
        assert_eq!(run(vec![OsString::from("--version")]), ExitCode::SUCCESS);
    }
}
