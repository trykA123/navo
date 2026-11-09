use std::io;
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::GitError;

use super::running::Ticket;

/// A spawned git and every process it starts; `stop` ends all of them.
///
/// Unix puts the tree in its own process group. Windows puts it in a job object before git
/// runs, and falls back to `taskkill /T` when Windows refuses the job.
///
/// On Unix a grandchild that calls `setsid` or `setpgid` escapes the group. Plain reads accept
/// that: fsmonitor is off and `GIT_OPTIONAL_LOCKS=0` is set.
///
/// A normal exit reaps the tree at once, so on Unix background descendants are not killed; their
/// process group id could be reused. Windows still terminates the job object.
pub(super) struct GitTree {
    child: Child,
    ticket: Option<Ticket>,
    reaped: bool,
    #[cfg(windows)]
    job: Option<navo_shell::KillJob>,
}

impl GitTree {
    #[cfg(unix)]
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        Ok(Self {
            child: command.spawn()?,
            ticket: None,
            reaped: false,
        })
    }

    #[cfg(windows)]
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        let (child, job) = navo_shell::spawn_in_kill_job(command)?;
        Ok(Self {
            child,
            ticket: None,
            reaped: false,
            job,
        })
    }

    /// Attaches the registry ticket, or clears it for a child with no repository.
    pub(super) fn attach(&mut self, ticket: Option<Ticket>) {
        self.ticket = ticket;
    }

    /// Takes the child's stdout and stderr pipes for the reader threads.
    pub(super) fn take_pipes(&mut self) -> (Option<ChildStdout>, Option<ChildStderr>) {
        (self.child.stdout.take(), self.child.stderr.take())
    }

    /// Polls the child; when it has exited, drops its registry ticket and marks the tree reaped.
    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        if status.is_some() {
            // On Unix a reaped group id can be reused; Windows ends the job before the ticket.
            let emptied = self.end_job()?;
            self.reaped = true;
            if emptied {
                self.ticket = None;
            }
        }
        Ok(status)
    }

    /// Ends the Windows kill job and waits up to 1 s for it to empty.
    /// Returns true when no process is left (always true without a job); fails when ending the job fails.
    fn end_job(&self) -> io::Result<bool> {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate()?;
            return Ok(wait_for_empty_job(job));
        }
        Ok(true)
    }

    /// Kills the descendants, then the git process, and reaps it; a reaped tree still ends its job.
    pub(super) fn stop(&mut self) -> Result<(), GitError> {
        if self.reaped {
            let result = self.end_job().map_err(git_io);
            if matches!(result, Ok(true)) {
                self.ticket = None;
            }
            return result.map(|_| ());
        }
        let descendants = self.stop_descendants();
        let killed = match self.child.try_wait().map_err(git_io) {
            Ok(None) => self.child.kill().map_err(git_io),
            result => result.map(|_| ()),
        };
        let waited = self.child.wait().map_err(git_io);
        self.reaped = true;
        #[cfg(windows)]
        let emptied = self.job.as_ref().is_none_or(wait_for_empty_job);
        #[cfg(not(windows))]
        let emptied = true;
        if emptied {
            self.ticket = None;
        }
        descendants.and(killed).and(waited.map(|_| ()))
    }

    fn stop_descendants(&self) -> Result<(), GitError> {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            return job.terminate().map_err(git_io);
        }
        run_tree_kill(self.child.id())
    }
}

#[cfg(all(test, windows))]
impl GitTree {
    /// How many processes the job still holds; zero when Windows refused a job.
    pub(super) fn active_processes(&self) -> io::Result<u32> {
        self.job
            .as_ref()
            .map_or(Ok(0), navo_shell::KillJob::active_processes)
    }
}

impl Drop for GitTree {
    /// Stops a live tree on drop; `stop` is idempotent and bounded, so a panic leaves no group.
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Waits up to one second for the terminated job to lose every active process.
#[cfg(windows)]
fn wait_for_empty_job(job: &navo_shell::KillJob) -> bool {
    let started = Instant::now();
    loop {
        match job.active_processes() {
            Ok(0) => return true,
            Ok(active) => {
                if started.elapsed() >= Duration::from_secs(1) {
                    log_still_running(active);
                    return false;
                }
            }
            Err(error) => {
                log_query_failure(&error);
                return false;
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Reports a job whose descendants outlived the one-second wait.
#[cfg(windows)]
fn log_still_running(active: u32) {
    use std::io::Write;
    let _ = writeln!(
        std::io::stderr(),
        "git job still has {active} active processes after 1 s"
    );
}

/// Reports a job accounting query that could not be read.
#[cfg(windows)]
fn log_query_failure(error: &io::Error) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "git job accounting failed: {error}");
}

fn run_tree_kill(pid: u32) -> Result<(), GitError> {
    let mut command = tree_kill(pid);
    let mut killer = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(git_io)?;
    let started = Instant::now();
    while killer.try_wait().map_err(git_io)?.is_none() {
        if started.elapsed() >= Duration::from_secs(1) {
            killer.kill().map_err(git_io)?;
            killer.wait().map_err(git_io)?;
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

#[cfg(unix)]
fn tree_kill(pid: u32) -> Command {
    let mut command = Command::new("kill");
    command.args(["-KILL", "--", &format!("-{pid}")]);
    command
}

#[cfg(windows)]
fn tree_kill(pid: u32) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("taskkill.exe");
    command.args(["/F", "/T", "/PID", &pid.to_string()]);
    command.creation_flags(0x0800_0000);
    command
}

fn git_io(error: io::Error) -> GitError {
    GitError::Io {
        path: "git process".into(),
        kind: error.kind(),
    }
}
