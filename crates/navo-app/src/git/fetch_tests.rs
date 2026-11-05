use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use navo_core::Location;
use navo_fs::DriveClass;
use navo_store::GitSettings;
use navo_ui::git::{BadgeKind, BadgeReason, RepoBadge};

use super::engine::{Engine, Step};
use super::ports::{Clock, Fetcher, Outcome, Watch};
use super::*;
use crate::git::scan::{Cancel, ScanRepo, Slots};

const MINUTE: i64 = 60_000;

#[derive(Default)]
struct FakeWatch {
    repos: Mutex<Vec<ScanRepo>>,
    network: Mutex<Vec<Location>>,
    rescans: Mutex<Vec<(Location, PathBuf)>>,
}

impl Watch for FakeWatch {
    fn repos(&self) -> Vec<ScanRepo> {
        self.repos.lock().unwrap().clone()
    }

    fn rescan(&self, repo: &ScanRepo) {
        self.rescans
            .lock()
            .unwrap()
            .push((repo.listing.clone(), repo.path.clone()));
    }

    fn drive_class(&self, listing: &Location) -> DriveClass {
        if self.network.lock().unwrap().contains(listing) {
            DriveClass::Network
        } else {
            DriveClass::Removable
        }
    }
}

#[derive(Default)]
struct FakeFetcher {
    calls: Mutex<Vec<PathBuf>>,
    outcomes: Mutex<HashMap<PathBuf, Outcome>>,
    slots: Mutex<Option<Slots>>,
    free_during_fetch: Mutex<Vec<bool>>,
    block: AtomicBool,
    entered: AtomicBool,
    saw_cancel: AtomicBool,
}

impl Fetcher for FakeFetcher {
    fn fetch(&self, repo: &Path, cancel: &Cancel) -> Outcome {
        self.calls.lock().unwrap().push(repo.to_path_buf());
        if let Some(slots) = self.slots.lock().unwrap().as_ref() {
            self.free_during_fetch
                .lock()
                .unwrap()
                .push(slots.try_acquire().is_some());
        }
        if self.block.load(Ordering::SeqCst) {
            self.entered.store(true, Ordering::SeqCst);
            while !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            self.saw_cancel.store(true, Ordering::SeqCst);
            return Outcome::Cancelled;
        }
        self.outcomes
            .lock()
            .unwrap()
            .get(repo)
            .copied()
            .unwrap_or(Outcome::Fetched)
    }
}

struct FakeClock(AtomicI64);

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct Rig {
    watch: Arc<FakeWatch>,
    fetcher: Arc<FakeFetcher>,
    clock: Arc<FakeClock>,
    slots: Slots,
}

impl Rig {
    fn new() -> Self {
        Self {
            watch: Arc::default(),
            fetcher: Arc::default(),
            clock: Arc::new(FakeClock(AtomicI64::new(1_000 * MINUTE))),
            slots: Slots::new(2),
        }
    }

    fn ports(&self) -> Ports {
        Ports {
            watch: self.watch.clone(),
            fetcher: self.fetcher.clone(),
            clock: self.clock.clone(),
            slots: self.slots.clone(),
        }
    }

    fn engine(&self, settings: &GitSettings) -> Engine {
        Engine::new(self.ports(), settings)
    }

    fn add(&self, name: &str, badge: RepoBadge) {
        self.watch.repos.lock().unwrap().push(ScanRepo {
            listing: listing(),
            path: PathBuf::from("fixture").join(name),
            badge,
        });
    }

    fn advance(&self, ms: i64) {
        self.clock.0.fetch_add(ms, Ordering::SeqCst);
    }

    fn fetched(&self) -> Vec<PathBuf> {
        self.fetcher.calls.lock().unwrap().clone()
    }
}

fn listing() -> Location {
    Location::new("fixture").unwrap()
}

fn badge() -> RepoBadge {
    RepoBadge {
        kind: BadgeKind::Clean,
        is_root: true,
        stale: false,
        branch: Some("main".to_owned()),
        upstream: Some("origin/main".to_owned()),
        ahead: 0,
        behind: 0,
        changed: 0,
        untracked: 0,
        conflicted: 0,
        fetched_ms: None,
        reason: None,
    }
}

