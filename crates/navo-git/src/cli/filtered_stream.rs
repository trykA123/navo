use std::ffi::OsStr;
use std::io::{self, Read};
use std::process::{Child, ChildStderr, ChildStdout, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::{FilterNotice, GitError, WorktreeFinish};

use super::process::{CommandSpec, base_command, command_io};

const STDERR_CAPACITY: usize = 8 * 1024;
type Outcome = Arc<Mutex<Option<Result<Output, GitError>>>>;
type ChildSlot = Arc<Mutex<Option<Child>>>;

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
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .map_err(|error| command_io(spec.git, error))?;
        let pipes = (child.stdout.take(), child.stderr.take());
        let (Some(stdout), Some(stderr)) = pipes else {
            return Err(stop_child(&mut child)
                .err()
                .unwrap_or(GitError::SessionClosed));
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
                .map(Child::try_wait)
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

struct Watchdog {
    activity: Arc<Activity>,
    handle: Option<JoinHandle<Result<(), GitError>>>,
    outcome: Outcome,
}

struct Activity {
    state: Mutex<Idle>,
    changed: Condvar,
    timed_out: AtomicBool,
}

#[derive(Default)]
struct Idle {
    since: Option<Instant>,
    stopped: bool,
}

impl Watchdog {
    fn start(child: ChildSlot, timeout: Duration, outcome: Outcome) -> Result<Self, GitError> {
        let activity = Arc::new(Activity {
            state: Mutex::new(Idle::default()),
            changed: Condvar::new(),
            timed_out: AtomicBool::new(false),
        });
        let watched = Arc::clone(&activity);
        let handle = thread::Builder::new()
            .name("git-filter-watchdog".to_owned())
            .spawn(move || watch(watched, child, timeout))
            .map_err(git_io)?;
        Ok(Self {
            activity,
            handle: Some(handle),
            outcome,
        })
    }

    fn set_active(&self, active: bool) {
        self.activity
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .since = active.then(Instant::now);
        self.activity.changed.notify_one();
    }

    fn timed_out(&self) -> bool {
        self.activity.timed_out.load(Ordering::Acquire)
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.activity
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stopped = true;
        self.activity.changed.notify_one();
        if let Some(handle) = self.handle.take() {
            let result = handle
                .join()
                .map_err(|_| GitError::SessionClosed)
                .and_then(|result| result);
            if let Err(error) = result {
                *self.outcome.lock().unwrap_or_else(PoisonError::into_inner) = Some(Err(error));
            }
        }
    }
}

fn watch(activity: Arc<Activity>, child: ChildSlot, timeout: Duration) -> Result<(), GitError> {
    let mut state = activity
        .state
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    loop {
        if state.stopped {
            return Ok(());
        }
        let Some(since) = state.since else {
            state = activity
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
            continue;
        };
        let remaining = timeout.saturating_sub(since.elapsed());
        if remaining.is_zero() {
            activity.timed_out.store(true, Ordering::Release);
            drop(state);
            return stop_shared(&child);
        }
        state = activity
            .changed
            .wait_timeout(state, remaining)
            .unwrap_or_else(PoisonError::into_inner)
            .0;
    }
}

fn capture_stderr(mut stderr: ChildStderr) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(STDERR_CAPACITY);
    let mut chunk = [0; 4096];
    loop {
        match stderr.read(&mut chunk) {
            Ok(0) => return Ok(bytes),
            Ok(read) => {
                let keep = read.min(STDERR_CAPACITY - bytes.len());
                bytes.extend_from_slice(&chunk[..keep]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
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

fn stop_shared(child: &ChildSlot) -> Result<(), GitError> {
    let owned = child.lock().unwrap_or_else(PoisonError::into_inner).take();
    let Some(mut owned) = owned else {
        return Ok(());
    };
    let result = stop_child(&mut owned);
    *child.lock().unwrap_or_else(PoisonError::into_inner) = Some(owned);
    result
}

fn stop_child(child: &mut Child) -> Result<(), GitError> {
    let descendants = stop_descendants(child.id());
    let killed = match child.try_wait().map_err(git_io) {
        Ok(None) => child.kill().map_err(git_io),
        result => result.map(|_| ()),
    };
    let waited = child.wait().map_err(git_io);
    descendants.and(killed).and(waited.map(|_| ()))
}

fn stop_descendants(pid: u32) -> Result<(), GitError> {
    let mut command = tree_kill(pid);
    let mut killer = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(git_io)?;
    let started = std::time::Instant::now();
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
