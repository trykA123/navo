//! Starts a user command and keeps its list file alive as long as the process.

#![allow(
    unsafe_code,
    reason = "checking whether a list file's process still runs needs Win32"
)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

use super::expand::{LIST_PATH_MARKER, MAX_COMMAND_LINE, Plan, command_line_units};
use super::model::Console;

mod dir;
mod sweep;

use self::dir::{list_dir, private_dir};
#[cfg(test)]
use self::sweep::sweep_with;
pub(super) use self::sweep::{Liveness, sweep};

const PENDING: &str = "pending";

/// Starts the command. The list file, when one is needed, is created before the spawn.
pub(super) fn start(
    plan: Plan,
    on_exit: Box<dyn FnOnce(Option<i32>) + Send>,
) -> Result<(), String> {
    start_with(plan, on_exit, |job| {
        std::thread::Builder::new()
            .name("navo-user-command-wait".to_owned())
            .spawn(job)
            .map(drop)
    })
}

type WaitJob = Box<dyn FnOnce() + Send>;

/// `spawn_waiter` runs before the child starts, so a child never lacks its waiter.
fn start_with(
    plan: Plan,
    on_exit: Box<dyn FnOnce(Option<i32>) + Send>,
    spawn_waiter: impl FnOnce(WaitJob) -> std::io::Result<()>,
) -> Result<(), String> {
    let list_path = prepare(&plan)?;
    let mut command = match command(&plan, list_path.as_deref()) {
        Ok(command) => command,
        Err(error) => {
            remove_list_if(list_path.as_deref());
            return Err(error);
        }
    };
    let (child_sender, child_receiver) = mpsc::channel::<Child>();
    let waiter_list = list_path.clone();
    let job: WaitJob = Box::new(move || {
        let Ok(mut child) = child_receiver.recv() else {
            return;
        };
        let code = child.wait().ok().and_then(|status| status.code());
        remove_list_if(waiter_list.as_deref());
        on_exit(code);
    });
    if let Err(error) = spawn_waiter(job) {
        remove_list_if(list_path.as_deref());
        return Err(format!("could not start the command: {error}"));
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            remove_list_if(list_path.as_deref());
            return Err(format!("could not start the command: {error}"));
        }
    };
    if let Some(path) = &list_path
        && let Err(error) = publish_pid(&sidecar(path), child.id())
    {
        tracing::warn!(%error, "could not publish the user command pid");
    }
    if child_sender.send(child).is_err() {
        tracing::warn!("the user command wait thread ended early");
    }
    Ok(())
}

fn remove_list_if(path: Option<&Path>) {
    if let Some(path) = path {
        remove_list(path);
    }
}

fn prepare(plan: &Plan) -> Result<Option<PathBuf>, String> {
    let list_path = match &plan.list {
        Some(paths) => {
            let dir = list_dir()?;
            Some(write_list(&dir, paths)?)
        }
        None => None,
    };
    if let Some(path) = &list_path
        && let Err(error) = create_pid(&sidecar(path))
    {
        remove_list(path);
        return Err(error);
    }
    Ok(list_path)
}

fn command(plan: &Plan, list_path: Option<&Path>) -> Result<Command, String> {
    let args = spawn_args(&plan.args, list_path)?;
    if command_line_units(&plan.program, &args) > MAX_COMMAND_LINE {
        return Err(
            "the command line is over 32,000 UTF-16 units; use `{marked-list}` for many files"
                .to_owned(),
        );
    }
    let mut command = Command::new(&plan.program);
    for arg in &args {
        command.arg(arg);
    }
    command
        .current_dir(&plan.start_in)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command, plan.console);
    Ok(command)
}

fn spawn_args(args: &[String], list_path: Option<&Path>) -> Result<Vec<String>, String> {
    let Some(list_path) = list_path else {
        if args.iter().any(|arg| arg.contains(LIST_PATH_MARKER)) {
            return Err("the marked list has no file".to_owned());
        }
        return Ok(args.to_vec());
    };
    let path = list_path.to_string_lossy();
    let replaced: Vec<String> = args
        .iter()
        .map(|arg| arg.replace(LIST_PATH_MARKER, &path))
        .collect();
    if replaced.iter().any(|arg| arg.contains(LIST_PATH_MARKER)) {
        return Err("the marked list path could not be inserted".to_owned());
    }
    Ok(replaced)
}

