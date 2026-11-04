#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line entry point reports to the terminal"
)]

use std::process::ExitCode;

use navo_index_service::Action;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    let extra = args.next();
    match (first.as_deref().and_then(|arg| arg.to_str()), extra) {
        (Some("run"), None) => navo_index_service::run(Action::Run),
        (Some("--version"), None) => {
            attach_console();
            println!("navo-index {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            attach_console();
            eprintln!("usage: navo-index [run | --version]");
            ExitCode::from(2)
        }
    }
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "attaching to the parent console requires Win32 FFI"
)]
fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: AttachConsole takes a process id and touches no memory we own; failure (no parent console) is harmless.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg(not(windows))]
fn attach_console() {}
