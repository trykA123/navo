use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use rusty_core::{FsError, Location, ProviderId};
use rusty_fs::{CancelToken, Capabilities, FakeFs, FsEntry, ListSink, Provider, ProviderOps};

#[derive(Default)]
struct Hold {
    is_held: bool,
    is_released: bool,
    was_cancelled: bool,
}

pub(super) struct Gate {
    inner: Arc<FakeFs>,
    folder: Location,
    gated_call: AtomicU32,
    calls: AtomicU32,
    hold: Mutex<Hold>,
    changed: Condvar,
}

impl Gate {
    pub fn new(inner: &Arc<FakeFs>, folder: &str) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::clone(inner),
            folder: inner.location(folder),
            gated_call: AtomicU32::new(1),
            calls: AtomicU32::new(0),
            hold: Mutex::new(Hold::default()),
            changed: Condvar::new(),
        })
    }

    pub fn arm(&self) {
        *self.hold.lock().unwrap() = Hold::default();
        let next = self.calls.load(Ordering::SeqCst) + 1;
        self.gated_call.store(next, Ordering::SeqCst);
    }

    pub fn is_held(&self) -> bool {
        self.hold.lock().unwrap().is_held
    }

    pub fn release(&self) {
        self.hold.lock().unwrap().is_released = true;
        self.changed.notify_all();
    }

    pub fn wait_until_done(&self) -> bool {
        let hold = self.hold.lock().unwrap();
        let hold = self
            .changed
            .wait_while(hold, |hold| hold.is_held && hold.is_released)
            .unwrap();
        hold.was_cancelled
    }

    fn hold_here(&self, cancel: &CancelToken) {
        let mut hold = self.hold.lock().unwrap();
        hold.is_held = true;
        self.changed.notify_all();
        hold = self
            .changed
            .wait_while(hold, |hold| !hold.is_released)
            .unwrap();
        hold.was_cancelled = cancel.is_cancelled();
        hold.is_held = false;
        self.changed.notify_all();
    }
}

impl Provider for Gate {
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
        let is_gated = folder == &self.folder
            && self.calls.fetch_add(1, Ordering::SeqCst) + 1
                == self.gated_call.load(Ordering::SeqCst);
        if is_gated {
            self.hold_here(cancel);
        }
        self.inner.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.inner.metadata(location)
    }
}

impl ProviderOps for Gate {
    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.inner.create_folder(location)
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.inner.create_file(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.inner.copy_file(source, target, progress, cancel)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.inner.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.inner.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.inner.remove_file(location)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.inner.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.inner.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.inner.recycle(location)
    }
}
