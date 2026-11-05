use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use navo_core::FileIdentity;
use navo_git::status::{Fingerprint, fingerprint};
use navo_git::trust::Refusal;

use crate::git::scan::RepoWatch;

use super::*;

/// A settled local root: one fixed fingerprint, a live watch that never changes, and a fixed
/// folder identity, so a status can earn a proof.
struct SettledProbe {
    repos: HashSet<PathBuf>,
    print: Fingerprint,
}

struct SettledWatch;

impl RepoWatch for SettledWatch {
    fn is_live(&self) -> bool {
        true
    }

    fn changes(&self) -> u64 {
        0
    }
}

impl Probe for SettledProbe {
    fn has_git(&self, folder: &Path) -> bool {
        self.repos.contains(folder)
    }

    fn drive_class(&self, _: &Location) -> DriveClass {
        DriveClass::Ssd
    }

    fn fingerprint(&self, _: &Path, _: Option<&str>) -> Option<Fingerprint> {
        Some(self.print)
    }

    fn watch_tree(&self, _: &Path) -> Option<Box<dyn RepoWatch>> {
        Some(Box::new(SettledWatch))
    }

    fn root_class(&self, _: &Path, _: &Location) -> DriveClass {
        DriveClass::Ssd
    }

    fn identity(&self, _: &Path) -> Option<FileIdentity> {
        Some(FileIdentity::fingerprint(7, Some(7), true))
    }
}

#[derive(Default)]
struct RecordingSource {
    calls: Mutex<Vec<PathBuf>>,
}

impl StatusSource for RecordingSource {
    fn status(&self, root: &Path, _: &Cancel) -> Result<RepoStatus, ScanError> {
        self.calls.lock().unwrap().push(root.to_path_buf());
        Ok(clean())
    }
}

fn settled_print() -> Fingerprint {
    let dir = tempfile::tempdir().unwrap();
    let head = dir.path().join(".git/HEAD");
    std::fs::create_dir_all(head.parent().unwrap()).unwrap();
    std::fs::write(&head, "ref: refs/heads/main\n").unwrap();
    let old = SystemTime::now() - Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&head)
        .unwrap()
        .set_modified(old)
        .unwrap();
    fingerprint(dir.path(), None, SystemTime::now()).unwrap()
}

fn wait_until(ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn badges_off_starts_nothing_and_publishes_nothing() {
    let rig = rig(&settings(false));
    rig.observe();
    std::thread::sleep(Duration::from_millis(50));
    assert!(rig.starts.lock().unwrap().is_empty());
    assert!(!rig.hooks.running());
    assert_eq!(rig.calls(), 0);
    assert!(rig.hooks.badges(&rig.folder).is_none());
}

#[test]
fn a_fake_status_reaches_badges() {
    let rig = rig(&settings(true));
    *rig.source.reply.lock().unwrap() = Some(Ok(dirty()));
    rig.observe();
    rig.wait_for(|rig| rig.fresh_kind().is_some());
    assert_eq!(rig.fresh_kind(), Some(BadgeKind::Modified));
}

#[test]
fn source_errors_reach_badges_as_unknown_with_their_reason() {
    for (error, reason) in [
        (ScanError::NoGit, BadgeReason::NoGit),
        (ScanError::Refused(Refusal::Guard), BadgeReason::Guard),
        (ScanError::Refused(Refusal::Filter), BadgeReason::Filter),
        (
            ScanError::Refused(Refusal::Ownership),
            BadgeReason::Ownership,
        ),
    ] {
        let rig = rig(&settings(true));
        *rig.source.reply.lock().unwrap() = Some(Err(error));
        rig.observe();
        rig.wait_for(|rig| rig.fresh_kind().is_some());
        let badges = rig.hooks.badges(&rig.folder).unwrap();
        assert_eq!(badges.by_name["repo"].kind, BadgeKind::Unknown);
        assert_eq!(badges.by_name["repo"].reason, Some(reason));
    }
}

#[test]
fn toggling_off_stops_the_scanner_and_on_starts_a_new_one() {
    let rig = rig(&settings(true));
    assert!(rig.hooks.running());
    rig.hooks.apply(&settings(false));
    assert!(!rig.hooks.running());
    rig.observe();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(rig.calls(), 0);
    rig.hooks.apply(&settings(true));
    assert!(rig.hooks.running());
    assert_eq!(rig.starts.lock().unwrap().len(), 2);
}

#[test]
fn observations_made_while_off_are_scanned_once_it_turns_on() {
    let rig = rig(&settings(false));
    rig.observe();
    rig.hooks.apply(&settings(true));
    rig.wait_for(|rig| rig.fresh_kind().is_some());
}

#[test]
fn toggling_leaves_one_subscription_that_fires_once_per_publish() {
    let rig = rig(&settings(true));
    let fired = Arc::new(AtomicUsize::new(0));
    let counter = fired.clone();
    rig.hooks.subscribe(Box::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    }));
    for _ in 0..3 {
        rig.hooks.apply(&settings(false));
        rig.hooks.apply(&settings(true));
    }
    rig.hooks.apply(&settings(false));
    let before = fired.load(Ordering::SeqCst);
    rig.observe();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fired.load(Ordering::SeqCst), before);
    rig.hooks.apply(&settings(true));
    rig.wait_for(|rig| rig.fresh_kind().is_some());
    let published = fired.load(Ordering::SeqCst) - before;
    assert!((1..=3).contains(&published), "{published}");
}

