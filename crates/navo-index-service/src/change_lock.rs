//! Serialises elevated service changes through an app mutex or the machine lock file.

use std::io;
use std::time::Duration;

#[cfg(windows)]
mod file;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::ChangeLock;

#[cfg(windows)]
pub(super) fn machine(wait: Duration) -> io::Result<file::FileLock> {
    file::FileLock::acquire(wait)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "another index service change held change.lock until the wait ended",
        )
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Local,
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Self::Local => "Local\\NavoIndexServiceChange",
        }
    }
}

const FILE_POLL: Duration = Duration::from_millis(100);
const FILE_NOT_FOUND: i32 = 2;
const PATH_NOT_FOUND: i32 = 3;
const ACCESS_DENIED: i32 = 5;
const SHARING_VIOLATION: i32 = 32;
const LOCK_VIOLATION: i32 = 33;

/// A vanished or delete-pending parent, or another holder of the change lock, may retry.
fn retryable_lock_error(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(FILE_NOT_FOUND | PATH_NOT_FOUND | ACCESS_DENIED | SHARING_VIOLATION | LOCK_VIOLATION)
    )
}

/// Retries `open` until it succeeds, the decided error is not retryable, or `wait` ends.
#[expect(
    clippy::too_many_arguments,
    reason = "the lock retry loop keeps its open, retry, clock and pause inputs explicit"
)]
fn acquire_file<T>(
    wait: Duration,
    mut open: impl FnMut() -> io::Result<T>,
    retry: impl Fn(&io::Error) -> bool,
    mut elapsed: impl FnMut() -> Duration,
    mut pause: impl FnMut(Duration),
) -> io::Result<Option<T>> {
    loop {
        match open() {
            Ok(handle) => return Ok(Some(handle)),
            Err(error) if retry(&error) => {}
            Err(error) => return Err(error),
        }
        let remaining = wait.saturating_sub(elapsed());
        if remaining.is_zero() {
            return Ok(None);
        }
        pause(FILE_POLL.min(remaining));
        if elapsed() >= wait {
            return Ok(None);
        }
    }
}

enum WaitResult {
    Acquired,
    Abandoned,
    TimedOut,
}

trait Mutex {
    fn wait(&self, wait: Duration) -> io::Result<WaitResult>;
    fn release(&self) -> io::Result<()>;
}

struct Held<M: Mutex> {
    mutex: M,
}

impl<M: Mutex> Held<M> {
    fn acquire(mutex: M, wait: Duration) -> io::Result<Option<Self>> {
        match mutex.wait(wait)? {
            WaitResult::Acquired | WaitResult::Abandoned => Ok(Some(Self { mutex })),
            WaitResult::TimedOut => Ok(None),
        }
    }
}

impl<M: Mutex> Drop for Held<M> {
    fn drop(&mut self) {
        if let Err(error) = self.mutex.release() {
            tracing::error!(%error, "index service: releasing the change mutex failed");
        }
    }
}

pub(super) fn install_unless_current(
    current: impl FnOnce() -> io::Result<bool>,
    install: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    if current()? {
        return Ok(());
    }
    install()
}

#[cfg(test)]
mod file_tests;
#[cfg(test)]
mod tests;
