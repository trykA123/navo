use std::io;
use std::time::Duration;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::ChangeLock;

#[cfg(windows)]
pub(super) fn global() -> io::Result<ChangeLock> {
    ChangeLock::acquire(Scope::Global, Duration::from_secs(60))?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "another index service change held the mutex for 60 seconds",
        )
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Global,
    Local,
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Self::Global => "Global\\NavoIndexServiceChange",
            Self::Local => "Local\\NavoIndexServiceChange",
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
mod tests;
