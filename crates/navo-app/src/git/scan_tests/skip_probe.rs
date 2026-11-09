use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use navo_core::{FileIdentity, Location};
use navo_fs::DriveClass;
use navo_git::status::{Fingerprint, RepoStatus, Upstream, fingerprint};
use navo_store::GitSettings;
use navo_ui::git::FolderBadges;

use crate::git::scan::tests::{Harness, clean};
use crate::git::scan::{GitScan, Limits, Probe, RepoWatch};

#[derive(Clone)]
struct WatchState {
    live: Arc<AtomicBool>,
    changes: Arc<AtomicU64>,
}

struct FakeWatch {
    live: Arc<AtomicBool>,
    changes: Arc<AtomicU64>,
    dropped: Arc<AtomicUsize>,
}

impl Drop for FakeWatch {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

impl RepoWatch for FakeWatch {
    fn is_live(&self) -> bool {
        self.live.load(Ordering::SeqCst)
    }

    fn changes(&self) -> u64 {
        self.changes.load(Ordering::SeqCst)
    }
}

/// A probe on a settled fixed drive: settable fingerprint, a live watch, and a clock the
/// test can move.
pub(crate) struct SkipProbe {
    repos: HashSet<PathBuf>,
    class: DriveClass,
    print: Mutex<Option<Fingerprint>>,
    watches: Mutex<HashMap<PathBuf, WatchState>>,
    start: Instant,
    offset: Mutex<Duration>,
    dropped: Arc<AtomicUsize>,
}

impl SkipProbe {
    pub(crate) fn new(class: DriveClass, repos: &[&str]) -> Self {
        Self {
            repos: repos.iter().map(PathBuf::from).collect(),
            class,
            print: Mutex::new(None),
            watches: Mutex::new(HashMap::new()),
            start: Instant::now(),
            offset: Mutex::new(Duration::ZERO),
            dropped: Arc::default(),
        }
    }

    pub(crate) fn set_print(&self, print: Option<Fingerprint>) {
        *self.print.lock().unwrap() = print;
    }

    pub(crate) fn advance(&self, by: Duration) {
        *self.offset.lock().unwrap() += by;
    }

    fn state(&self, root: &Path) -> WatchState {
        self.watches
            .lock()
            .unwrap()
            .entry(root.to_path_buf())
            .or_insert_with(|| WatchState {
                live: Arc::new(AtomicBool::new(true)),
                changes: Arc::new(AtomicU64::new(0)),
            })
            .clone()
    }

    /// Waits until `count` watches were released.
    pub(super) fn wait_dropped(&self, count: usize) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.dropped.load(Ordering::SeqCst) < count {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        true
    }

    pub(super) fn bump(&self, root: &Path) {
        self.state(root).changes.fetch_add(1, Ordering::SeqCst);
    }
}

impl Probe for SkipProbe {
    fn has_git(&self, folder: &Path) -> bool {
        self.repos.contains(folder)
    }

    fn drive_class(&self, _: &Location) -> DriveClass {
        self.class
    }

    fn now(&self) -> Instant {
        self.start + *self.offset.lock().unwrap() + self.start.elapsed()
    }

    fn fingerprint(&self, _: &Path, _: Option<&str>) -> Option<Fingerprint> {
        *self.print.lock().unwrap()
    }

    fn watch_tree(&self, root: &Path) -> Option<Box<dyn RepoWatch>> {
        let state = self.state(root);
        Some(Box::new(FakeWatch {
            live: state.live,
            changes: state.changes,
            dropped: self.dropped.clone(),
        }))
    }

    fn root_class(&self, _: &Path, _: &Location) -> DriveClass {
        self.class
    }

    fn identity(&self, _: &Path) -> Option<FileIdentity> {
        Some(FileIdentity::fingerprint(7, Some(7), true))
    }
}

fn write_stamp(root: &Path, relative: &str, text: &str, at: SystemTime) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

/// Two fingerprints of one settled `.git` layout, before and after an index write.
pub(crate) fn prints() -> (Fingerprint, Fingerprint) {
    let dir = tempfile::tempdir().unwrap();
    let old = SystemTime::now() - Duration::from_secs(60);
    let now = SystemTime::now();
    write_stamp(dir.path(), ".git/HEAD", "ref: refs/heads/main\n", old);
    write_stamp(dir.path(), ".git/refs/heads/main", "a\n", old);
    write_stamp(dir.path(), ".git/index", "one", old);
    write_stamp(dir.path(), ".git/config", "[core]\n", old);
    let first = fingerprint(dir.path(), None, now).unwrap();
    write_stamp(dir.path(), ".git/index", "two-longer", old);
    let second = fingerprint(dir.path(), None, now).unwrap();
    assert_ne!(first, second);
    (first, second)
}

pub(crate) fn repo() -> PathBuf {
    PathBuf::from("fixture/repo")
}

pub(crate) fn harness(probe: Arc<SkipProbe>, network: bool) -> Harness {
    let settings = GitSettings {
        network,
        ..GitSettings::default()
    };
    Harness::build(probe, move |ports, notify| {
        GitScan::start_with_limits(ports, &settings, notify, Limits::default()).unwrap()
    })
}

pub(crate) fn scanned(h: &Harness, folder: &str, name: &str) {
    h.observe(folder, 1, &[name]);
    assert_eq!(
        h.next().0,
        PathBuf::from(folder).join(name),
        "the first open scans the repository"
    );
}

pub(super) fn upstream_status(name: &str) -> RepoStatus {
    let mut status = clean();
    status.upstream = Some(Upstream {
        name: name.to_owned(),
        ahead: 0,
        behind: 0,
    });
    status
}

pub(crate) fn fresh(h: &Harness, folder: &str, name: &str) -> Arc<FolderBadges> {
    h.wait(folder, |badges| {
        badges.by_name.get(name).is_some_and(|badge| !badge.stale)
    })
}
