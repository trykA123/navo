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

fn wait_for(updater: &Arc<Updater>, wanted: impl Fn(&UpdateView) -> bool) -> UpdateView {
    for _ in 0..1000 {
        let view = view_of(updater);
        if wanted(&view) {
            return view;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the state never matched: {:?}", view_of(updater));
}

const LONG: Duration = Duration::from_secs(3600);

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
