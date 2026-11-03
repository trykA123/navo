use std::process::{Command, Stdio};
use std::sync::Arc;

use navo_core::Location;

use super::{Arg, Launch, LaunchError, Process};

pub(crate) type TerminalOpen = Arc<dyn Fn(&Location) -> Result<(), String> + Send + Sync>;
pub(crate) type UrlOpen = Arc<dyn Fn(String) + Send + Sync>;

/// How `Terminal` and `Url` launches run: the Settings terminal and the shell launch worker.
#[derive(Clone)]
pub(crate) struct Ports {
    pub terminal: TerminalOpen,
    pub url: UrlOpen,
}

/// Starts a launch. A process is waited on a worker thread, and `on_exit` runs when it ends.
/// Terminal and URL launches have no process to wait for, so `on_exit` is dropped.
pub(crate) fn start(
    launch: Launch,
    ports: &Ports,
    on_exit: Box<dyn FnOnce() + Send>,
) -> Result<(), LaunchError> {
    match launch {
        Launch::Process(process) => spawn(&process, on_exit),
        Launch::Terminal { repo } => {
            let folder =
                Location::new(repo).map_err(|error| LaunchError::Terminal(error.to_string()))?;
            (ports.terminal)(&folder).map_err(LaunchError::Terminal)
        }
        Launch::Url(url) => {
            (ports.url)(url);
            Ok(())
        }
    }
}

fn spawn(process: &Process, on_exit: Box<dyn FnOnce() + Send>) -> Result<(), LaunchError> {
    let mut child = command(process)
        .spawn()
        .map_err(|error| LaunchError::Spawn(error.to_string()))?;
    let waiter = std::thread::Builder::new()
        .name("navo-git-wait".to_owned())
        .spawn(move || {
            let _ = child.wait();
            on_exit();
        });
    if let Err(error) = waiter {
        tracing::warn!(%error, "could not start the Git tool wait thread");
    }
    Ok(())
}

fn command(process: &Process) -> Command {
    let mut command = Command::new(&process.program);
    for arg in &process.args {
        match arg {
            Arg::Plain(text) => command.arg(text),
            Arg::Raw(text) => raw_arg(&mut command, text),
        };
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command, process.console);
    command
}

#[cfg(windows)]
fn raw_arg<'a>(command: &'a mut Command, text: &str) -> &'a mut Command {
    use std::os::windows::process::CommandExt as _;
    command.raw_arg(text)
}

#[cfg(not(windows))]
fn raw_arg<'a>(command: &'a mut Command, text: &str) -> &'a mut Command {
    command.arg(text)
}

#[cfg(windows)]
fn detach(command: &mut Command, console: bool) {
    use std::os::windows::process::CommandExt as _;
    command.creation_flags(windows_flags(console));
}

#[cfg(unix)]
fn detach(command: &mut Command, _console: bool) {
    use std::os::unix::process::CommandExt as _;
    command.process_group(0);
}

/// CREATE_NEW_PROCESS_GROUP, plus CREATE_NO_WINDOW for programs that may own a console.
#[cfg(any(windows, test))]
pub(super) fn windows_flags(console: bool) -> u32 {
    let hidden = if console { 0x0800_0000 } else { 0 };
    hidden | 0x0000_0200
}
