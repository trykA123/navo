//! Built-in terminal sessions: shell detection and shells running in a pseudo terminal.
//! Sessions run on background threads; the UI only writes, resizes and drops them.

mod backend;
mod pty;
pub mod shells;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use backend::{Backend, Sink};
#[cfg(any(test, feature = "test-support"))]
pub use backend::{FakeBackend, FakeHandle};
pub use pty::PtyBackend;
pub use shells::{ShellKind, ShellSpec};

/// Why a terminal operation failed.
#[derive(Debug, thiserror::Error)]
pub enum TermError {
    /// The shell's program does not exist; holds the shell's label.
    #[error("shell not found: {0}")]
    ShellMissing(String),
    /// The start folder does not exist (checked right before the shell starts).
    #[error("start folder not found: {}", .0.display())]
    CwdMissing(PathBuf),
    #[error("could not start the shell: {0}")]
    Spawn(#[source] io::Error),
    /// The shell is not reading its input fast enough; the bytes were not sent.
    #[error("terminal input is full; try again")]
    InputFull,
    #[error("terminal input or output failed: {0}")]
    Io(#[source] io::Error),
    #[error("the shell has exited")]
    Exited,
    #[error("terminal size {cols}x{rows} is out of range")]
    InvalidSize { cols: u16, rows: u16 },
}

/// The visible grid size in character cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl Size {
    /// ConPTY takes signed 16-bit sizes; zero is meaningless everywhere.
    const MAX: u16 = i16::MAX.unsigned_abs();

    pub(crate) fn check(self) -> Result<(), TermError> {
        let fits = |cells: u16| (1..=Self::MAX).contains(&cells);
        if fits(self.cols) && fits(self.rows) {
            Ok(())
        } else {
            Err(TermError::InvalidSize {
                cols: self.cols,
                rows: self.rows,
            })
        }
    }
}

/// One terminal session. Dropping it ends the shell without blocking the caller.
pub struct Session {
    backend: Box<dyn Backend>,
}

impl Session {
    /// Starts `shell` in a real pseudo terminal; output and the exit go to `sink`.
    pub fn spawn(
        shell: &ShellSpec,
        cwd: &Path,
        size: Size,
        sink: Arc<dyn Sink>,
    ) -> Result<Self, TermError> {
        let backend = PtyBackend::spawn(shell, cwd, size, sink)?;
        Ok(Self::with_backend(Box::new(backend)))
    }

    /// A session over any backend (a fake in tests).
    pub fn with_backend(backend: Box<dyn Backend>) -> Self {
        Self { backend }
    }

    /// Sends bytes to the shell as if typed.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), TermError> {
        self.backend.write(bytes)
    }

    /// Resizes the pseudo terminal; callers debounce and skip unchanged sizes.
    pub fn resize(&mut self, size: Size) -> Result<(), TermError> {
        self.backend.resize(size)
    }
}

#[cfg(test)]
mod tests;
