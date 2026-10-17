use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use rusty_core::{FsError, Location, ProviderId};

use crate::{CancelToken, Capabilities, FakeFs, FsEntry, ListSink, Provider, Watch, WatchSink};

#[derive(Default)]
struct Hold {
    is_held: bool,
    is_released: bool,
}

pub(super) struct Held {
    inner: Arc<FakeFs>,
    folder: Location,
    held_call: u32,
    calls: AtomicU32,
    hold: Mutex<Hold>,
    changed: Condvar,
}

impl Held {
    pub fn new(inner: &Arc<FakeFs>, folder: &str, held_call: u32) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::clone(inner),
            folder: inner.location(folder),
            held_call,
            calls: AtomicU32::new(0),
            hold: Mutex::new(Hold::default()),
            changed: Condvar::new(),
        })
    }

    pub fn wait_until_held(&self) {
        let hold = self.hold.lock().unwrap_or_else(PoisonError::into_inner);
        let (hold, timeout) = self
            .changed
            .wait_timeout_while(hold, Duration::from_secs(10), |hold| !hold.is_held)
            .unwrap_or_else(PoisonError::into_inner);
        assert!(hold.is_held && !timeout.timed_out());
    }

    pub fn release(&self) {
        self.hold
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_released = true;
        self.changed.notify_all();
    }

    fn hold_here(&self) {
        let mut hold = self.hold.lock().unwrap_or_else(PoisonError::into_inner);
        hold.is_held = true;
        self.changed.notify_all();
        let _released = self
            .changed
            .wait_while(hold, |hold| !hold.is_released)
            .unwrap_or_else(PoisonError::into_inner);
    }
}

impl Provider for Held {
    fn id(&self) -> ProviderId {
        self.inner.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        let is_held = folder == &self.folder
            && self.calls.fetch_add(1, Ordering::SeqCst) + 1 == self.held_call;
        if is_held {
            self.hold_here();
        }
        self.inner.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.inner.metadata(location)
    }

    fn watch(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        self.inner.watch(folder, sink)
    }

    fn watch_tree(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        self.inner.watch_tree(folder, sink)
    }
}
