use std::sync::mpsc::{self, Receiver, Sender};

use navo_ui::git::RepoBadge;

use super::super::super::fetch::FetchTimer;
use super::super::super::fetch::ports::{Clock, Fetcher, Outcome, Watch};
use super::super::super::scan::{ScanRepo, Slots};
use super::*;

fn revision_of(rig: &Rig) -> u64 {
    rig.hooks.badges(&rig.folder).map_or(0, |b| b.revision)
}

#[test]
fn revisions_keep_rising_when_the_scanner_restarts() {
    let rig = rig(&settings(true));
    rig.observe();
    rig.wait_for(|rig| rig.fresh_kind().is_some());
    let before = revision_of(&rig);
    let mut other = settings(true);
    other.engine = GitEngine::Git;
    rig.hooks.apply(&other);
    rig.wait_for(|rig| rig.calls() == 2 && rig.fresh_kind().is_some());
    assert!(revision_of(&rig) > before);
}

#[test]
fn turning_badges_off_and_on_tells_the_subscribers_and_empties_the_badges() {
    let rig = rig(&settings(true));
    rig.observe();
    rig.wait_for(|rig| rig.fresh_kind().is_some());
    let told = rig.published.load(Ordering::SeqCst);
    rig.hooks.apply(&settings(false));
    assert!(rig.published.load(Ordering::SeqCst) > told);
    assert!(rig.hooks.badges(&rig.folder).is_none());
    let told = rig.published.load(Ordering::SeqCst);
    rig.hooks.apply(&settings(true));
    assert!(rig.published.load(Ordering::SeqCst) > told);
    rig.wait_for(|rig| rig.fresh_kind().is_some());
}

fn fetching(minutes: u16) -> GitSettings {
    GitSettings {
        badges: true,
        fetch: true,
        fetch_minutes: minutes,
        ..GitSettings::default()
    }
}

#[test]
fn a_new_fetch_interval_restarts_the_timer_and_not_the_scanner() {
    let mut rig = rig(&fetching(15));
    let minutes = Arc::new(Mutex::new(Vec::new()));
    let seen = minutes.clone();
    rig.hooks.timers = Box::new(move |settings, _| {
        seen.lock().unwrap().push(settings.fetch_minutes);
        None
    });
    rig.hooks.apply(&fetching(30));
    assert_eq!(*minutes.lock().unwrap(), [30]);
    assert_eq!(rig.starts.lock().unwrap().len(), 1);
}

struct OneRepo;

impl Watch for OneRepo {
    fn repos(&self) -> Vec<ScanRepo> {
        vec![ScanRepo {
            listing: Location::new("fixture").unwrap(),
            path: PathBuf::from("fixture/repo"),
            badge: RepoBadge {
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
            },
        }]
    }

    fn rescan(&self, _: &Location) {}

    fn drive_class(&self, _: &Location) -> DriveClass {
        DriveClass::Removable
    }
}

struct Epoch;

impl Clock for Epoch {
    fn now_ms(&self) -> i64 {
        1_000_000_000
    }
}

/// A fetch that tells when it was cancelled and then ignores it until released, like a hung Git.
struct HungFetch {
    cancelled: Sender<()>,
    release: Mutex<Receiver<()>>,
}

impl Fetcher for HungFetch {
    fn fetch(&self, _: &Path, cancel: &Cancel) -> Outcome {
        while !cancel.is_cancelled() {
            std::thread::sleep(Duration::from_millis(1));
        }
        self.cancelled.send(()).unwrap();
        let _ = self.release.lock().unwrap().recv();
        Outcome::Cancelled
    }
}

#[test]
fn a_fetch_timer_that_is_slow_to_stop_never_holds_the_hooks_state() {
    let mut rig = rig(&fetching(15));
    let (cancelled, stopping) = mpsc::channel();
    let (hold, release) = mpsc::channel::<()>();
    let fetcher = Arc::new(HungFetch {
        cancelled,
        release: Mutex::new(release),
    });
    rig.hooks.timers = Box::new(move |settings, _| {
        let ports = fetch::Ports {
            watch: Arc::new(OneRepo),
            fetcher: fetcher.clone(),
            clock: Arc::new(Epoch),
            slots: Slots::new(2),
        };
        FetchTimer::start(settings, ports)
    });
    rig.hooks.apply(&fetching(30));
    std::thread::scope(|scope| {
        let stop = scope.spawn(|| rig.hooks.apply(&settings(true)));
        stopping.recv().unwrap();
        assert!(rig.hooks.state.try_lock().is_ok(), "the state stays locked");
        assert!(rig.hooks.tools().is_empty());
        rig.hooks.focus_regained();
        drop(hold);
        stop.join().unwrap();
    });
}

#[test]
fn a_second_pull_or_push_on_a_repository_is_refused_and_shutdown_cancels_the_first() {
    let wired = wired("git-cli", &[ToolKind::GitCli]);
    let previews = wired.previews();
    let shown = |action| {
        previews
            .iter()
            .find(|(held, _)| *held == action)
            .unwrap()
            .1
            .clone()
    };
    let (entered, running) = mpsc::channel();
    *wired.sync.hold.lock().unwrap() = Some(entered);
    std::thread::scope(|scope| {
        let pull =
            scope.spawn(|| wired.run_shown(GitAction::Pull, shown(GitAction::Pull).as_ref()));
        running.recv().unwrap();
        let refused = wired.run_shown(GitAction::Push, shown(GitAction::Push).as_ref());
        assert_eq!(refused.unwrap_err(), navo_l10n::tr("git-fail-busy"));
        wired.rig.hooks.shutdown();
        assert!(pull.join().unwrap().is_err());
    });
    assert_eq!(wired.sync.runs.lock().unwrap().len(), 1);
}