fn settings(minutes: u16) -> GitSettings {
    GitSettings {
        fetch: true,
        fetch_minutes: minutes,
        ..GitSettings::default()
    }
}

fn run_until_idle(engine: &mut Engine) {
    let cancel = Cancel::new();
    while engine.tick(&cancel) == Step::Ran {}
}

#[test]
fn with_fetch_off_no_timer_and_no_fetch_happen() {
    let rig = Rig::new();
    rig.add("a", badge());
    let off = GitSettings {
        fetch: false,
        ..GitSettings::default()
    };
    assert!(FetchTimer::start(&off, rig.ports()).is_none());
    assert!(rig.fetched().is_empty());
}

#[test]
fn a_repository_is_fetched_once_per_interval() {
    let rig = Rig::new();
    rig.add("a", badge());
    let mut engine = rig.engine(&settings(10));
    run_until_idle(&mut engine);
    assert_eq!(rig.fetched().len(), 1);
    rig.advance(10 * MINUTE - 1);
    run_until_idle(&mut engine);
    assert_eq!(rig.fetched().len(), 1);
    rig.advance(1);
    run_until_idle(&mut engine);
    assert_eq!(rig.fetched().len(), 2);
}

#[test]
fn a_repository_fetched_within_the_interval_is_left_alone() {
    let rig = Rig::new();
    let now = rig.clock.now_ms();
    rig.add(
        "recent",
        RepoBadge {
            fetched_ms: Some(now - 5 * MINUTE),
            ..badge()
        },
    );
    let mut engine = rig.engine(&settings(10));
    run_until_idle(&mut engine);
    assert!(rig.fetched().is_empty());
    rig.advance(5 * MINUTE);
    run_until_idle(&mut engine);
    assert_eq!(rig.fetched().len(), 1);
}

#[test]
fn repositories_without_a_usable_state_are_skipped() {
    let rig = Rig::new();
    rig.add(
        "nogit",
        RepoBadge {
            kind: BadgeKind::Unknown,
            reason: Some(BadgeReason::NoGit),
            ..badge()
        },
    );
    rig.add(
        "untrusted",
        RepoBadge {
            kind: BadgeKind::Unknown,
            reason: Some(BadgeReason::Guard),
            ..badge()
        },
    );
    rig.add(
        "local-only",
        RepoBadge {
            upstream: None,
            ..badge()
        },
    );
    let mut engine = rig.engine(&settings(5));
    run_until_idle(&mut engine);
    assert!(rig.fetched().is_empty());
}

#[test]
fn network_listings_wait_for_the_network_setting() {
    let rig = Rig::new();
    rig.add("remote", badge());
    rig.watch.network.lock().unwrap().push(listing());
    let mut engine = rig.engine(&settings(5));
    run_until_idle(&mut engine);
    assert!(rig.fetched().is_empty());
    let allowed = GitSettings {
        network: true,
        ..settings(5)
    };
    let mut engine = rig.engine(&allowed);
    run_until_idle(&mut engine);
    assert_eq!(rig.fetched().len(), 1);
}

#[test]
fn a_guarded_repository_is_refused_once_per_interval_and_never_rescanned() {
    let rig = Rig::new();
    rig.add("guarded", badge());
    rig.fetcher
        .outcomes
        .lock()
        .unwrap()
        .insert(PathBuf::from("fixture/guarded"), Outcome::Refused);
    let mut engine = rig.engine(&settings(5));
    run_until_idle(&mut engine);
    run_until_idle(&mut engine);
    assert_eq!(rig.fetched().len(), 1);
    assert!(rig.watch.rescans.lock().unwrap().is_empty());
}

#[test]
fn a_failed_fetch_waits_for_the_next_interval() {
    let rig = Rig::new();
    rig.add("slow", badge());
    rig.fetcher
        .outcomes
        .lock()
        .unwrap()
        .insert(PathBuf::from("fixture/slow"), Outcome::Failed);
    let mut engine = rig.engine(&settings(5));
    run_until_idle(&mut engine);
    rig.advance(5 * MINUTE - 1);
    run_until_idle(&mut engine);
    assert_eq!(rig.fetched().len(), 1);
    assert!(rig.watch.rescans.lock().unwrap().is_empty());
    rig.advance(1);
    run_until_idle(&mut engine);
    assert_eq!(rig.fetched().len(), 2);
}