fn write_list(dir: &Path, paths: &[String]) -> Result<PathBuf, String> {
    if paths
        .iter()
        .any(|path| path.chars().any(|ch| ch == '\r' || ch == '\n'))
    {
        return Err("a list path contains a line break".to_owned());
    }
    private_dir(dir)?;
    let mut content = String::new();
    for path in paths {
        content.push_str(path);
        content.push_str("\r\n");
    }
    let mut file = tempfile::Builder::new()
        .prefix("list-")
        .suffix(".txt")
        .tempfile_in(dir)
        .map_err(|error| format!("could not create the list file: {error}"))?;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("could not write the list file: {error}"))?;
    file.flush()
        .map_err(|error| format!("could not write the list file: {error}"))?;
    let (file, path) = file
        .keep()
        .map_err(|error| format!("could not keep the list file: {error}"))?;
    drop(file);
    Ok(path)
}

fn create_pid(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "the pid sidecar has no folder".to_owned())?;
    let trusted = list_dir()?;
    if trusted.as_path() != parent {
        return Err("the pid sidecar is not in the list folder".to_owned());
    }
    private_dir(parent)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("could not create the pid sidecar: {error}"))?;
    file.write_all(PENDING.as_bytes())
        .map_err(|error| format!("could not write the pid sidecar: {error}"))?;
    file.flush()
        .map_err(|error| format!("could not write the pid sidecar: {error}"))?;
    Ok(())
}

fn publish_pid(path: &Path, pid: u32) -> Result<(), String> {
    publish_pid_with(path, pid, |from, to| std::fs::rename(from, to))
}

fn publish_pid_with(
    path: &Path,
    pid: u32,
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "the pid sidecar has no folder".to_owned())?;
    let mut temp = tempfile::Builder::new()
        .prefix(".pid-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|error| format!("could not create the pid temp file: {error}"))?;
    temp.write_all(pid.to_string().as_bytes())
        .map_err(|error| format!("could not write the pid temp file: {error}"))?;
    temp.flush()
        .map_err(|error| format!("could not flush the pid temp file: {error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("could not flush the pid temp file: {error}"))?;
    replace(temp.path(), path)
        .map_err(|error| format!("could not publish the pid sidecar: {error}"))
}

/// Whether a process from a pid sidecar still runs.
#[cfg(windows)]
pub(super) fn is_running(pid: u32) -> Liveness {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const STILL_ACTIVE: u32 = 259;
    // SAFETY: OpenProcess takes a process id and returns a new handle or null.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(code) if code == ERROR_INVALID_PARAMETER as i32 => Liveness::Gone,
            Some(code) if code == ERROR_ACCESS_DENIED as i32 => Liveness::Unknown,
            _ => Liveness::Unknown,
        };
    }
    // SAFETY: the handle was just opened and is owned here; wrapping it closes it once.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut code = 0;
    // SAFETY: the handle is live and code is writable output storage.
    let ok = unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) };
    if ok == 0 {
        Liveness::Unknown
    } else if code == STILL_ACTIVE {
        Liveness::Running
    } else {
        Liveness::Gone
    }
}

/// Whether a process from a pid sidecar still runs.
#[cfg(not(windows))]
pub(super) fn is_running(_pid: u32) -> Liveness {
    Liveness::Running
}

/// The Win32 creation flags for a shown or hidden console.
#[cfg(any(windows, test))]
pub(super) fn creation_flags(console: Console) -> u32 {
    let console_flag = match console {
        Console::Show => 0x0000_0010,
        Console::Hide => 0x0800_0000,
    };
    console_flag | 0x0000_0200
}

#[cfg(windows)]
fn detach(command: &mut Command, console: Console) {
    use std::os::windows::process::CommandExt as _;
    command.creation_flags(creation_flags(console));
}

#[cfg(unix)]
fn detach(command: &mut Command, _console: Console) {
    use std::os::unix::process::CommandExt as _;
    command.process_group(0);
}

fn sidecar(path: &Path) -> PathBuf {
    let mut file = path.as_os_str().to_owned();
    file.push(".pid");
    PathBuf::from(file)
}

fn remove_list(path: &Path) {
    remove_one(path);
    remove_one(&sidecar(path));
}

fn remove_one(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "could not remove a user command list file");
        }
    }
}

#[cfg(test)]
mod tests;
