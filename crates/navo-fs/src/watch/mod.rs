mod batch;
mod translate;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use navo_core::{FsError, FsErrorKind, Location, ProviderPath};

use batch::Batch;
pub(crate) use translate::{RawKind, Translate};

pub const WATCH_QUIET: Duration = Duration::from_millis(100);
pub const WATCH_MAX_DELAY: Duration = Duration::from_millis(500);

/// Raw notifications queued for the debouncer; a full queue turns into one overflow (relist).
const RAW_CAPACITY: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchChange {
    Created(ProviderPath),
    Removed(ProviderPath),
    Modified(ProviderPath),
    Renamed {
        from: ProviderPath,
        to: ProviderPath,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    Changed(Vec<WatchChange>),
    Overflow,
    Failed(FsError),
}

pub type WatchSink = Box<dyn FnMut(WatchEvent) + Send>;

pub(crate) enum Raw {
    Change(WatchChange),
    Overflow,
    Failed(FsError),
    Stop,
}

/// Flags the debouncer reads after every wake-up, so no producer or `Drop` ever blocks on the queue.
#[derive(Debug, Default)]
struct Signals {
    /// A notification did not fit the queue; the next batch is an overflow.
    lost: AtomicBool,
    /// The watch is dropping; the debouncer returns at its next wake-up.
    stop: AtomicBool,
}

#[derive(Clone, Debug)]
pub(crate) struct RawSender {
    sender: SyncSender<Raw>,
    live: Arc<AtomicBool>,
    count: Arc<AtomicU64>,
    signals: Arc<Signals>,
}

impl RawSender {
    pub(crate) fn mark_live(&self) {
        self.live.store(true, Ordering::Release);
    }

    /// Counts one completed read, so a record the parser skips still counts as a change.
    #[cfg(windows)]
    pub(crate) fn count_read(&self) {
        self.count.fetch_add(1, Ordering::Release);
    }

    pub(crate) fn change(&self, change: WatchChange) -> bool {
        self.count.fetch_add(1, Ordering::Release);
        self.send(Raw::Change(change))
    }

    pub(crate) fn overflow(&self) -> bool {
        self.count.fetch_add(1, Ordering::Release);
        self.send(Raw::Overflow)
    }

    /// A failure that meets a full queue arrives as an overflow; `is_live` still turns false.
    pub(crate) fn failed(&self, error: FsError) {
        self.count.fetch_add(1, Ordering::Release);
        self.live.store(false, Ordering::Release);
        let _ = self.send(Raw::Failed(error));
    }

    /// Queues without blocking the OS watcher thread; false once the debouncer is gone.
    fn send(&self, raw: Raw) -> bool {
        match self.sender.try_send(raw) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.signals.lost.store(true, Ordering::Release);
                // Wakes a debouncer that drained the queue before the flag was set.
                let _ = self.sender.try_send(Raw::Overflow);
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

pub struct Watch {
    raw: SyncSender<Raw>,
    signals: Arc<Signals>,
    live: Arc<AtomicBool>,
    count: Arc<AtomicU64>,
    backend: Option<Box<dyn Send>>,
    debouncer: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Depth {
    Folder,
    Subtree,
}

impl Watch {
    pub(crate) fn start<B, F>(
        folder: &Location,
        sink: WatchSink,
        backend: F,
    ) -> Result<Self, FsError>
    where
        B: Send + 'static,
        F: FnOnce(RawSender) -> Result<B, FsError>,
    {
        Self::start_at(Depth::Folder, folder, sink, backend)
    }

    pub(crate) fn start_tree<B, F>(
        folder: &Location,
        sink: WatchSink,
        backend: F,
    ) -> Result<Self, FsError>
    where
        B: Send + 'static,
        F: FnOnce(RawSender) -> Result<B, FsError>,
    {
        Self::start_at(Depth::Subtree, folder, sink, backend)
    }

    fn start_at<B, F>(
        depth: Depth,
        folder: &Location,
        sink: WatchSink,
        backend: F,
    ) -> Result<Self, FsError>
    where
        B: Send + 'static,
        F: FnOnce(RawSender) -> Result<B, FsError>,
    {
        let batch = match depth {
            Depth::Folder => Batch::default(),
            Depth::Subtree => Batch::for_tree(),
        };
        let (raw, events) = mpsc::sync_channel(RAW_CAPACITY);
        let signals = Arc::new(Signals::default());
        let live = Arc::new(AtomicBool::new(false));
        let count = Arc::new(AtomicU64::new(0));
        let backend = backend(RawSender {
            sender: raw.clone(),
            live: Arc::clone(&live),
            count: Arc::clone(&count),
            signals: Arc::clone(&signals),
        })?;
        let flags = Arc::clone(&signals);
        let debouncer = std::thread::Builder::new()
            .name("folder-watch".to_owned())
            .spawn(move || debounce(&events, sink, batch, &flags))
            .map_err(|_| FsError {
                location: Some(folder.clone()),
                ..FsError::new(FsErrorKind::Unknown)
            })?;
        Ok(Self {
            raw,
            signals,
            live,
            count,
            backend: Some(Box::new(backend)),
            debouncer: Some(debouncer),
        })
    }
}

impl Watch {
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.live.load(Ordering::Acquire)
    }

    /// Raw notifications received so far, including those a batch collapses.
    #[must_use]
    pub fn raw_changes(&self) -> u64 {
        self.count.load(Ordering::Acquire)
    }
}

impl Drop for Watch {
    // The backend goes first so no new notification races the stop; nothing here blocks on a full
    // queue, because the debouncer checks `stop` after every message it takes.
    fn drop(&mut self) {
        drop(self.backend.take());
        self.signals.stop.store(true, Ordering::Release);
        let _ = self.raw.try_send(Raw::Stop);
        if let Some(debouncer) = self.debouncer.take() {
            let _ = debouncer.join();
        }
    }
}

fn debounce(events: &Receiver<Raw>, mut sink: WatchSink, mut batch: Batch, signals: &Signals) {
    loop {
        let next = match batch.deadline() {
            Some(at) => events.recv_timeout(at.saturating_duration_since(Instant::now())),
            None => events.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match next {
            Ok(Raw::Change(change)) => batch.push(change, Instant::now()),
            Ok(Raw::Overflow) => batch.overflow(Instant::now()),
            Ok(Raw::Failed(error)) => {
                batch.take().into_iter().for_each(&mut sink);
                sink(WatchEvent::Failed(error));
            }
            Ok(Raw::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if signals.stop.load(Ordering::Acquire) {
            return;
        }
        if signals.lost.swap(false, Ordering::AcqRel) {
            batch.overflow(Instant::now());
        }
        if let Some(event) = batch.take_due(Instant::now()) {
            sink(event);
        }
    }
}

#[cfg(test)]
mod tests;
