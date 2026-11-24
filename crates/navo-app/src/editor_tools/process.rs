//! Spawns a checked editor tool with piped stdio, feeds stdin and drains stdout and stderr on
//! threads, and kills the whole process tree when the tool exits or the timeout passes.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use super::{ToolError, ToolOutput, ToolRequest};

/// How often the wait loop checks whether the tool has exited.
const POLL: Duration = Duration::from_millis(10);

/// One captured stream: the kept bytes and whether more was dropped.
type Captured = (Vec<u8>, bool);

/// A running tool and the handle that kills its whole tree.
struct Running {
    child: Child,
    #[cfg(windows)]
    job: navo_shell::KillJob,
}

/// Runs `exe` with `req`'s argv, cwd and stdin, keeping at most `cap` bytes per stream.
/// Fails with `Spawn` when the OS refuses the process, `Timeout` past `req.timeout`. Every
/// process the tool left behind is killed before this returns.
pub(super) fn run(exe: &Path, req: &ToolRequest, cap: usize) -> Result<ToolOutput, ToolError> {
    let deadline = Instant::now() + req.timeout;
    let mut command = Command::new(exe);
    command
        .args(&req.argv)
        .current_dir(&req.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut running = spawn(&mut command)?;
    if let Some(stdin) = running.child.stdin.take() {
        feed(stdin, req.stdin.clone());
    }
    let stdout = running.child.stdout.take().map(|pipe| drain(pipe, cap));
    let stderr = running.child.stderr.take().map(|pipe| drain(pipe, cap));
    let Some(status) = wait_until(&mut running, deadline) else {
        return Err(kill(running));
    };
    // A grandchild may still hold a pipe open, so the readers share the same deadline.
    let (Some(out), Some(err)) = (collect(stdout, deadline), collect(stderr, deadline)) else {
        return Err(kill(running));
    };
    kill_tree(&running);
    Ok(ToolOutput {
        status: status.code(),
        truncated: out.1 || err.1,
        stdout: out.0,
        stderr: err.0,
    })
}

/// Starts the command in its own process group, so a timeout can kill its children too.
#[cfg(unix)]
fn spawn(command: &mut Command) -> Result<Running, ToolError> {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    let child = command
        .spawn()
        .map_err(|error| ToolError::Spawn(error.to_string()))?;
    Ok(Running { child })
}

/// Starts the command without a console window inside a kill-on-close Job Object; fails with
/// `Spawn` when the OS refuses the process or the Job Object.
#[cfg(windows)]
fn spawn(command: &mut Command) -> Result<Running, ToolError> {
    let (mut child, job) = navo_shell::spawn_in_kill_job(command)
        .map_err(|error| ToolError::Spawn(error.to_string()))?;
    let job = require_job(&mut child, job)?;
    Ok(Running { child, job })
}

/// Returns the Job Object confining `child`; without one, kills and reaps the child and fails
/// with `Spawn`, so no tool runs where its children could outlive a timeout.
#[cfg(any(windows, test))]
pub(super) fn require_job<J>(child: &mut Child, job: Option<J>) -> Result<J, ToolError> {
    job.ok_or_else(|| {
        child.kill().unwrap_or_default();
        child.wait().map(drop).unwrap_or_default();
        ToolError::Spawn("the tool could not be placed in a Job Object".to_owned())
    })
}

/// Writes `bytes` to the tool's stdin on a thread, then closes it; a tool that exits without
/// reading makes the write fail, which is fine.
fn feed(mut stdin: ChildStdin, bytes: Vec<u8>) {
    thread::spawn(move || {
        stdin.write_all(&bytes).unwrap_or_default();
    });
}

/// Reads `pipe` to its end on a thread, keeping the first `cap` bytes; the result arrives on
/// the returned channel.
fn drain(mut pipe: impl Read + Send + 'static, cap: usize) -> Receiver<Captured> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut truncated = false;
        let mut buffer = [0_u8; 64 * 1024];
        while let Ok(read) = pipe.read(&mut buffer) {
            if read == 0 {
                break;
            }
            let room = cap.saturating_sub(kept.len());
            kept.extend_from_slice(&buffer[..read.min(room)]);
            truncated |= read > room;
        }
        sender.send((kept, truncated)).unwrap_or_default();
    });
    receiver
}

/// Polls the tool until it exits (its status) or `deadline` passes (`None`).
fn wait_until(running: &mut Running, deadline: Instant) -> Option<ExitStatus> {
    loop {
        match running.child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            _ => return None,
        }
    }
}

/// Waits for a reader's bytes until `deadline`; `None` when it is late. A missing pipe
/// counts as empty.
fn collect(reader: Option<Receiver<Captured>>, deadline: Instant) -> Option<Captured> {
    let Some(reader) = reader else {
        return Some((Vec::new(), false));
    };
    reader
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
}

/// Kills the tool and every process it started, reaps it so no zombie stays, and returns
/// the `Timeout` error.
fn kill(mut running: Running) -> ToolError {
    kill_tree(&running);
    running.child.kill().unwrap_or_default();
    running.child.wait().map(drop).unwrap_or_default();
    ToolError::Timeout
}

/// Sends SIGKILL to the tool's process group; a group with no process left is a no-op.
#[cfg(unix)]
fn kill_tree(running: &Running) {
    if let Ok(pid) = libc::pid_t::try_from(running.child.id()) {
        // SAFETY: kill takes no pointers; a negative pid names the group this tool leads.
        unsafe { libc::kill(-pid, libc::SIGKILL) };
    }
}

/// Terminates every process in the tool's Job Object.
#[cfg(windows)]
fn kill_tree(running: &Running) {
    running.job.terminate().unwrap_or_default();
}
