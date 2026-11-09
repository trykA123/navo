// Navo Setup entry point: hidden tester commands, the version line, or exit 2.

#![allow(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "the command-line entry point prints results and errors"
)]

use std::ffi::OsString;
use std::process::ExitCode;

pub mod cli;
pub mod flow;
pub mod host;
pub mod marker;
pub mod mode;
#[cfg(test)]
mod mode_tests;
pub mod payload;
pub mod placement;
pub mod runner;
#[cfg(test)]
mod runner_fake;
#[cfg(windows)]
pub mod shortcuts;
pub mod syntax;

#[cfg(windows)]
pub mod probe;

pub mod upgrade;

#[cfg(feature = "tester")]
mod tester;

/// Runs Navo Setup with the process arguments.
fn main() -> ExitCode {
    if !restrict_dll_search() {
        eprintln!("navo-setup: cannot restrict DLL loading to System32");
        return ExitCode::from(5);
    }
    attach_console();
    run(std::env::args_os().skip(1).collect())
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
    let host: Option<&mut dyn crate::host::Host> = Some(&mut windows_host);
    #[cfg(not(windows))]
    let host: Option<&mut dyn crate::host::Host> = None;
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