#[test]
fn a_successful_fetch_rescans_its_listing_and_fetches_one_at_a_time() {
    let rig = Rig::new();
    rig.add("a", badge());
    rig.add("b", badge());
    *rig.fetcher.slots.lock().unwrap() = Some(rig.slots.clone());
    let mut engine = rig.engine(&settings(5));
    let cancel = Cancel::new();
    assert_eq!(engine.tick(&cancel), Step::Ran);
    assert_eq!(rig.fetched().len(), 1);
    assert_eq!(engine.tick(&cancel), Step::Ran);
    assert_eq!(engine.tick(&cancel), Step::Idle);
    assert_eq!(rig.fetched().len(), 2);
    let rescanned: Vec<Location> = rig
        .watch
        .rescans
        .lock()
        .unwrap()
        .iter()
        .map(|(listing, _)| listing.clone())
        .collect();
    assert_eq!(rescanned, vec![listing(); 2]);
}

#[test]
fn a_successful_fetch_rescans_only_the_repository_it_fetched() {
    let rig = Rig::new();
    rig.add("a", badge());
    rig.add("b", badge());
    let mut engine = rig.engine(&settings(5));
    let cancel = Cancel::new();
    assert_eq!(engine.tick(&cancel), Step::Ran);
    assert_eq!(
        *rig.watch.rescans.lock().unwrap(),
        vec![(listing(), PathBuf::from("fixture/a"))]
    );
    assert_eq!(engine.tick(&cancel), Step::Ran);
    assert_eq!(
        *rig.watch.rescans.lock().unwrap(),
        vec![
            (listing(), PathBuf::from("fixture/a")),
            (listing(), PathBuf::from("fixture/b")),
        ]
    );
}

#[test]
fn a_repository_that_is_no_longer_observed_is_not_fetched() {
    let rig = Rig::new();
    rig.add("gone", badge());
    let mut engine = rig.engine(&settings(5));
    rig.watch.repos.lock().unwrap().clear();
    run_until_idle(&mut engine);
    assert!(rig.fetched().is_empty());
}

#[test]
fn a_fetch_holds_one_of_the_two_process_slots() {
    let rig = Rig::new();
    rig.add("a", badge());
    *rig.fetcher.slots.lock().unwrap() = Some(rig.slots.clone());
    let scanner_slot = rig.slots.try_acquire().unwrap();
    let mut engine = rig.engine(&settings(5));
    run_until_idle(&mut engine);
    assert_eq!(*rig.fetcher.free_during_fetch.lock().unwrap(), [false]);
    drop(scanner_slot);
    assert!(rig.slots.try_acquire().is_some());
}

#[test]
fn no_free_slot_defers_the_fetch() {
    let rig = Rig::new();
    rig.add("a", badge());
    let held = [
        rig.slots.try_acquire().unwrap(),
        rig.slots.try_acquire().unwrap(),
    ];
    let mut engine = rig.engine(&settings(5));
    assert_eq!(engine.tick(&Cancel::new()), Step::Busy);
    assert!(rig.fetched().is_empty());
    drop(held);
    assert_eq!(engine.tick(&Cancel::new()), Step::Ran);
}

#[test]
fn stop_cancels_a_running_fetch_quickly() {
    let rig = Rig::new();
    rig.add("a", badge());
    rig.fetcher.block.store(true, Ordering::SeqCst);
    let mut timer = FetchTimer::start(&settings(5), rig.ports()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !rig.fetcher.entered.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "fetch never started");
        std::thread::yield_now();
    }
    let started = Instant::now();
    assert!(timer.stop());
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(rig.fetcher.saw_cancel.load(Ordering::SeqCst));
    assert!(rig.watch.rescans.lock().unwrap().is_empty());
    assert!(rig.slots.try_acquire().is_some());
}

#[test]
fn slots_cap_at_the_limit_and_free_on_drop() {
    let slots = Slots::new(2);
    let first = slots.try_acquire();
    let second = slots.try_acquire();
    assert!(first.is_some() && second.is_some());
    assert!(slots.try_acquire().is_none());
    drop(first);
    assert!(slots.try_acquire().is_some());
}
