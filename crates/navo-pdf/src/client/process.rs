//! The helper process as a trait object, so `navo-pdf` does not own `std::process::Child` and a
//! caller can create the process its own way (S2a0; `navo-shell` spawns it confined later).

use std::io::{self, Read, Write};
#[cfg(windows)]
use std::os::windows::io::{AsHandle, BorrowedHandle};
use std::process::Child;

/// A running helper process that the client can kill, poll and share handles into.
pub trait HelperProcess: Send {
    /// Ends the process; returns `Ok` when it has already exited.
    fn kill(&mut self) -> io::Result<()>;

    /// Checks without blocking whether the process has exited (`true` = exited).
    fn try_wait(&mut self) -> io::Result<bool>;

    /// The live process handle, with at least `PROCESS_DUP_HANDLE`; the target of a handle share.
    #[cfg(windows)]
    fn process_handle(&self) -> BorrowedHandle<'_>;
}

impl HelperProcess for Child {
    /// Calls `Child::kill`, which already answers `Ok` for an exited child.
    fn kill(&mut self) -> io::Result<()> {
        Child::kill(self)
    }

    /// Maps `Child::try_wait` to whether an exit status exists.
    fn try_wait(&mut self) -> io::Result<bool> {
        Child::try_wait(self).map(|status| status.is_some())
    }

    /// The `Child`'s own process handle, which has full access.
    #[cfg(windows)]
    fn process_handle(&self) -> BorrowedHandle<'_> {
        self.as_handle()
    }
}

/// A started helper: its process, the pipe ends to its stdin and stdout, and an optional guard
/// (for example the Job handle) that the client drops after the pipes.
pub struct Spawned {
    pub process: Box<dyn HelperProcess>,
    pub input: Box<dyn Write + Send>,
    pub output: Box<dyn Read + Send>,
    pub guard: Option<Box<dyn Send>>,
}
