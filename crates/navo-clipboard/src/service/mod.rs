//! Background clipboard service: one worker thread owns the store.

mod notes;
mod requests;
mod saved;
mod secret;
mod shared;
mod snapshot;
mod worker;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::limits::Limits;
use crate::model::Captured;
use crate::store::ClipStore;

use shared::{EndGuard, Shared, lock};
use worker::{Msg, Worker};

pub use secret::{
    Clock, NoSealer, SECRET_CLEAR_AFTER, SealError, Sealer, SecretBytes, Sequence, SystemClock,
};
pub use snapshot::{NoteHeader, Snapshot};

const QUEUE: usize = 64;
const STOP_WAIT: Duration = Duration::from_secs(2);

/// How a row goes back to the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    Copy,
    Paste,
}

/// Why the sink could not write. Never carries clipboard content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SinkError {
    #[error("clipboard writes are not supported here")]
    Unsupported,
    #[error("the clipboard write failed")]
    Failed,
}

/// Where the worker writes a row back. Called on the worker thread only.
pub trait ClipSink: Send + Sync {
    fn write_text(&self, text: &str) -> Result<(), SinkError>;
    fn write_files(&self, paths: &[PathBuf], cut: bool) -> Result<(), SinkError>;
    /// Writes text marked private, so clipboard history leaves it out, and returns the
    /// clipboard sequence the write produced. An `Err` leaves the clipboard without the
    /// text, and the sink wipes its own copy of the text on both paths.
    fn write_private(&self, text: &str) -> Result<Sequence, SinkError>;
    /// Empties the clipboard when its sequence is still `sequence`; returns whether it did.
    fn clear_if_sequence(&self, sequence: Sequence) -> Result<bool, SinkError>;
}

/// The ports the worker calls.
#[derive(Clone)]
pub struct Ports {
    pub sink: Arc<dyn ClipSink>,
    pub sealer: Arc<dyn Sealer>,
    pub clock: Arc<dyn Clock>,
}

/// What the last failure was about; no content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailKind {
    Store,
    Sink,
    NotFound,
    Thread,
    /// The request was dropped: the queue was full or the service had stopped.
    Busy,
    /// The item is over a size cap.
    TooLarge,
    /// The saved list is full.
    Full,
    /// The request does not fit the item, such as plain text for a secret.
    Refused,
    /// The target folder already has a note.
    NoteExists,
    /// This build cannot seal secrets.
    SecretsUnsupported,
    /// Sealing or unsealing failed.
    Seal,
}

/// The answer to one request, sent once from the worker. A disconnect means the worker
/// discarded the request while stopping.
pub type Reply<T> = Receiver<Result<T, FailKind>>;

struct Running {
    thread: JoinHandle<()>,
    done: mpsc::Receiver<()>,
}

/// Records captures, serves snapshots and copies rows back, all off the caller's thread.
pub struct ClipService {
    shared: Arc<Shared>,
    tx: Mutex<Option<SyncSender<Msg>>>,
    running: Mutex<Option<Running>>,
    stop_wait: Duration,
}

impl ClipService {
    /// Reads the first snapshot, then starts the worker. Recording starts on.
    pub fn start(store: ClipStore, limits: Limits, ports: Ports) -> Self {
        Self::start_with(store, limits, ports, STOP_WAIT)
    }

    fn start_with(store: ClipStore, limits: Limits, ports: Ports, stop_wait: Duration) -> Self {
        let shared = Arc::new(Shared::new());
        let mut worker = Worker::new(store, limits, ports, shared.clone());
        worker.refresh();
        let (tx, rx) = mpsc::sync_channel(QUEUE);
        let (done_tx, done) = mpsc::channel();
        let ended = EndGuard(shared.clone());
        let spawned = thread::Builder::new()
            .name("navo-clipboard".into())
            .spawn(move || {
                let _done = done_tx;
                let _ended = ended;
                worker.run(&rx);
            });
        let (tx, running) = match spawned {
            Ok(thread) => (Some(tx), Some(Running { thread, done })),
            Err(_) => {
                shared.fail(FailKind::Thread);
                (None, None)
            }
        };
        Self {
            shared,
            tx: Mutex::new(tx),
            running: Mutex::new(running),
            stop_wait,
        }
    }

    /// Queues a capture. Returns false when it was dropped.
    pub fn captured(&self, captured: Captured) -> bool {
        self.shared.is_recording() && self.send(Msg::Captured(captured))
    }

    /// Queues a write of the row to the sink. The write is the same for both `How`s.
    pub fn copy(&self, id: i64, _how: How) -> bool {
        self.send(Msg::Copy(id, None))
    }

    pub fn delete(&self, id: i64) -> bool {
        self.send(Msg::Delete(id))
    }

    /// Clears the history rows; the reply tells whether the store took it. Saved items stay.
    pub fn clear_history(&self) -> Reply<()> {
        self.request(Msg::Clear)
    }

    /// Deletes all clipboard data and the set-aside copies; the reply tells whether the
    /// files are gone. A failure leaves the data where it was.
    pub fn wipe_all(&self) -> Reply<()> {
        self.request(Msg::Wipe)
    }

    /// False once the worker thread has ended or never started, as after a panic or a stop.
    pub fn is_alive(&self) -> bool {
        !self.shared.has_ended()
    }

    /// Whether captures are recorded.
    pub fn is_recording(&self) -> bool {
        self.shared.is_recording()
    }

    /// Takes effect at once; captures queued earlier are dropped too.
    pub fn set_recording(&self, on: bool) {
        self.shared.set_recording(on);
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.shared.snapshot()
    }

    /// Calls `listener` on the worker thread with the new revision after each change.
    /// A panic in it is counted as a `Thread` failure and the worker goes on.
    pub fn subscribe(&self, listener: Box<dyn Fn(u64) + Send>) {
        self.shared.subscribe(listener);
    }

    /// Events dropped because the queue was full or the service had stopped.
    pub fn dropped(&self) -> u64 {
        self.shared.dropped()
    }

    pub fn errors(&self) -> u64 {
        self.shared.errors()
    }

    pub fn last_error(&self) -> Option<FailKind> {
        self.shared.last_error()
    }

    /// Finishes the queued work, empties the clipboard of a pending secret copy that is still
    /// current, and joins the worker. When the worker does not end in 2 s this
    /// returns false, and the worker discards the rest and ends once its current call returns.
    /// A later call reports whether it has ended by then.
    pub fn stop(&self) -> bool {
        drop(lock(&self.tx).take());
        let mut running = lock(&self.running);
        let Some(current) = running.as_ref() else {
            return true;
        };
        match current.done.recv_timeout(self.stop_wait) {
            Err(RecvTimeoutError::Timeout) => {
                self.shared.set_stopping();
                false
            }
            Ok(()) | Err(RecvTimeoutError::Disconnected) => running
                .take()
                .is_some_and(|ended| ended.thread.join().is_ok()),
        }
    }

    #[cfg(test)]
    fn wake(&self) -> bool {
        self.send(Msg::Wake)
    }

    fn send(&self, msg: Msg) -> bool {
        let sent = lock(&self.tx)
            .as_ref()
            .is_some_and(|tx| tx.try_send(msg).is_ok());
        if !sent {
            self.shared.count_dropped();
        }
        sent
    }
}

impl Drop for ClipService {
    fn drop(&mut self) {
        self.stop();
    }
}
