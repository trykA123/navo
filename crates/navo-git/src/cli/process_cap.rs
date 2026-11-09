#[cfg(test)]
use std::cell::RefCell;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::process::{ChildStderr, ChildStdout, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::GitError;

use super::process::{CommandSpec, base_command, command_io};
use super::process_tree::GitTree;
use super::process_watch::{
    Cancel, ChildSlot, Control, Outcome, ProcessOutcome, Watchdog, capture_stderr, stop_shared,
};
use super::running::{self, Ticket, Work};

#[cfg(test)]
type AfterReadersHook = Box<dyn FnOnce(&Option<Cancel>)>;

#[cfg(test)]
thread_local! {
    /// A one-shot hook that runs after the readers join in `Capture::finish`.
    static AFTER_READERS: RefCell<Option<AfterReadersHook>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessOptions {
    pub stdout_cap: usize,
    pub timeout: Duration,
}

#[derive(Debug)]
pub struct ProcessOutput {
    pub outcome: ProcessOutcome,
    pub status: Option<ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl ProcessOutput {
    pub fn capped(&self) -> bool {
        self.outcome == ProcessOutcome::Capped
    }
}

type Reader = JoinHandle<io::Result<Vec<u8>>>;

/// Installs a one-shot test hook that runs after the readers join in `Capture::finish`.
#[cfg(test)]
pub(super) fn hook_after_readers(hook: impl FnOnce(&Option<Cancel>) + 'static) {
    AFTER_READERS.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

/// Runs and clears the test hook installed for the current thread.
#[cfg(test)]
fn after_readers(stop: &Option<Cancel>) {
    let hook = AFTER_READERS.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook(stop);
    }
}

struct Capture {
    child: ChildSlot,
    stop: Option<Cancel>,
    watchdog: Option<Watchdog>,
    stdout: Option<Reader>,
    stderr: Option<Reader>,
    capped: Arc<AtomicBool>,
    outcome: Outcome,
}

pub(super) fn run_cancellable<S: AsRef<OsStr>>(
    spec: &CommandSpec<'_>,
    args: &[S],
    cancel: &Cancel,
    options: &ProcessOptions,
) -> Result<ProcessOutput, GitError> {
    if cancel.is_cancelled() {
        return Ok(ProcessOutput {
            outcome: ProcessOutcome::Cancelled,
            status: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        });
    }
    let ticket = spec
        .repo
        .map(|repo| running::register(repo, Work::Read))
        .transpose()?;
    let stop = ticket.as_ref().map(Ticket::stop_flag);
    if stop.as_ref().is_some_and(Cancel::is_cancelled) {
        return Err(GitError::Paused);
    }
    let mut command = base_command(spec);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = GitTree::spawn(&mut command).map_err(|error| command_io(spec.git, error))?;
    child.attach(ticket);
    let pipes = child.take_pipes();
    let (Some(stdout), Some(stderr)) = pipes else {
        let stopped = child.stop();
        if stop.as_ref().is_some_and(Cancel::is_cancelled) {
            return Err(GitError::Paused);
        }
        stopped?;
        return Err(GitError::SessionClosed);
    };
    let mut capture = Capture {
        child: Arc::new(Mutex::new(Some(child))),
        stop,
        watchdog: None,
        stdout: None,
        stderr: None,
        capped: Arc::new(AtomicBool::new(false)),
        outcome: Arc::new(Mutex::new(None)),
    };
    capture.start((stdout, stderr), cancel, options)?;
    capture.finish()
}

impl Capture {
    fn start(
        &mut self,
        pipes: (ChildStdout, ChildStderr),
        cancel: &Cancel,
        options: &ProcessOptions,
    ) -> Result<(), GitError> {
        let (stdout, stderr) = pipes;
        let capped = Arc::clone(&self.capped);
        let limit = options.stdout_cap;
        self.stdout = Some(
            thread::Builder::new()
                .name("git-capped-stdout".into())
                .spawn(move || capture_stdout(stdout, limit, &capped))
                .map_err(reader_io)?,
        );
        self.stderr = Some(
            thread::Builder::new()
                .name("git-capped-stderr".into())
                .spawn(move || capture_stderr(stderr))
                .map_err(reader_io)?,
        );
        self.watchdog = Some(Watchdog::start(
            Arc::clone(&self.child),
            options.timeout,
            Arc::clone(&self.outcome),
            Some(Control {
                cancel: cancel.clone(),
                capped: Arc::clone(&self.capped),
            }),
            self.stop.clone(),
        )?);
        Ok(())
    }

    fn finish(&mut self) -> Result<ProcessOutput, GitError> {
        let stdout = join_reader(self.stdout.take())?;
        let stderr = join_reader(self.stderr.take())?;
        if self.stop.as_ref().is_some_and(Cancel::is_cancelled) {
            return Err(GitError::Paused);
        }
        #[cfg(test)]
        after_readers(&self.stop);
        let status = self.wait()?;
        let reason = self.watchdog.take().and_then(Watchdog::finish);
        // A stop can land after the readers join and before the tree is reaped.
        if self.stop.as_ref().is_some_and(Cancel::is_cancelled) {
            return Err(GitError::Paused);
        }
        if let Some(Err(error)) = self
            .outcome
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            return Err(error);
        }
        if reason.is_none() {
            stop_shared(&self.child)?;
        }
        self.child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let outcome = reason.unwrap_or_else(|| {
            if self.capped.load(Ordering::Acquire) {
                ProcessOutcome::Capped
            } else {
                ProcessOutcome::Done
            }
        });
        Ok(ProcessOutput {
            outcome,
            status: Some(status),
            stdout,
            stderr,
        })
    }

    fn wait(&self) -> Result<ExitStatus, GitError> {
        loop {
            let status = self
                .child
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_mut()
                .map(GitTree::try_wait)
                .transpose()
                .map_err(reader_io)?
                .flatten();
            if let Some(status) = status {
                return Ok(status);
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.watchdog.take();
        let stopped = stop_shared(&self.child);
        let stdout = join_reader(self.stdout.take());
        let stderr = join_reader(self.stderr.take());
        if let Err(error) = stopped.and(stdout).and(stderr) {
            *self.outcome.lock().unwrap_or_else(PoisonError::into_inner) = Some(Err(error));
        }
    }
}

fn join_reader(handle: Option<Reader>) -> Result<Vec<u8>, GitError> {
    match handle {
        Some(handle) => handle
            .join()
            .map_err(|_| GitError::SessionClosed)?
            .map_err(reader_io),
        None => Ok(Vec::new()),
    }
}

fn capture_stdout(mut reader: impl Read, limit: usize, capped: &AtomicBool) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    while bytes.len() < limit {
        let keep = chunk.len().min(limit - bytes.len());
        match reader.read(&mut chunk[..keep]) {
            Ok(0) => return Ok(bytes),
            Ok(read) => bytes.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    capped.store(true, Ordering::Release);
    Ok(bytes)
}

fn reader_io(error: io::Error) -> GitError {
    GitError::Io {
        path: "git process".into(),
        kind: error.kind(),
    }
}
