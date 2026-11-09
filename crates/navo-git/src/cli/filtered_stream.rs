#[cfg(test)]
use std::cell::RefCell;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::process::{ChildStdout, Output, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::{FilterNotice, GitError, WorktreeFinish};

use super::process::{CommandSpec, base_command, command_io};
use super::process_tree::GitTree;

use super::process_watch::{Cancel, ChildSlot, Outcome, Watchdog, capture_stderr, stop_shared};
use super::running::{self, Ticket, Work};

#[cfg(test)]
type AfterStartHook = Box<dyn FnOnce(&FilteredStream)>;
#[cfg(test)]
type AfterReapHook = Box<dyn FnOnce(Option<&Cancel>)>;

#[cfg(test)]
thread_local! {
    /// A one-shot hook that runs at the end of `FilteredStream::start`.
    static AFTER_START: RefCell<Option<AfterStartHook>> = const { RefCell::new(None) };
    /// A one-shot hook that runs after `complete` observes the exit.
    static AFTER_REAP: RefCell<Option<AfterReapHook>> = const { RefCell::new(None) };
}

/// Installs a one-shot test hook that runs at the end of `FilteredStream::start`.
#[cfg(test)]
fn hook_after_start(hook: impl FnOnce(&FilteredStream) + 'static) {
    AFTER_START.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

/// Runs and clears the test hook installed for the current thread.
#[cfg(test)]
fn after_start(stream: &FilteredStream) {
    let hook = AFTER_START.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook(stream);
    }
}

/// Installs a one-shot test hook that runs after `complete` observes the exit.
#[cfg(test)]
fn hook_after_reap(hook: impl FnOnce(Option<&Cancel>) + 'static) {
    AFTER_REAP.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

/// Runs and clears the test hook installed for the current thread.
#[cfg(test)]
fn after_reap(stop: Option<&Cancel>) {
    let hook = AFTER_REAP.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook(stop);
    }
}

pub(super) struct FilteredStream {
    stdout: ChildStdout,
    child: ChildSlot,
    stop: Option<Cancel>,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
    watchdog: Option<Watchdog>,
    outcome: Outcome,
}

impl FilteredStream {
    /// Starts a filtered `cat-file` stream and registers its read ticket.
    pub(super) fn start<S: AsRef<OsStr>>(
        spec: &CommandSpec<'_>,
        args: &[S],
    ) -> Result<Self, GitError> {
        let ticket = spec
            .repo
            .map(|repo| running::register(repo, Work::Read))
            .transpose()?;
        let stop = ticket.as_ref().map(Ticket::stop_flag);
        let mut command = base_command(spec);
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child =
            GitTree::spawn(&mut command).map_err(|error| command_io(spec.git, error))?;
        child.attach(ticket);
        let pipes = child.take_pipes();
        let (Some(stdout), Some(stderr)) = pipes else {
            return Err(child.stop().err().unwrap_or(GitError::SessionClosed));
        };
        let mut stream = Self {
            stdout,
            child: Arc::new(Mutex::new(Some(child))),
            stop,
            stderr: None,
            watchdog: None,
            outcome: Arc::new(Mutex::new(None)),
        };
        stream.stderr = Some(
            thread::Builder::new()
                .name("git-filter-stderr".to_owned())
                .spawn(move || capture_stderr(stderr))
                .map_err(|error| command_io(spec.git, error))?,
        );
        stream.watchdog = Some(Watchdog::start(
            Arc::clone(&stream.child),
            spec.timeout,
            Arc::clone(&stream.outcome),
            None,
            stream.stop.clone(),
        )?);
        #[cfg(test)]
        after_start(&stream);
        Ok(stream)
    }

    pub(super) fn output(&self) -> Option<Result<Output, GitError>> {
        self.outcome
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(super) fn finisher(&self) -> WorktreeFinish {
        let outcome = Arc::clone(&self.outcome);
        Box::new(move || {
            let result = outcome
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let output = result
                .ok_or_else(|| io::Error::other("Git reader has not reached EOF"))?
                .map_err(stream_error)?;
            check_status(&output)?;
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            Ok(if detail.is_empty() {
                Vec::new()
            } else {
                vec![FilterNotice::Filter { detail }]
            })
        })
    }

    /// Waits for the child, joins stderr, and returns the captured output.
    fn complete(&mut self) -> Result<Output, GitError> {
        if self.stop.as_ref().is_some_and(Cancel::is_cancelled) {
            return Err(GitError::Paused);
        }
        let status = loop {
            let status = self
                .child
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_mut()
                .map(GitTree::try_wait)
                .transpose()
                .map_err(git_io)?
                .flatten();
            if let Some(status) = status {
                break status;
            }
            thread::sleep(Duration::from_millis(5));
        };
        #[cfg(test)]
        after_reap(self.stop.as_ref());
        if self.stop.as_ref().is_some_and(Cancel::is_cancelled) {
            return Err(GitError::Paused);
        }
        let stderr = match self.stderr.take() {
            Some(handle) => handle
                .join()
                .map_err(|_| GitError::SessionClosed)?
                .map_err(git_io)?,
            None => Vec::new(),
        };
        if self.stop.as_ref().is_some_and(Cancel::is_cancelled) {
            return Err(GitError::Paused);
        }
        if self.watchdog.as_ref().is_some_and(Watchdog::timed_out) {
            return Err(GitError::Timeout);
        }
        Ok(Output {
            status,
            stdout: Vec::new(),
            stderr,
        })
    }

    fn read_chunk(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self.stdout.read(buffer) {
            Ok(0) => {
                let result = self.complete();
                *self.outcome.lock().unwrap_or_else(PoisonError::into_inner) = Some(result.clone());
                check_status(&result.map_err(stream_error)?)?;
                Ok(0)
            }
            result => result,
        }
    }
}

impl Read for FilteredStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.stop.as_ref().is_some_and(Cancel::is_cancelled) {
            let mut outcome = self.outcome.lock().unwrap_or_else(PoisonError::into_inner);
            if outcome.is_none() {
                *outcome = Some(Err(GitError::Paused));
            }
            return Err(stream_error(GitError::Paused));
        }
        if let Some(output) = self.output() {
            check_status(&output.map_err(stream_error)?)?;
            return Ok(0);
        }
        if let Some(watchdog) = &self.watchdog {
            watchdog.set_active(true);
        }
        let result = self.read_chunk(buffer);
        if let Some(watchdog) = &self.watchdog {
            watchdog.set_active(false);
            if watchdog.timed_out() {
                *self.outcome.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some(Err(GitError::Timeout));
                return Err(stream_error(GitError::Timeout));
            }
        }
        result
    }
}

