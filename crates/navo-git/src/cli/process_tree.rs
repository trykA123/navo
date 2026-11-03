use std::io;
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::GitError;

/// A spawned git and every process it starts; `stop` ends all of them.
///
/// Unix puts the tree in its own process group. Windows puts it in a job object before git
/// runs, and falls back to `taskkill /T` when Windows refuses the job.
pub(super) struct GitTree {
    child: Child,
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
        })
    }

    #[cfg(windows)]
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        let (child, job) = navo_shell::spawn_in_kill_job(command)?;
        Ok(Self { child, job })
    }

    pub(super) fn take_pipes(&mut self) -> (Option<ChildStdout>, Option<ChildStderr>) {
        (self.child.stdout.take(), self.child.stderr.take())
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Kills the descendants, then the git process, and reaps it.
    pub(super) fn stop(&mut self) -> Result<(), GitError> {
        let descendants = self.stop_descendants();
        let killed = match self.child.try_wait().map_err(git_io) {
            Ok(None) => self.child.kill().map_err(git_io),
            result => result.map(|_| ()),
        };
        let waited = self.child.wait().map_err(git_io);
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
