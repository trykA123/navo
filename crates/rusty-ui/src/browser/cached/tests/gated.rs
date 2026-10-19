use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use rusty_core::{FsError, Location, ProviderId};
use rusty_fs::{
    CancelToken, Capabilities, EntryBatch, FakeFs, FsEntry, ListSink, Provider, ProviderOps,
};

#[derive(Default)]
struct Hold {
    is_held: bool,
    is_released: bool,
}

pub struct Gated {
    inner: Arc<FakeFs>,
    folder: Location,
    gated_call: u32,
    calls: AtomicU32,
    hold: Mutex<Hold>,
    changed: Condvar,
}

impl Gated {
    pub fn new(inner: &Arc<FakeFs>, folder: &str, gated_call: u32) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::clone(inner),
            folder: inner.location(folder),
            gated_call,
            calls: AtomicU32::new(0),
            hold: Mutex::new(Hold::default()),
            changed: Condvar::new(),
        })
    }

    pub fn wait_until_held(&self) {
        let hold = self.hold.lock().unwrap();
        drop(self.changed.wait_while(hold, |hold| !hold.is_held).unwrap());
    }

    pub fn release(&self) {
        self.hold.lock().unwrap().is_released = true;
        self.changed.notify_all();
    }

    fn hold_here(&self) {
        let mut hold = self.hold.lock().unwrap();
        hold.is_held = true;
        self.changed.notify_all();
        drop(
            self.changed
                .wait_while(hold, |hold| !hold.is_released)
                .unwrap(),
        );
    }
}

impl Provider for Gated {
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
            && self.calls.fetch_add(1, Ordering::SeqCst) + 1 == self.gated_call;
        if !is_gated {
            return self.inner.list(folder, sink, cancel);
        }
        let mut seen: Vec<FsEntry> = Vec::new();
        self.inner.list(
            folder,
            &mut |batch: EntryBatch| {
                seen.extend(batch.entries);
                ControlFlow::Continue(())
            },
            cancel,
        )?;
        self.hold_here();
        let _ = sink.batch(EntryBatch {
            entries: seen,
            last: true,
        });
        Ok(())
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.inner.metadata(location)
    }
}

impl ProviderOps for Gated {
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
