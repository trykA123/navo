use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use navo_core::Location;
use navo_fs::DriveClass;
use navo_git::status::{RepoStatus, parse_porcelain_v2};
use navo_store::GitSettings;
use navo_ui::git::{BadgeKind, FolderBadges};

use super::*;

#[path = "scan_tests/lifecycle.rs"]
mod lifecycle;
#[path = "scan_tests/limits.rs"]
mod limits;
#[path = "scan_tests/results.rs"]
mod results;

#[derive(Default)]
struct FakeProbe {
    repos: HashSet<PathBuf>,
    network: bool,
    probes: AtomicUsize,
    clock: AtomicUsize,
    step: usize,
}

impl Probe for FakeProbe {
    fn has_git(&self, folder: &Path) -> bool {
        self.probes.fetch_add(1, Ordering::SeqCst);
        self.repos.contains(folder)
    }

    fn drive_class(&self, _: &Location) -> DriveClass {
        if self.network {
            DriveClass::Network
        } else {
            DriveClass::Removable
        }
    }

    fn now(&self) -> Instant {
        *self.epoch()
            + Duration::from_millis(self.clock.fetch_add(self.step, Ordering::SeqCst) as u64)
    }
}

impl FakeProbe {
    fn epoch(&self) -> &'static Instant {
        static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        EPOCH.get_or_init(Instant::now)
    }
}

#[derive(Default)]
struct FakeSource {
    started: Mutex<Option<mpsc::Sender<(PathBuf, Cancel)>>>,
    replies: Mutex<VecDeque<Result<RepoStatus, ScanError>>>,
    ignore_cancel: AtomicBool,
}

impl StatusSource for FakeSource {
    fn status(&self, root: &Path, cancel: &Cancel) -> Result<RepoStatus, ScanError> {
        if let Some(started) = self.started.lock().unwrap().as_ref() {
            started.send((root.to_path_buf(), cancel.clone())).unwrap();
        }
        loop {
            let ignore = self.ignore_cancel.load(Ordering::SeqCst);
            let may_reply = ignore || !cancel.is_cancelled();
            let reply = if may_reply {
                self.replies.lock().unwrap().pop_front()
            } else {
                None
            };
            if let Some(reply) = reply {
                return reply;
            }
            if !may_reply {
                return Err(ScanError::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

struct Harness {
    scan: GitScan,
    source: Arc<FakeSource>,
    calls: mpsc::Receiver<(PathBuf, Cancel)>,
    notices: mpsc::Receiver<u64>,
}

impl Harness {
    fn new(probe: Arc<FakeProbe>) -> Self {
        Self::build(probe, |ports, notify| {
            GitScan::start(ports, &GitSettings::default(), notify).unwrap()
        })
    }

    fn configured(probe: Arc<FakeProbe>, settings: GitSettings, limits: Limits) -> Self {
        Self::build(probe, move |ports, notify| {
            GitScan::start_with_limits(ports, &settings, notify, limits).unwrap()
        })
    }

    fn build(probe: Arc<FakeProbe>, start: impl FnOnce(Ports, Notify) -> GitScan) -> Self {
        let (started, calls) = mpsc::channel();
        let (notify, notices) = mpsc::channel();
        let source = Arc::new(FakeSource::default());
        *source.started.lock().unwrap() = Some(started);
        let ports = Ports {
            probe,
            source: source.clone(),
        };
        let scan = start(
            ports,
            Box::new(move |revision| {
                let _ = notify.send(revision);
            }),
        );
        Self {
            scan,
            source,
            calls,
            notices,
        }
    }

    fn reply(&self, result: Result<RepoStatus, ScanError>) {
        self.source.replies.lock().unwrap().push_back(result);
    }

    fn observe(&self, folder: &str, generation: u64, names: &[&str]) {
        self.scan.observe(
            &location(folder),
            generation,
            names
                .iter()
                .map(|name| Row {
                    name: (*name).to_owned(),
                    on_screen: false,
                })
                .collect(),
        );
    }

    fn next(&self) -> (PathBuf, Cancel) {
        self.calls.recv_timeout(Duration::from_secs(3)).unwrap()
    }

    fn wait(&self, folder: &str, ready: impl Fn(&FolderBadges) -> bool) -> Arc<FolderBadges> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let current = self.scan.badges(&location(folder));
            if let Some(badges) = current.filter(|badges| ready(badges)) {
                return badges;
            }
            self.notices
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        }
    }
}

fn location(folder: &str) -> Location {
    Location::new(folder).unwrap()
}
fn status_with(records: &[u8]) -> RepoStatus {
    let mut output = b"# branch.oid (initial)\0# branch.head main\0".to_vec();
    output.extend_from_slice(records);
    parse_porcelain_v2(&output, false).unwrap()
}
fn clean() -> RepoStatus {
    status_with(b"")
}
fn probe(repos: &[&str]) -> Arc<FakeProbe> {
    Arc::new(FakeProbe {
        repos: repos.iter().map(PathBuf::from).collect(),
        ..FakeProbe::default()
    })
}

#[test]
fn subtree_badges_use_one_status_and_nested_repository_takes_precedence() {
    let h = Harness::new(probe(&["fixture/repo", "fixture/repo/nested"]));
    h.observe("fixture/repo", 40, &["src", "docs", "nested"]);
    let roots = [h.next().0, h.next().0];
    assert!(roots.contains(&PathBuf::from("fixture/repo")));
    assert!(roots.contains(&PathBuf::from("fixture/repo/nested")));
    let status = status_with(b"? src/deep/file\0");
    h.reply(Ok(status.clone()));
    h.reply(Ok(status));
    let badges = h.wait("fixture/repo", |b| {
        b.by_name.len() == 3 && b.by_name.values().all(|b| !b.stale)
    });
    assert_eq!(badges.by_name["src"].kind, BadgeKind::Modified);
    assert_eq!(badges.by_name["docs"].kind, BadgeKind::Clean);
    assert_eq!(badges.by_name["nested"].kind, BadgeKind::Modified);
    assert!(badges.revision >= 40);
    assert!(h.scan.stop());
}
