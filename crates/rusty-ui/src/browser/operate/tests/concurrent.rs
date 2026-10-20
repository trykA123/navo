use std::sync::{Condvar, Mutex, PoisonError};

use rusty_core::{FsError, Location, ProviderId};
use rusty_fs::{CancelToken, Capabilities, FsEntry, ListSink};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

pub(super) struct Tally {
    want: usize,
    counts: Mutex<(usize, usize)>,
    changed: Condvar,
}

impl Tally {
    pub(super) fn new(want: usize) -> Arc<Self> {
        Arc::new(Self {
            want,
            counts: Mutex::new((0, 0)),
            changed: Condvar::new(),
        })
    }

    pub(super) fn peak(&self) -> usize {
        self.counts.lock().unwrap_or_else(PoisonError::into_inner).1
    }

    fn enter(&self) {
        let mut counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        counts.0 += 1;
        counts.1 = counts.1.max(counts.0);
        self.changed.notify_all();
        let want = self.want;
        let _ = self
            .changed
            .wait_timeout_while(counts, WAIT, |counts| counts.1 < want);
    }

    fn leave(&self) {
        let mut counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        counts.0 -= 1;
    }
}

pub(super) struct Counting {
    pub(super) fs: Arc<FakeFs>,
    pub(super) tally: Arc<Tally>,
}

impl Provider for Counting {
    fn id(&self) -> ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.fs.metadata(location)
    }
}

impl ProviderOps for Counting {
    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_folder(location)
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_file(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.tally.enter();
        let copied = self.fs.copy_file(source, target, progress, cancel);
        self.tally.leave();
        copied
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_file(location)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.fs.recycle(location)
    }
}

pub(super) fn many_files() -> Arc<FakeFs> {
    let fs = fake();
    for index in 0..5 {
        fs.add_file(&format!("home/docs/f{index}.txt"), 5);
    }
    fs
}

pub(super) fn counting_browser(fs: &Arc<FakeFs>, want: usize) -> (Browser, Arc<Tally>) {
    let tally = Tally::new(want);
    let provider = Arc::new(Counting {
        fs: Arc::clone(fs),
        tally: Arc::clone(&tally),
    });
    let mut browser = Browser::new(home_beside_dest(fs), provider, Arc::new(|| {}));
    browser.start();
    settle(&mut browser);
    browser.set_class_of(|_| DriveClass::Ssd);
    (browser, tally)
}
