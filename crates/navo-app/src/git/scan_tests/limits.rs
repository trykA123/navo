use super::*;
use crate::git::scan::{listing, queue};

#[test]
fn queue_capacity_drops_all_requests_from_oldest_listing() {
    let mut shared = queue::Shared::default();
    let first = queue::Listing::new(&location("first"), 1, Vec::new()).unwrap();
    let second = queue::Listing::new(&location("second"), 2, Vec::new()).unwrap();
    shared.observe(first.clone());
    shared.observe(second.clone());
    for index in 0..253 {
        shared.push(queue::Request::Status(
            first.clone(),
            format!("repo-{index}").into(),
        ));
    }
    assert_eq!(shared.queue.len(), 255);
    shared.push(queue::Request::Status(second.clone(), "repo-254".into()));
    assert_eq!(shared.queue.len(), 256);
    shared.push(queue::Request::Status(second.clone(), "repo-255".into()));
    assert!(shared.queue.len() <= 256);
    assert!(
        shared
            .queue
            .iter()
            .all(|q| !Arc::ptr_eq(q.listing(), &first))
    );
}

#[test]
fn folder_probe_cap_is_two_thousand_and_visible_rows_come_first() {
    for count in [1999, 2000, 2001, 3000] {
        let probe = FakeProbe::default();
        let rows = (0..count)
            .map(|index| Row {
                name: format!("row-{index}"),
                on_screen: index == count - 1,
            })
            .collect();
        let listing = queue::Listing::new(&location("fixture"), 1, rows).unwrap();
        assert_eq!(listing.rows[0].name, format!("row-{}", count - 1));
        listing::discover(listing, &probe, false, Limits::default());
        assert_eq!(probe.probes.load(Ordering::SeqCst), count.min(2000) + 1);
    }
}

#[test]
fn probe_budget_stops_at_five_hundred_ms() {
    let probe = FakeProbe {
        step: 100,
        ..FakeProbe::default()
    };
    let rows = (0..100)
        .map(|i| Row {
            name: format!("row-{i}"),
            on_screen: false,
        })
        .collect();
    let listing = queue::Listing::new(&location("fixture"), 1, rows).unwrap();
    listing::discover(listing, &probe, false, Limits::default());
    assert!(probe.probes.load(Ordering::SeqCst) <= 4);
}

#[test]
fn repo_cap_is_sixty_four_and_on_screen_repos_take_priority() {
    for count in [63, 64, 65, 100] {
        let rows = (0..count)
            .map(|i| Row {
                name: format!("repo-{i}"),
                on_screen: i == count - 1,
            })
            .collect();
        let probe = FakeProbe {
            repos: (0..count)
                .map(|i| PathBuf::from(format!("fixture/repo-{i}")))
                .collect(),
            ..FakeProbe::default()
        };
        let listing = queue::Listing::new(&location("fixture"), 1, rows).unwrap();
        let found = listing::discover(listing, &probe, false, Limits::default());
        assert_eq!(found.roots.len(), count.min(64));
        assert_eq!(
            found.roots[0],
            PathBuf::from(format!("fixture/repo-{}", count - 1))
        );
    }
}

#[test]
fn network_is_skipped_unless_enabled_and_removable_is_scanned() {
    let p = Arc::new(FakeProbe {
        repos: [PathBuf::from("fixture/a")].into(),
        network: true,
        ..FakeProbe::default()
    });
    let h = Harness::new(p.clone());
    h.observe("fixture", 1, &["a"]);
    h.wait("fixture", |b| b.revision >= 1);
    assert_eq!(p.probes.load(Ordering::SeqCst), 0);
    assert!(h.calls.try_recv().is_err());
    assert!(h.scan.stop());
    let settings = GitSettings {
        network: true,
        ..GitSettings::default()
    };
    let h = Harness::configured(p, settings, Limits::default());
    h.observe("fixture", 1, &["a"]);
    assert_eq!(h.next().0, PathBuf::from("fixture/a"));
    assert!(h.scan.stop());
}

#[test]
fn only_two_status_calls_run_at_once() {
    let h = Harness::new(probe(&["fixture/a", "fixture/b", "fixture/c"]));
    h.observe("fixture", 1, &["a", "b", "c"]);
    h.next();
    h.next();
    assert!(h.calls.try_recv().is_err());
    h.reply(Ok(clean()));
    assert_eq!(h.next().0, PathBuf::from("fixture/c"));
    assert!(h.scan.stop());
}

#[test]
fn a_slot_held_outside_the_scanner_counts_against_its_two_processes() {
    let h = Harness::new(probe(&["fixture/a", "fixture/b", "fixture/c"]));
    let outside = h.scan.slots().try_acquire().unwrap();
    h.observe("fixture", 1, &["a", "b", "c"]);
    h.next();
    std::thread::sleep(Duration::from_millis(100));
    assert!(h.calls.try_recv().is_err());
    drop(outside);
    h.next();
    assert!(h.scan.stop());
}

