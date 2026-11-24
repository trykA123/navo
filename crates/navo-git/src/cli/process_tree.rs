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
    child: Option<Child>,
    #[cfg(windows)]
    job: Option<navo_shell::KillJob>,
    ticket: Option<Ticket>,
    status: Option<ExitStatus>,
    reaped: bool,
}

impl GitTree {
    #[cfg(unix)]
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        Ok(Self {
            child: Some(command.spawn()?),
            ticket: None,
            status: None,
            reaped: false,
        })
    }

    #[cfg(windows)]
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        let (child, job) = navo_shell::spawn_in_kill_job(command)?;
        Ok(Self {
            child: Some(child),
            ticket: None,
            status: None,
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
        match self.child.as_mut() {
            Some(child) => (child.stdout.take(), child.stderr.take()),
            None => (None, None),
        }
    }

    /// Polls the child; when it has exited, ends its job, closes its handle and marks the tree
    /// reaped. Returns the stored status on a later call.
    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let Some(child) = self.child.as_mut() else {
            return Ok(self.status);
        };
        let status = child.try_wait()?;
        if status.is_some() {
            let emptied = self.end_job()?;
            self.reaped = true;
            self.status = status;
            // The process handle goes before the ticket: a pause wakes only once the exited
            // child can no longer hold its folder open.
            self.child = None;
            if emptied {
                self.ticket = None;
            }
        }
        Ok(status)
    }

    /// Ends the Windows kill job and waits up to 1 s for every process in it to exit.
    /// Returns true when no process is left (always true without a job); fails when ending the job fails.
    fn end_job(&self) -> io::Result<bool> {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            return end_and_wait(job);
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
        let (descendants, emptied) = self.stop_descendants();
        let killed = match self.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(None) => child.kill().map_err(git_io),
                result => result.map(|_| ()).map_err(git_io),
            },
            None => Ok(()),
        };
        let waited = match self.child.as_mut() {
            Some(child) => child.wait().map(Some).map_err(git_io),
            None => Ok(None),
        };
        if let Ok(Some(status)) = &waited {
            self.status = Some(*status);
        }
        self.reaped = true;
        // Close the handle after every job process exited and before the ticket: a pause
        // wakes only once the stopped processes can no longer hold their folder open.
        self.child = None;
        if emptied {
            self.ticket = None;
        }
        descendants.and(killed).and(waited.map(|_| ()))
    }

    /// Ends the job and waits for its processes to exit, or kills the process tree when there
    /// is no job. The flag is false while a job process may still hold the folder.
    fn stop_descendants(&self) -> (Result<(), GitError>, bool) {
        #[cfg(windows)]
        if self.job.is_some() {
            let ended = self.end_job().map_err(git_io);
            let emptied = matches!(ended, Ok(true));
            return (ended.map(|_| ()), emptied);
        }
        match &self.child {
            Some(child) => (run_tree_kill(child.id()), true),
            None => (Ok(()), true),
        }
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

/// Terminates the job and waits up to one second for every process in it to fully exit.
/// Returns false, and reports it, when one is still exiting; the folder is then not free.
#[cfg(windows)]
fn end_and_wait(job: &navo_shell::KillJob) -> io::Result<bool> {
    let exited = job.terminate_and_wait(Duration::from_secs(1))?;
    if !exited {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "git job processes still exiting after 1 s"
        );
    }
    Ok(exited)
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
