//! A compare side built by an opener: it runs on the session thread and refreshes once.

use std::io::Read;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use navo_compare::model::{
    CancelToken, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp, Status,
};
use navo_core::{FileIdentity, ProviderId, ProviderPath};

use super::location;
use crate::compare::handle::{CompareHandle, CompareStart, CompareTarget};
use crate::compare::local::LocalSide;
use crate::compare::settings::CompareSettings;

/// Wraps a side and counts the `refresh` calls the session thread makes on it.
struct Counting {
    inner: Arc<dyn SideSource>,
    refreshes: Arc<AtomicUsize>,
}

impl SideSource for Counting {
    fn provider(&self) -> &ProviderId {
        self.inner.provider()
    }

    fn list(
        &self,
        folder: &RelPath,
        sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), Reason> {
        self.inner.list(folder, sink, cancel)
    }

    fn refresh(&self) {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        self.inner.refresh();
    }

    fn open(&self, rel: &RelPath, hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        self.inner.open(rel, hint)
    }

    fn stamp(&self, rel: &RelPath) -> Result<Stamp, Reason> {
        self.inner.stamp(rel)
    }

    fn object_id(&self, rel: &RelPath) -> Option<ObjectId> {
        self.inner.object_id(rel)
    }

    fn file_id(&self, rel: &RelPath) -> Option<FileIdentity> {
        self.inner.file_id(rel)
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        self.inner.cache_path(rel)
    }

    fn case_sensitive(&self) -> bool {
        self.inner.case_sensitive()
    }

    fn read_only(&self) -> bool {
        self.inner.read_only()
    }
}

#[test]
fn a_side_target_opens_on_the_session_thread_and_refreshes() {
    let (left, right) = super::handle::two_trees();
    let opened_on: Arc<Mutex<Option<ThreadId>>> = Arc::new(Mutex::new(None));
    let refreshes: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let left_path = left.path().to_path_buf();
    let at = Arc::clone(&opened_on);
    let count = Arc::clone(&refreshes);
    let open = Box::new(move || {
        *at.lock().unwrap() = Some(thread::current().id());
        let inner: Arc<dyn SideSource> = Arc::new(LocalSide::new(location(&left_path))?);
        Ok(Arc::new(Counting {
            inner,
            refreshes: count,
        }) as Arc<dyn SideSource>)
    });
    let (sender, receiver) = mpsc::channel();
    let request = CompareStart {
        left: CompareTarget::Side(open),
        right: CompareTarget::Local(location(right.path())),
        settings: CompareSettings::default(),
        cache: Arc::new(super::handle::NoCache),
    };
    let handle = CompareHandle::start(request, sender);
    let (rows, totals, _) = super::handle::finish(&receiver);
    assert_eq!((totals.rows, totals.same, totals.different), (6, 3, 1));
    assert_eq!(
        super::handle::final_status(&rows, "left_only.txt"),
        Status::OnlyLeft
    );
    let id = opened_on
        .lock()
        .unwrap()
        .expect("the opener must record its thread");
    assert_ne!(id, thread::current().id());
    handle.refresh();
    let until = Instant::now() + Duration::from_secs(10);
    while refreshes.load(Ordering::SeqCst) < 1 && Instant::now() < until {
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(100));
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
}
