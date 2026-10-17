mod batch;
mod translate;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};

use batch::Batch;
pub(crate) use translate::{RawKind, Translate};

pub const WATCH_QUIET: Duration = Duration::from_millis(100);
pub const WATCH_MAX_DELAY: Duration = Duration::from_millis(500);

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

#[derive(Clone, Debug)]
pub(crate) struct RawSender {
    sender: Sender<Raw>,
    live: Arc<AtomicBool>,
}

impl RawSender {
    pub(crate) fn mark_live(&self) {
        self.live.store(true, Ordering::Release);
    }

    pub(crate) fn change(&self, change: WatchChange) -> bool {
        self.sender.send(Raw::Change(change)).is_ok()
    }

    pub(crate) fn overflow(&self) -> bool {
        self.sender.send(Raw::Overflow).is_ok()
    }

    pub(crate) fn failed(&self, error: FsError) {
        self.live.store(false, Ordering::Release);
        let _ = self.sender.send(Raw::Failed(error));
    }
}

pub struct Watch {
    raw: Sender<Raw>,
    live: Arc<AtomicBool>,
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
        let (raw, events) = mpsc::channel();
        let live = Arc::new(AtomicBool::new(false));
        let backend = backend(RawSender {
            sender: raw.clone(),
            live: Arc::clone(&live),
        })?;
        let debouncer = std::thread::Builder::new()
            .name("folder-watch".to_owned())
            .spawn(move || debounce(&events, sink, batch))
            .map_err(|_| FsError {
                kind: FsErrorKind::Unknown,
                location: Some(folder.clone()),
                os_code: None,
            })?;
        Ok(Self {
            raw,
            live,
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
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.raw.send(Raw::Stop);
        drop(self.backend.take());
        if let Some(debouncer) = self.debouncer.take() {
            let _ = debouncer.join();
        }
    }
}

fn debounce(events: &Receiver<Raw>, mut sink: WatchSink, mut batch: Batch) {
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
        if let Some(event) = batch.take_due(Instant::now()) {
            sink(event);
        }
    }
}

#[cfg(test)]
mod tests;
