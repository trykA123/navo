use std::ops::ControlFlow;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use rusty_core::{FsError, Location, ProviderId};
use rusty_fs::{
    CancelToken, Capabilities, EntryBatch, FakeFs, FsEntry, ListSink, Provider, ProviderOps,
};

struct Share {
    is_answering: bool,
    held: usize,
}

pub struct Stalled {
    inner: Arc<FakeFs>,
    folder: Location,
    sends_first_batch: bool,
    share: Mutex<Share>,
    changed: Condvar,
}

impl Stalled {
    pub fn new(inner: &Arc<FakeFs>, folder: &str) -> Arc<Self> {
        Self::build(inner, folder, false)
    }

    pub fn trickling(inner: &Arc<FakeFs>, folder: &str) -> Arc<Self> {
        Self::build(inner, folder, true)
    }

    fn build(inner: &Arc<FakeFs>, folder: &str, sends_first_batch: bool) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::clone(inner),
            folder: inner.location(folder),
            sends_first_batch,
            share: Mutex::new(Share {
                is_answering: true,
                held: 0,
            }),
            changed: Condvar::new(),
        })
    }

    pub fn wait_until_held(&self, count: usize) {
        let share = self.share.lock().unwrap();
        let (share, timeout) = self
            .changed
            .wait_timeout_while(share, Duration::from_secs(5), |share| share.held < count)
            .unwrap();
        assert!(!timeout.timed_out(), "held {} of {count} lists", share.held);
    }

    pub fn stop_answering(&self) {
        self.share.lock().unwrap().is_answering = false;
    }

    pub fn answer(&self) {
        self.share.lock().unwrap().is_answering = true;
        self.changed.notify_all();
    }

    fn hold(&self, cancel: &CancelToken) {
        let mut share = self.share.lock().unwrap();
        if share.is_answering {
            return;
        }
        share.held += 1;
        self.changed.notify_all();
        while !share.is_answering && !cancel.is_cancelled() {
            share = self
                .changed
                .wait_timeout(share, Duration::from_millis(20))
                .unwrap()
                .0;
        }
    }
}

impl Provider for Stalled {
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
        if folder != &self.folder {
            return self.inner.list(folder, sink, cancel);
        }
        if self.sends_first_batch {
            let _ = sink.batch(EntryBatch {
                entries: Vec::new(),
                last: false,
            });
        }
        self.hold(cancel);
        let mut rows: Vec<FsEntry> = Vec::new();
        self.inner.list(
            folder,
            &mut |batch: EntryBatch| {
                rows.extend(batch.entries);
                ControlFlow::Continue(())
            },
            cancel,
        )?;
        let _ = sink.batch(EntryBatch {
            entries: rows,
            last: true,
        });
        Ok(())
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.inner.metadata(location)
    }
}

impl ProviderOps for Stalled {
    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.inner.create_folder(location)
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