#[test]
fn disabled_scanner_and_nonlocal_listing_never_probe() {
    let p = probe(&["fixture/a"]);
    let h = Harness::configured(
        p.clone(),
        GitSettings {
            badges: false,
            ..GitSettings::default()
        },
        Limits::default(),
    );
    h.observe("fixture", 1, &["a"]);
    assert!(h.scan.running.lock().unwrap().is_none());
    assert!(h.scan.badges(&location("fixture")).is_none());
    assert_eq!(p.probes.load(Ordering::SeqCst), 0);
    assert!(h.scan.stop());
    let h = Harness::new(p.clone());
    h.observe("archive://fixture", 1, &["a"]);
    assert!(h.scan.shared.0.lock().unwrap().active.is_empty());
    assert_eq!(p.probes.load(Ordering::SeqCst), 0);
    assert!(h.scan.stop());
}

#[test]
fn system_probe_accepts_dot_git_file_or_directory() {
    let dir = tempfile::tempdir().unwrap();
    let p = SystemProbe;
    assert!(!p.has_git(dir.path()));
    std::fs::write(dir.path().join(".git"), "gitdir: elsewhere").unwrap();
    assert!(p.has_git(dir.path()));
    std::fs::remove_file(dir.path().join(".git")).unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    assert!(p.has_git(dir.path()));
    let _class = p.drive_class(&location("fixture"));
}

/// Blocks `has_git` under `stuck` until the sender of `release` is dropped, as a dead share does.
struct StuckProbe {
    release: Mutex<mpsc::Receiver<()>>,
}

impl Probe for StuckProbe {
    fn has_git(&self, folder: &Path) -> bool {
        if folder.starts_with("stuck") {
            let _ = self.release.lock().unwrap().recv();
        }
        folder.ends_with("repo")
    }

    fn drive_class(&self, listing: &Location) -> DriveClass {
        if listing.as_str().starts_with("stuck") {
            DriveClass::Network
        } else {
            DriveClass::Removable
        }
    }
}

#[test]
fn a_probe_stuck_on_a_network_share_does_not_block_the_scanner() {
    let (hold, release) = mpsc::channel::<()>();
    let probe = Arc::new(StuckProbe {
        release: Mutex::new(release),
    });
    let settings = GitSettings {
        network: true,
        ..GitSettings::default()
    };
    let limits = Limits {
        probe_timeout: Duration::from_millis(20),
        ..Limits::default()
    };
    let (started, calls) = mpsc::channel();
    let source = Arc::new(FakeSource::default());
    *source.started.lock().unwrap() = Some(started);
    let ports = Ports {
        probe,
        source: source.clone(),
        saved: None,
    };
    let scan = GitScan::start_with_limits(ports, &settings, Box::new(|_| {}), limits).unwrap();
    let rows = |name: &str| {
        vec![Row {
            name: name.to_owned(),
            on_screen: false,
        }]
    };
    scan.observe(&location("stuck"), 1, rows("repo"));
    scan.observe(&location("fixture"), 1, rows("repo"));
    let (root, _) = calls.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(root, PathBuf::from("fixture/repo"));
    drop(hold);
    assert!(scan.stop());
}

fn network_limits() -> (GitSettings, Limits) {
    let settings = GitSettings {
        network: true,
        ..GitSettings::default()
    };
    let limits = Limits {
        probe_timeout: Duration::from_millis(20),
        ..Limits::default()
    };
    (settings, limits)
}

#[test]
fn a_probe_that_hits_its_deadline_marks_the_listing_slow() {
    let (hold, release) = mpsc::channel::<()>();
    let probe = Arc::new(StuckProbe {
        release: Mutex::new(release),
    });
    let (settings, limits) = network_limits();
    let h = Harness::build(probe, move |ports, notify| {
        GitScan::start_with_limits(ports, &settings, notify, limits).unwrap()
    });
    h.observe("stuck", 1, &["repo"]);
    h.observe("fixture", 2, &["repo"]);
    let slow = h.wait("stuck", |b| b.revision >= 1);
    assert!(slow.slow_drive);
    assert!(slow.by_name.is_empty());
    let normal = h.wait("fixture", |b| b.revision >= 2);
    assert!(!normal.slow_drive);
    drop(hold);
    assert!(h.scan.stop());
}

#[test]
fn a_fast_network_listing_is_not_marked_slow() {
    let p = Arc::new(FakeProbe {
        repos: [PathBuf::from("fixture/a")].into(),
        network: true,
        ..FakeProbe::default()
    });
    let (settings, _) = network_limits();
    let h = Harness::configured(p, settings, Limits::default());
    h.observe("fixture", 1, &["a"]);
    let badges = h.wait("fixture", |b| b.revision >= 1);
    assert!(!badges.slow_drive);
    assert!(h.scan.stop());
}

#[test]
fn git_processes_run_at_half_the_cores_from_two_to_four() {
    let caps: Vec<usize> = [1, 2, 4, 6, 8, 64].map(process_cap).into();
    assert_eq!(caps, [2, 2, 2, 3, 4, 4]);
}