#[test]
fn only_scanner_relevant_settings_restart_it() {
    let rig = rig(&settings(true));
    let mut other = settings(true);
    other.tool = "fork".to_owned();
    rig.hooks.apply(&other);
    assert_eq!(rig.starts.lock().unwrap().len(), 1);
    other.fetch = true;
    rig.hooks.apply(&other);
    assert_eq!(rig.starts.lock().unwrap().len(), 2);
    other.fetch_minutes = 30;
    rig.hooks.apply(&other);
    assert_eq!(rig.starts.lock().unwrap().len(), 2);
    other.engine = GitEngine::Git;
    rig.hooks.apply(&other);
    assert_eq!(rig.starts.lock().unwrap().last(), Some(&GitEngine::Git));
    other.network = true;
    rig.hooks.apply(&other);
    assert_eq!(rig.starts.lock().unwrap().len(), 4);
}

#[test]
fn rescan_runs_the_status_again() {
    let rig = rig(&settings(true));
    rig.observe();
    rig.wait_for(|rig| rig.calls() == 1 && rig.fresh_kind().is_some());
    rig.hooks.rescan(&rig.folder);
    rig.wait_for(|rig| rig.calls() == 2);
}

#[test]
fn focus_rescans_only_when_the_status_is_over_60_seconds_old() {
    let rig = rig(&settings(true));
    rig.observe();
    rig.wait_published();
    rig.advance(Duration::from_secs(30));
    rig.hooks.focus_regained();
    rig.stays_at(1);
    rig.advance(Duration::from_secs(31));
    rig.hooks.focus_regained();
    rig.wait_for(|rig| rig.calls() == 2);
    rig.hooks.focus_regained();
    rig.stays_at(2);
}

#[test]
fn the_window_reaches_the_focus_rescan_through_the_ui_trait() {
    let rig = rig(&settings(true));
    rig.observe();
    rig.wait_published();
    let ui: &dyn GitHooks = &rig.hooks;
    ui.focus_regained();
    rig.stays_at(1);
    rig.advance(Duration::from_secs(61));
    ui.focus_regained();
    rig.wait_for(|rig| rig.calls() == 2);
}

#[test]
fn forgotten_folders_are_not_rescanned_on_focus() {
    let rig = rig(&settings(true));
    rig.observe();
    rig.wait_published();
    rig.hooks.forget(&rig.folder);
    rig.advance(Duration::from_secs(120));
    rig.hooks.focus_regained();
    rig.stays_at(1);
}

#[test]
fn a_git_action_rescans_only_the_repository_it_ran_in() {
    let source = Arc::new(RecordingSource::default());
    let probe = Arc::new(SettledProbe {
        repos: ["fixture/a", "fixture/b"]
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        print: settled_print(),
    });
    let factory: Factory = {
        let (source, probe) = (source.clone(), probe.clone());
        Box::new(move |_| Ports {
            source: source.clone(),
            probe: probe.clone(),
            saved: None,
        })
    };
    let hooks = Hooks::new(
        &settings(true),
        factory,
        Box::new(Instant::now),
        Tools::new(
            Arc::new(FakeTools::default()),
            FakeSync::replying(Ok(pulled())),
        ),
    );
    let listing = Location::new("fixture").unwrap();
    hooks.observe(&listing, 1, vec!["a".to_owned(), "b".to_owned()]);
    wait_until(|| source.calls.lock().unwrap().len() == 2);
    wait_until(|| {
        hooks.badges(&listing).is_some_and(|badges| {
            badges.by_name.len() == 2 && badges.by_name.values().all(|badge| !badge.stale)
        })
    });
    source.calls.lock().unwrap().clear();

    let rescan = hooks.rescan_of(&Location::new("fixture/b").unwrap());
    rescan();
    wait_until(|| !source.calls.lock().unwrap().is_empty());
    std::thread::sleep(Duration::from_millis(80));
    let calls = source.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1, "only the named repository is rescanned");
    assert!(calls[0].ends_with("b"), "{calls:?}");
}

#[test]
fn the_fetch_timer_runs_only_with_badges_and_fetch_on() {
    let fetching = |badges, fetch| GitSettings {
        badges,
        fetch,
        ..GitSettings::default()
    };
    let rig = rig(&fetching(true, false));
    assert!(!rig.hooks.fetching());
    drop(rig.hooks.start(&fetching(true, true)));
    assert!(rig.hooks.fetching());
    drop(rig.hooks.start(&fetching(false, true)));
    assert!(!rig.hooks.fetching());
    drop(rig.hooks.start(&fetching(true, true)));
    drop(rig.hooks.start(&fetching(true, false)));
    assert!(!rig.hooks.fetching());
}
