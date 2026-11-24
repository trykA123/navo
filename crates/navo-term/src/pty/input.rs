//! The input path: a bounded queue to a writer thread, so typing never waits for the shell.
//! A full queue refuses the write; a write blocked on the shell never holds up the close.

use std::io::{self, Write};
use std::sync::mpsc::{self, SyncSender, TrySendError};

use super::run_thread;
use crate::TermError;

/// Writes waiting for the shell; keystrokes and pastes are one write each.
const INPUT_QUEUE: usize = 64;

/// The session's side of the writer thread. Dropping it lets the thread end once its current
/// write returns; nothing waits for that.
pub(super) struct Input {
    queue: SyncSender<Vec<u8>>,
}

impl Input {
    /// Queues `bytes` without waiting; `InputFull` when the shell is not reading.
    pub(super) fn send(&self, bytes: &[u8]) -> Result<(), TermError> {
        match self.queue.try_send(bytes.to_vec()) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(TermError::InputFull),
            Err(TrySendError::Disconnected(_)) => Err(TermError::Exited),
        }
    }

    /// A second way into the queue, for answers the session sends on its own.
    pub(super) fn replies(&self) -> SyncSender<Vec<u8>> {
        self.queue.clone()
    }
}

/// Starts the writer thread. It stops at the first failed write (the shell is gone).
pub(super) fn start(writer: Box<dyn Write + Send>) -> io::Result<Input> {
    let (queue, pending) = mpsc::sync_channel::<Vec<u8>>(INPUT_QUEUE);
    let started = run_thread("navo-term-write", writer, move |mut writer| {
        for bytes in pending {
            if let Err(error) = writer.write_all(&bytes).and_then(|()| writer.flush()) {
                tracing::debug!(%error, "terminal input closed");
                break;
            }
        }
    });
    started.map(|_| Input { queue }).map_err(|(error, _)| error)
}
