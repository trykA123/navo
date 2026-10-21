use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use rusty_core::{FsError, FsErrorKind, ProviderId};
use rusty_fs::{CancelToken, Capabilities, FakeFs, FsEntry, ListSink, Watch, WatchSink};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

struct Blocked {
    fs: FakeFs,
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    starts: AtomicUsize,
}

impl Provider for Blocked {
    fn id(&self) -> ProviderId {
        self.fs.id()
    }
    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }
    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.fs.metadata(location)
    }
    fn list(
        &self,
        location: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.fs.list(location, sink, cancel)
    }
    fn watch_tree(&self, _: &Location, _: WatchSink) -> Result<Watch, FsError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        self.entered.send(()).unwrap();
        self.release.lock().unwrap().recv_timeout(WAIT).unwrap();
        Err(FsError::new(FsErrorKind::Unknown))
    }
}

#[test]
fn five_reconfigurations_share_one_retirement_worker_and_do_not_restart_a_pending_root() {
    let folder = tempfile::tempdir().unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let provider = Arc::new(Blocked {
        fs: FakeFs::new(),
        entered,
        release: Mutex::new(released),
        starts: AtomicUsize::new(0),
    });
    provider.fs.add_folder("tree");
    let root = provider.fs.location("tree");
    let mut hosts = Hosts {
        provider: provider.clone(),
        dirs: Some(StoreDirs::in_profile(folder.path())),
        options: None,
        queue: Arc::new(IoQueue::new()),
        roots: Vec::new(),
        desired: Vec::new(),
        exclude: Vec::new(),
        configured: Configured::default(),
        retirement: Retirements::new().unwrap(),
    };
    let mut options = IndexOptions {
        mode: IndexMode::Walk,
        drives: vec![root.as_str().into()],
        ..IndexOptions::default()
    };
    hosts.configure(&options, None);
    waiting.recv_timeout(WAIT).unwrap();
    let worker = hosts.retirement.worker;
    for revision in 0..5 {
        options.exclude = vec![format!("skip-{revision}")];
        hosts.configure(&options, None);
        assert_eq!(hosts.retirement.worker, worker);
        assert!(hosts.retirement.pending(&root));
        assert!(hosts.roots.is_empty());
    }
    assert_eq!(provider.starts.load(Ordering::SeqCst), 1);
    release.send(()).unwrap();
    let deadline = std::time::Instant::now() + WAIT;
    while hosts.retirement.pending(&root) {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    hosts.configure(&options, None);
    waiting.recv_timeout(WAIT).unwrap();
    assert_eq!(hosts.roots.len(), 1);
    assert_eq!(provider.starts.load(Ordering::SeqCst), 2);
    release.send(()).unwrap();
    drop(hosts);
}
