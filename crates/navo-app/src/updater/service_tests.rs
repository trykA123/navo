use super::*;
use navo_update::fake::{DownloadStep, FakeFeed};
use std::time::Duration;

fn version(text: &str) -> Version {
    Version::parse(text).expect("version")
}

fn release(text: &str) -> Found {
    Found::Release(Release {
        version: version(text),
        size: 1,
    })
}

fn feed(kind: InstallKind) -> Arc<FakeFeed> {
    Arc::new(FakeFeed::new(kind, version("0.0.1-alpha.20261103c")))
}

fn view_of(updater: &Arc<Updater>) -> UpdateView {
    (updater.hooks().state)()
}

fn poll_until<T>(attempts: usize, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    for _ in 0..attempts {
        if let Some(found) = check() {
            return Some(found);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

fn wait_for(updater: &Arc<Updater>, wanted: impl Fn(&UpdateView) -> bool) -> UpdateView {
    poll_until(1000, || {
        let view = view_of(updater);
        wanted(&view).then_some(view)
    })
    .unwrap_or_else(|| panic!("the state never matched: {:?}", view_of(updater)))
}

const LONG: Duration = Duration::from_secs(3600);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

#[test]
fn a_portable_copy_stays_hidden_and_asks_nothing() {
    let fake = feed(InstallKind::Portable);
    let updater = Updater::start(fake.clone(), Duration::ZERO);
    (updater.hooks().check_now)();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(view_of(&updater), UpdateView::Hidden);
    assert_eq!(fake.check_calls(), 0);
}

#[test]
fn an_installed_copy_waits_for_the_delay_before_it_checks() {
    let fake = feed(InstallKind::Installed);
    let updater = Updater::start(fake.clone(), LONG);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(view_of(&updater), UpdateView::Idle);
    assert_eq!(fake.check_calls(), 0);
}

#[test]
fn the_start_up_check_runs_after_the_delay_and_ends_up_to_date() {
    let fake = feed(InstallKind::Installed);
    fake.answer_check(Ok(Found::Nothing));
    let updater = Updater::start(fake.clone(), Duration::from_millis(20));
    wait_for(&updater, |view| *view == UpdateView::UpToDate);
    assert_eq!(fake.check_calls(), 1);
}

#[test]
fn a_setting_turned_off_before_the_delay_cancels_the_start_up_check() {
    let fake = feed(InstallKind::Installed);
    let updater = Updater::start(fake.clone(), Duration::from_millis(100));
    updater.set_auto(false);
    wait_for(&updater, |view| *view == UpdateView::Off);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(view_of(&updater), UpdateView::Off);
    assert_eq!(fake.check_calls(), 0);
}

#[test]
fn check_now_works_with_automatic_checks_off() {
    let fake = feed(InstallKind::Installed);
    fake.answer_check(Ok(Found::Nothing));
    let updater = Updater::start(fake.clone(), LONG);
    updater.set_auto(false);
    assert_eq!(view_of(&updater), UpdateView::Off);
    (updater.hooks().check_now)();
    wait_for(&updater, |view| *view == UpdateView::UpToDate);
    assert_eq!(fake.check_calls(), 1);
}

#[test]
fn a_found_release_downloads_and_becomes_ready_once_per_version() {
    let fake = feed(InstallKind::Installed);
    fake.answer_check(Ok(release("0.0.1-alpha.20261104a")));
    let steps = fake.hold_download();
    let updater = Updater::start(fake.clone(), LONG);
    let hooks = updater.hooks();
    (hooks.check_now)();
    steps.send(DownloadStep::Progress(40)).expect("progress");
    wait_for(&updater, |view| {
        matches!(view, UpdateView::Downloading { percent: 40, .. })
    });
    assert_eq!((hooks.take_ready_notice)(), None);
    steps.send(DownloadStep::Done(Ok(()))).expect("done");
    let ready = wait_for(&updater, |view| matches!(view, UpdateView::Ready { .. }));
    assert_eq!(
        ready,
        UpdateView::Ready {
            version: "0.0.1-alpha.20261104a".into()
        }
    );
    assert_eq!(
        (hooks.take_ready_notice)().as_deref(),
        Some("0.0.1-alpha.20261104a")
    );
    assert_eq!((hooks.take_ready_notice)(), None);
}

#[test]
fn a_package_staged_by_an_earlier_run_is_ready_after_the_start() {
    let fake = Arc::new(
        FakeFeed::new(InstallKind::Installed, version("0.0.1-alpha.20261103c"))
            .with_staged(version("0.0.1-alpha.20261104a")),
    );
    let updater = Updater::start(fake.clone(), Duration::ZERO);
    wait_for(&updater, |view| matches!(view, UpdateView::Ready { .. }));
    assert_eq!(fake.check_calls(), 0);
}

#[test]
fn a_failed_check_shows_its_failure() {
    let fake = feed(InstallKind::Installed);
    fake.answer_check(Err(FeedError::Offline));
    let updater = Updater::start(fake, LONG);
    (updater.hooks().check_now)();
    wait_for(&updater, |view| {
        *view == UpdateView::Failed(navo_ui::window_view::UpdateFailure::Offline)
    });
}

#[test]
fn the_version_is_the_packages_when_installed_else_the_build() {
    let updater = Updater::start(feed(InstallKind::Installed), LONG);
    assert_eq!(
        Updater::version_text(Some(&updater)),
        "0.0.1-alpha.20261103c"
    );
    assert_eq!(Updater::version_text(None), env!("CARGO_PKG_VERSION"));
}

#[test]
fn automatic_checks_repeat_daily_and_catch_up_once_after_sleep() {
    let fake = feed(InstallKind::Installed);
    let updater = Updater::start(fake.clone(), LONG);
    updater.set_keep_in_tray(true);
    let mut timer = Schedule::new(Duration::ZERO, START_DELAY);
    updater.tick(&mut timer, START_DELAY - Duration::from_secs(1));
    assert_eq!(fake.check_calls(), 0);
    for (now, calls) in [(START_DELAY, 1), (START_DELAY + DAY, 2), (DAY * 4, 3)] {
        fake.answer_check(Ok(Found::Nothing));
        updater.tick(&mut timer, now);
        wait_for(&updater, |view| {
            *view == UpdateView::UpToDate && fake.check_calls() == calls
        });
        updater.tick(&mut timer, now);
        assert!(!timer.take_due(now + DAY - Duration::from_secs(1)));
        updater.tick(&mut timer, now + DAY - Duration::from_secs(1));
        assert_eq!(fake.check_calls(), calls);
    }
}

#[test]
fn automatic_checks_skip_disabled_tray_off_and_portable_copies() {
    for (kind, auto, tray) in [
        (InstallKind::Installed, false, true),
        (InstallKind::Installed, true, false),
        (InstallKind::Portable, true, true),
    ] {
        let fake = feed(kind);
        let updater = Updater::start(fake.clone(), LONG);
        updater.set_auto(auto);
        updater.set_keep_in_tray(tray);
        let mut timer = Schedule::new(Duration::ZERO, Duration::ZERO);
        assert!(timer.take_due(Duration::ZERO));
        updater.tick(&mut timer, DAY);
        assert_eq!(fake.check_calls(), 0);
        assert!(lock(&updater.runner).is_none());
    }
}

#[test]
fn a_failed_automatic_check_retries_at_the_next_daily_tick() {
    let fake = feed(InstallKind::Installed);
    let updater = Updater::start(fake.clone(), LONG);
    updater.set_keep_in_tray(true);
    let mut timer = Schedule::new(Duration::ZERO, Duration::ZERO);
    fake.answer_check(Err(FeedError::Offline));
    updater.tick(&mut timer, Duration::ZERO);
    wait_for(&updater, |view| matches!(view, UpdateView::Failed(_)));
    updater.tick(&mut timer, DAY - Duration::from_secs(1));
    assert_eq!(fake.check_calls(), 1);
    fake.answer_check(Ok(Found::Nothing));
    updater.tick(&mut timer, DAY);
    wait_for(&updater, |view| *view == UpdateView::UpToDate);
    assert_eq!(fake.check_calls(), 2);
}

#[test]
fn daily_ticks_do_not_overlap_a_check_or_download() {
    let fake = feed(InstallKind::Installed);
    let held = fake.hold_check();
    let download = fake.hold_download();
    let updater = Updater::start(fake.clone(), LONG);
    updater.set_keep_in_tray(true);
    let mut timer = Schedule::new(Duration::ZERO, Duration::ZERO);
    updater.tick(&mut timer, Duration::ZERO);
    wait_for(&updater, |view| *view == UpdateView::Checking);
    updater.tick(&mut timer, DAY);
    held.send(Ok(release("0.4.0"))).expect("check answer");
    wait_for(&updater, |view| {
        matches!(view, UpdateView::Downloading { .. })
    });
    updater.tick(&mut timer, DAY * 2);
    download
        .send(DownloadStep::Done(Ok(())))
        .expect("download answer");
    wait_for(&updater, |view| matches!(view, UpdateView::Ready { .. }));
    assert_eq!(fake.check_calls(), 1);
    assert_eq!(fake.download_calls(), 1);
}

#[test]
fn restart_handoff_failure_uses_the_existing_failure_view() {
    use navo_ui::window_view::UpdateFailure;
    let fake = Arc::new(
        FakeFeed::new(InstallKind::Installed, version("0.0.1-alpha.20261103c"))
            .with_staged(version("0.0.1-alpha.20261104a")),
    );
    let updater = Updater::start(fake, Duration::ZERO);
    wait_for(&updater, |view| matches!(view, UpdateView::Ready { .. }));
    assert_eq!(
        updater.restart_with(|| Err(UpdateFailure::Busy)),
        Err(UpdateFailure::Busy)
    );
    assert_eq!(view_of(&updater), UpdateView::Failed(UpdateFailure::Busy));
}

#[test]
fn restart_handoff_rejects_portable_and_not_ready_without_applying() {
    for kind in [
        InstallKind::Portable,
        InstallKind::Unpackaged,
        InstallKind::Installed,
    ] {
        let updater = Updater::start(feed(kind), LONG);
        assert!(updater.restart_with(|| panic!("must not apply")).is_err());
    }
}

#[test]
fn a_daily_check_clears_a_restart_failure() {
    use navo_ui::window_view::UpdateFailure;
    let fake = Arc::new(
        FakeFeed::new(InstallKind::Installed, version("0.0.1-alpha.20261103c"))
            .with_staged(version("0.0.1-alpha.20261104a")),
    );
    let updater = Updater::start(fake.clone(), LONG);
    updater.set_keep_in_tray(true);
    let mut timer = Schedule::new(Duration::ZERO, Duration::ZERO);
    updater.tick(&mut timer, Duration::ZERO);
    wait_for(&updater, |view| matches!(view, UpdateView::Ready { .. }));
    let _ = updater.restart_with(|| Err(UpdateFailure::Apply));
    assert_eq!(view_of(&updater), UpdateView::Failed(UpdateFailure::Apply));
    updater.tick(&mut timer, DAY - Duration::from_secs(1));
    assert_eq!(view_of(&updater), UpdateView::Failed(UpdateFailure::Apply));
    fake.answer_check(Ok(Found::Nothing));
    updater.tick(&mut timer, DAY);
    assert!(!matches!(view_of(&updater), UpdateView::Failed(_)));
}

fn temp_settings() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    Arc<Mutex<SettingsStore>>,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("navo.toml");
    let (store, _) = SettingsStore::load(&path);
    (dir, path, Arc::new(Mutex::new(store)))
}

fn rel(text: &str) -> Release {
    Release {
        version: version(text),
        size: 1024,
    }
}

fn wait_targets(updater: &Arc<Updater>) -> Vec<navo_ui::window_view::RollbackTargetView> {
    let hooks = updater.hooks();
    poll_until(400, || {
        let targets = (hooks.rollback_targets)();
        (!targets.is_empty()).then_some(targets)
    })
    .unwrap_or_default()
}

fn wait_rollback_calls(fake: &Arc<FakeFeed>, expected: usize) -> Vec<Version> {
    poll_until(400, || {
        let calls = fake.rollback_calls();
        (calls.len() >= expected).then_some(calls)
    })
    .unwrap_or_else(|| fake.rollback_calls())
}

#[test]
fn rollback_targets_maps_compatibility_without_blocking() {
    use navo_ui::window_view::{RollbackCompatibilityView, RollbackTargetView};

    let fake = Arc::new(
        FakeFeed::new(InstallKind::Installed, version("0.9.4")).with_recent_releases(vec![
            rel("0.9.4"),
            rel("0.9.3"),
            rel("0.9.2"),
            rel("0.9.1"),
            rel("0.9.0"),
        ]),
    );
    let compat = Arc::new(
        |target: &RollbackTarget| match target.version.to_string().as_str() {
            "0.9.3" => RollbackCompatibility::Compatible,
            "0.9.2" => RollbackCompatibility::CompatibleWithBackup {
                backup_path: std::path::PathBuf::from("session.bin.bak"),
            },
            _ => RollbackCompatibility::Incompatible {
                reason: "schema mismatch".to_owned(),
            },
        },
    );
    let updater = Updater::start_with_rollback(
        fake,
        LONG,
        RollbackConfig {
            settings: None,
            compat,
        },
    );
    let targets = wait_targets(&updater);
    assert_eq!(
        targets,
        vec![
            RollbackTargetView {
                version: "0.9.3".into(),
                status: RollbackCompatibilityView::Compatible,
            },
            RollbackTargetView {
                version: "0.9.2".into(),
                status: RollbackCompatibilityView::CompatibleWithBackup,
            },
            RollbackTargetView {
                version: "0.9.1".into(),
                status: RollbackCompatibilityView::Incompatible {
                    reason: "schema mismatch".into(),
                },
            },
        ]
    );
}

#[test]
fn apply_rollback_persists_skipped_version_and_prevents_auto_loop() {
    let (_dir, path, store) = temp_settings();
    let fake = Arc::new(
        FakeFeed::new(InstallKind::Installed, version("0.9.4"))
            .with_recent_releases(vec![rel("0.9.3")]),
    );
    let updater = Updater::start_with_rollback(
        fake.clone(),
        LONG,
        RollbackConfig {
            settings: Some(store),
            compat: Arc::new(|_| RollbackCompatibility::Compatible),
        },
    );
    assert!(!wait_targets(&updater).is_empty());
    let hooks = updater.hooks();
    assert_eq!((hooks.apply_rollback)("0.9.3"), Ok(()));
    assert_eq!((hooks.skipped_version)().as_deref(), Some("0.9.4"));
    let (reloaded, _) = SettingsStore::load(&path);
    assert_eq!(
        reloaded.current().updates.skipped_version.as_deref(),
        Some("0.9.4")
    );
    assert_eq!(wait_rollback_calls(&fake, 1), vec![version("0.9.3")]);

    // After restart on 0.9.3, automatic check skips 0.9.4, while manual check_now offers it.
    let after_store = Arc::new(Mutex::new(reloaded));
    let after_fake = Arc::new(FakeFeed::new(InstallKind::Installed, version("0.9.3")));
    after_fake.answer_check(Ok(release("0.9.4")));
    let after_updater = Updater::start_with_rollback(
        after_fake.clone(),
        Duration::from_millis(10),
        RollbackConfig {
            settings: Some(after_store),
            compat: Arc::new(|_| RollbackCompatibility::Compatible),
        },
    );
    wait_for(&after_updater, |view| *view == UpdateView::UpToDate);
    assert_eq!(after_fake.download_calls(), 0);

    after_fake.answer_check(Ok(release("0.9.4")));
    let steps = after_fake.hold_download();
    (after_updater.hooks().check_now)();
    wait_for(&after_updater, |view| {
        matches!(view, UpdateView::Downloading { .. })
    });
    steps.send(DownloadStep::Done(Ok(()))).expect("done");
    wait_for(&after_updater, |view| {
        *view
            == UpdateView::Ready {
                version: "0.9.4".into(),
            }
    });
}

#[test]
fn clear_skipped_version_clears_store_and_reenables_auto_check() {
    let (_dir, path, store) = temp_settings();
    store
        .lock()
        .expect("store")
        .save_preserving_edits(
            "updates.skipped-version",
            Some(navo_store::SettingValue::Text("0.9.4".to_owned())),
        )
        .expect("seed skip");
    let fake = Arc::new(FakeFeed::new(InstallKind::Installed, version("0.9.3")));
    let updater = Updater::start_with_rollback(
        fake.clone(),
        LONG,
        RollbackConfig {
            settings: Some(store),
            compat: Arc::new(|_| RollbackCompatibility::Compatible),
        },
    );
    updater.set_keep_in_tray(true);
    let hooks = updater.hooks();
    assert_eq!((hooks.skipped_version)().as_deref(), Some("0.9.4"));
    (hooks.clear_skipped_version)();
    assert_eq!((hooks.skipped_version)(), None);
    let (reloaded, _) = SettingsStore::load(&path);
    assert_eq!(reloaded.current().updates.skipped_version, None);

    fake.answer_check(Ok(release("0.9.4")));
    let steps = fake.hold_download();
    let mut timer = Schedule::new(Duration::ZERO, Duration::ZERO);
    updater.tick(&mut timer, Duration::ZERO);
    wait_for(&updater, |view| {
        matches!(view, UpdateView::Downloading { .. })
    });
    steps.send(DownloadStep::Done(Ok(()))).expect("done");
    wait_for(&updater, |view| matches!(view, UpdateView::Ready { .. }));
}

#[test]
fn incompatible_rollback_target_is_refused_without_skip_or_downgrade() {
    let (_dir, path, store) = temp_settings();
    let fake = Arc::new(
        FakeFeed::new(InstallKind::Installed, version("0.9.4"))
            .with_recent_releases(vec![rel("0.9.1")]),
    );
    let updater = Updater::start_with_rollback(
        fake.clone(),
        LONG,
        RollbackConfig {
            settings: Some(store),
            compat: Arc::new(|_| RollbackCompatibility::Incompatible {
                reason: "schema too new".to_owned(),
            }),
        },
    );
    assert!(!wait_targets(&updater).is_empty());
    let hooks = updater.hooks();
    let err = (hooks.apply_rollback)("0.9.1").expect_err("must refuse incompatible target");
    assert!(err.contains("schema too new"), "unexpected error: {err}");
    assert_eq!((hooks.skipped_version)(), None);
    let (reloaded, _) = SettingsStore::load(&path);
    assert_eq!(reloaded.current().updates.skipped_version, None);
    assert!(fake.rollback_calls().is_empty());
}

#[test]
fn panicking_rollback_resets_in_flight_flag() {
    use std::sync::atomic::AtomicUsize;

    struct PanickingRollbackFeed {
        calls: AtomicUsize,
    }

    impl UpdateFeed for PanickingRollbackFeed {
        fn kind(&self) -> InstallKind {
            InstallKind::Installed
        }
        fn current(&self) -> Version {
            version("0.9.4")
        }
        fn staged(&self) -> Option<Version> {
            None
        }
        fn check(&self) -> Result<Found, FeedError> {
            Ok(Found::Nothing)
        }
        fn download(&self, _: &Release, _: &dyn Fn(u8)) -> Result<(), FeedError> {
            Ok(())
        }
        fn recent_releases(&self) -> Result<Vec<Release>, FeedError> {
            Ok(vec![rel("0.9.3")])
        }
        fn rollback(&self, _: &Release) -> Result<(), FeedError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                panic!("simulated rollback panic");
            }
            Ok(())
        }
    }

    let feed = Arc::new(PanickingRollbackFeed {
        calls: AtomicUsize::new(0),
    });
    let updater = Updater::start_with_rollback(feed.clone(), LONG, RollbackConfig::default());
    assert!(!wait_targets(&updater).is_empty());
    let hooks = updater.hooks();

    assert_eq!((hooks.apply_rollback)("0.9.3"), Ok(()));
    poll_until(400, || {
        (!updater.rollback_in_flight.load(Ordering::SeqCst)
            && feed.calls.load(Ordering::SeqCst) >= 1)
            .then_some(())
    });
    assert!(!updater.rollback_in_flight.load(Ordering::SeqCst));
    assert_eq!((hooks.apply_rollback)("0.9.3"), Ok(()));
    poll_until(400, || {
        (feed.calls.load(Ordering::SeqCst) >= 2).then_some(())
    });
    assert_eq!(feed.calls.load(Ordering::SeqCst), 2);
}
