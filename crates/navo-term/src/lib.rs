//! Built-in terminal sessions: shell detection, shells running in a pseudo terminal and the grid.
//! Sessions run on background threads; the UI only writes, resizes and drops them.

mod backend;
pub mod grid;
pub mod prompt;
mod pty;
pub mod shells;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use backend::{Backend, ReplyFn, Sink};
#[cfg(any(test, feature = "test-support"))]
pub use backend::{FakeBackend, FakeHandle};
pub use prompt::{MarkKind, Nonce, PromptMark, PromptSink, PromptState, PtyWriter, SyncWait};
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
    writer: Arc<PtyWriter>,
}

impl Session {
    /// Starts `shell` in a real pseudo terminal; output and the exit go to `sink`.
    pub fn spawn(
        shell: &ShellSpec,
        cwd: &Path,
        size: Size,
        sink: Arc<dyn Sink>,
    ) -> Result<Self, TermError> {
        let nonce = Nonce::generate()?;
        let hook = prompt::ShellHook::for_shell(shell, &nonce);
        let trusted_nonce = hook.is_some().then_some(nonce);
        let writer =
            PtyBackend::spawn_session(shell, cwd, size, sink, trusted_nonce, hook.as_ref())?;
        Ok(Self::with_writer(writer))
    }

    /// A session over any backend (a fake in tests) without shell prompt integration.
    pub fn with_backend(backend: Box<dyn Backend>) -> Self {
        Self::with_writer(Arc::new(PtyWriter::new(ShellKind::Other, None, backend)))
    }

    /// A session backed by an already-configured serialized writer (used in tests and custom wiring).
    pub fn with_writer(writer: Arc<PtyWriter>) -> Self {
        Self { writer }
    }

    /// Sends bytes to the shell as if typed.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), TermError> {
        self.writer.write_user(bytes)
    }

    /// Resizes the pseudo terminal; callers debounce and skip unchanged sizes.
    pub fn resize(&mut self, size: Size) -> Result<(), TermError> {
        self.writer.resize(size)
    }

    /// Manually syncs the shell's working directory to `folder` if at a clean trusted prompt,
    /// queues `folder` as the newest pending target if busy/typed, or returns `NeedsIntegration`/`UnusualName`.
    pub fn sync_cwd(&self, folder: &Path) -> Result<Option<SyncWait>, TermError> {
        self.writer.sync_cwd(folder)
    }

    /// Why syncing to `folder` would wait right now, or `None` if it can write immediately.
    pub fn sync_reason(&self, folder: &Path) -> Option<SyncWait> {
        self.writer.sync_reason(folder)
    }

    /// Current trusted prompt state of the shell.
    pub fn prompt(&self) -> PromptState {
        self.writer.prompt_state()
    }

    /// Recorded OSC 133 marks anchored to grid lines for command blocks.
    pub fn marks(&self) -> Vec<PromptMark> {
        self.writer.marks()
    }

    /// Exit code reported by the latest `133;D` mark, if any.
    pub fn last_exit_code(&self) -> Option<i32> {
        self.writer.last_exit_code()
    }

    /// Shared handle to the session's single serialized writer.
    pub fn writer(&self) -> Arc<PtyWriter> {
        Arc::clone(&self.writer)
    }
}

#[cfg(test)]
mod tests;
