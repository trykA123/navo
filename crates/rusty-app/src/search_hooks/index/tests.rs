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

fn hosts(provider: Arc<dyn Provider>, dirs: Option<StoreDirs>) -> Hosts {
    Hosts {
        provider,
        dirs,
        options: None,
        queue: Arc::new(IoQueue::new()),
        roots: Vec::new(),
        desired: Vec::new(),
        exclude: Vec::new(),
        configured: Configured::default(),
        retirement: Retirements::new().unwrap(),
    }
}

#[test]
fn a_busy_or_slow_pipe_still_counts_as_a_running_service() {
    assert!(service_running(&Err::<(), _>(pipe::PipeError::TimedOut)));
    assert!(service_running(&Ok::<_, pipe::PipeError>(())));
    assert!(!service_running(&Err::<(), _>(
        pipe::PipeError::NotAvailable
    )));
}

#[test]
fn a_pipe_that_breaks_after_connecting_is_not_reported_as_absent() {
    assert!(!matches!(
        after_connect(pipe::PipeError::NotAvailable),
        pipe::PipeError::NotAvailable
    ));
    assert!(matches!(
        after_connect(pipe::PipeError::TimedOut),
        pipe::PipeError::TimedOut
    ));
}

#[test]
fn the_status_hook_answers_while_a_walker_holds_the_hosts_lock() {
    let shared = Arc::new(Mutex::new(hosts(Arc::new(FakeFs::new()), None)));
    let status = status::hook(shared.clone());
    let (locked, held) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let _guard = shared.lock().unwrap();
        locked.send(()).unwrap();
        let _ = released.recv_timeout(WAIT);
    });
    held.recv_timeout(WAIT).unwrap();
    let started = std::time::Instant::now();
    assert!(status().is_none());
    assert!(started.elapsed() < Duration::from_millis(50));
    release.send(()).unwrap();
    holder.join().unwrap();
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
    let mut hosts = hosts(provider.clone(), Some(StoreDirs::in_profile(folder.path())));
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
