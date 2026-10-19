use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use rusty_core::{FsError, Location, ProviderId};
use rusty_fs::{CancelToken, Capabilities, FakeFs, FsEntry, ListSink, Provider, ProviderOps};

const HOLD_LIMIT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Hold {
    held: usize,
    reading: usize,
    most_reading: usize,
    is_released: bool,
}

pub struct Held {
    inner: Arc<FakeFs>,
    folders: Vec<Location>,
    hold: Mutex<Hold>,
    changed: Condvar,
}

impl Held {
    pub fn new(inner: &Arc<FakeFs>, folders: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::clone(inner),
            folders: folders.iter().map(|path| inner.location(path)).collect(),
            hold: Mutex::new(Hold::default()),
            changed: Condvar::new(),
        })
    }

    pub fn wait_until_held(&self, count: usize) {
        let hold = self.hold.lock().unwrap();
        let (hold, timeout) = self
            .changed
            .wait_timeout_while(hold, Duration::from_secs(5), |hold| hold.held < count)
            .unwrap();
        assert!(!timeout.timed_out(), "held {} of {count} lists", hold.held);
    }

    pub fn most_reading(&self) -> usize {
        self.hold.lock().unwrap().most_reading
    }

    pub fn release(&self) {
        self.hold.lock().unwrap().is_released = true;
        self.changed.notify_all();
    }

    fn start_reading(&self) {
        let mut hold = self.hold.lock().unwrap();
        hold.reading += 1;
        hold.most_reading = hold.most_reading.max(hold.reading);
    }

    fn hold_here(&self) {
        let mut hold = self.hold.lock().unwrap();
        hold.held += 1;
        self.changed.notify_all();
        drop(
            self.changed
                .wait_timeout_while(hold, HOLD_LIMIT, |hold| !hold.is_released)
                .unwrap(),
        );
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
        self.start_reading();
        if self.folders.contains(folder) {
            self.hold_here();
        }
        let listed = self.inner.list(folder, sink, cancel);
        self.hold.lock().unwrap().reading -= 1;
        listed
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.inner.metadata(location)
    }
}

impl ProviderOps for Held {
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
