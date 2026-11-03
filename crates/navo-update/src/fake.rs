//! A scripted `UpdateFeed` for tests. Replies are queued in order; a held reply blocks the call.

use crate::{FeedError, Found, InstallKind, Release, UpdateFeed, Version};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// One step of a scripted download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadStep {
    Progress(u8),
    Done(Result<(), FeedError>),
}

type CheckReply = Receiver<Result<Found, FeedError>>;
type DownloadScript = Receiver<DownloadStep>;

/// A feed that answers from queued scripts. A dropped sender ends a held call with an error.
pub struct FakeFeed {
    kind: InstallKind,
    current: Version,
    staged: Option<Version>,
    checks: Mutex<VecDeque<CheckReply>>,
    downloads: Mutex<VecDeque<DownloadScript>>,
    check_calls: AtomicUsize,
    download_calls: AtomicUsize,
}

impl FakeFeed {
    pub fn new(kind: InstallKind, current: Version) -> Self {
        Self {
            kind,
            current,
            staged: None,
            checks: Mutex::default(),
            downloads: Mutex::default(),
            check_calls: AtomicUsize::new(0),
            download_calls: AtomicUsize::new(0),
        }
    }

    pub fn with_staged(mut self, staged: Version) -> Self {
        self.staged = Some(staged);
        self
    }

    /// Queues a check that answers at once.
    pub fn answer_check(&self, result: Result<Found, FeedError>) {
        let _ = self.hold_check().send(result);
    }

    /// Queues a check that blocks until the sender answers. Dropping it gives `Offline`.
    pub fn hold_check(&self) -> Sender<Result<Found, FeedError>> {
        let (sender, receiver) = mpsc::channel();
        lock(&self.checks).push_back(receiver);
        sender
    }

    /// Queues a download that follows the sent steps. Dropping the sender interrupts it.
    pub fn hold_download(&self) -> Sender<DownloadStep> {
        let (sender, receiver) = mpsc::channel();
        lock(&self.downloads).push_back(receiver);
        sender
    }

    pub fn check_calls(&self) -> usize {
        self.check_calls.load(Ordering::SeqCst)
    }

    pub fn download_calls(&self) -> usize {
        self.download_calls.load(Ordering::SeqCst)
    }
}

impl UpdateFeed for FakeFeed {
    fn kind(&self) -> InstallKind {
        self.kind
    }

    fn current(&self) -> Version {
        self.current.clone()
    }

    fn staged(&self) -> Option<Version> {
        self.staged.clone()
    }

    fn check(&self) -> Result<Found, FeedError> {
        self.check_calls.fetch_add(1, Ordering::SeqCst);
        let reply = lock(&self.checks).pop_front();
        match reply {
            Some(reply) => reply.recv().unwrap_or(Err(FeedError::Offline)),
            None => Err(FeedError::Other("no scripted check".into())),
        }
    }

    fn download(&self, _release: &Release, progress: &dyn Fn(u8)) -> Result<(), FeedError> {
        self.download_calls.fetch_add(1, Ordering::SeqCst);
        let script = lock(&self.downloads).pop_front();
        let Some(script) = script else {
            return Err(FeedError::Other("no scripted download".into()));
        };
        loop {
            match script.recv() {
                Ok(DownloadStep::Progress(percent)) => progress(percent),
                Ok(DownloadStep::Done(result)) => return result,
                Err(_) => return Err(FeedError::Interrupted),
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
