use std::ffi::OsStr;
use std::io::{self, Read};
use std::process::{ChildStdout, Output, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::{FilterNotice, GitError, WorktreeFinish};

use super::process::{CommandSpec, base_command, command_io};
use super::process_tree::GitTree;

use super::process_watch::{ChildSlot, Outcome, Watchdog, capture_stderr, stop_shared};

pub(super) struct FilteredStream {
    stdout: ChildStdout,
    child: ChildSlot,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
    watchdog: Option<Watchdog>,
    outcome: Outcome,
}

impl FilteredStream {
    pub(super) fn start<S: AsRef<OsStr>>(
        spec: &CommandSpec<'_>,
        args: &[S],
    ) -> Result<Self, GitError> {
        let mut command = base_command(spec);
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child =
            GitTree::spawn(&mut command).map_err(|error| command_io(spec.git, error))?;
        let pipes = child.take_pipes();
        let (Some(stdout), Some(stderr)) = pipes else {
            return Err(child.stop().err().unwrap_or(GitError::SessionClosed));
        };
        let mut stream = Self {
            stdout,
            child: Arc::new(Mutex::new(Some(child))),
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
        )?);
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

    fn complete(&mut self) -> Result<Output, GitError> {
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
        let stderr = match self.stderr.take() {
            Some(handle) => handle
                .join()
                .map_err(|_| GitError::SessionClosed)?
                .map_err(git_io)?,
            None => Vec::new(),
        };
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
        let stopped = if self.output().is_none() {
            stop_shared(&self.child)
        } else {
            Ok(())
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