impl Drop for FilteredStream {
    fn drop(&mut self) {
        self.watchdog.take();
        let stopped = if matches!(self.output(), Some(Ok(_))) {
            Ok(())
        } else {
            stop_shared(&self.child)
        };
        let joined = self
            .stderr
            .take()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| GitError::SessionClosed)?
                    .map_err(git_io)
            })
            .transpose();
        if let Err(error) = stopped.and(joined.map(|_| ())) {
            *self.outcome.lock().unwrap_or_else(PoisonError::into_inner) = Some(Err(error));
        }
    }
}

fn check_status(output: &Output) -> io::Result<()> {
    if output.status.success() {
        return Ok(());
    }
    Err(io::Error::other(GitError::Command {
        args: "cat-file --filters".to_owned(),
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    }))
}

fn stream_error(error: GitError) -> io::Error {
    let kind = if matches!(error, GitError::Timeout) {
        io::ErrorKind::TimedOut
    } else {
        io::ErrorKind::Other
    };
    io::Error::new(kind, error)
}

fn git_io(error: io::Error) -> GitError {
    GitError::Io {
        path: "git filter".into(),
        kind: error.kind(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use crate::{CliGit, GitError, GitPath, GitSource};

    use super::{hook_after_reap, hook_after_start};

    /// The Git program the test starts.
    fn git_program() -> &'static str {
        if cfg!(windows) { "git.exe" } else { "git" }
    }

    /// Creates a repository with one committed file and returns its parent folder.
    fn repo() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        fs::create_dir(&path).unwrap();
        run_git(&path, &["init", "-q"]);
        fs::write(path.join("data.bin"), b"payload\n").unwrap();
        run_git(&path, &["add", "-A"]);
        run_git(
            &path,
            &[
                "-c",
                "user.name=Navo",
                "-c",
                "user.email=navo@example.invalid",
                "commit",
                "-qm",
                "one",
            ],
        );
        temp
    }

    /// Runs one Git command and asserts that it succeeded.
    fn run_git(repo: &std::path::Path, args: &[&str]) {
        let output = Command::new(git_program())
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
    }

    /// A stop between the start and the first read still returns Paused.
    #[test]
    fn a_stop_after_start_returns_paused() {
        let temp = repo();
        let repo = temp.path().join("repo");
        let cli = CliGit::new(&repo).unwrap();
        hook_after_start(|stream| {
            stream
                .stop
                .as_ref()
                .expect("the stream has a registry ticket")
                .cancel();
        });
        let result = cli.read_blob_worktree_form("HEAD", &GitPath::from("data.bin"));
        assert!(matches!(result, Err(GitError::Paused)));
    }

    /// A stop that lands after the filter reap still returns Paused.
    #[test]
    fn a_stop_after_the_filter_reap_returns_paused() {
        let temp = repo();
        let repo = temp.path().join("repo");
        let cli = CliGit::new(&repo).unwrap();
        hook_after_reap(|stop| stop.expect("the stream has a registry ticket").cancel());
        let result = cli.read_blob_worktree_form("HEAD", &GitPath::from("missing.bin"));
        assert!(matches!(result, Err(GitError::Paused)));
    }
}
