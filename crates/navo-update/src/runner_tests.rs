use super::*;
use crate::fake::{DownloadStep, FakeFeed};
use crate::{FeedError, Found, InstallKind, Release};

const SHORT: Duration = Duration::from_millis(60);
const WAIT: Duration = Duration::from_secs(5);

fn v(major: u64) -> Version {
    Version::new(major, 0, 0)
}

fn newer() -> Found {
    Found::Release(Release {
        version: v(2),
        size: 10,
    })
}

fn feed(kind: InstallKind) -> Arc<FakeFeed> {
    Arc::new(FakeFeed::new(kind, v(1)))
}

fn wait_for(runner: &UpdateRunner, what: &str, want: impl Fn(&UpdateState) -> bool) {
    let end = Instant::now() + WAIT;
    while !want(&runner.snapshot()) {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn is_failed(state: &UpdateState, want: &FeedError) -> bool {
    matches!(state, UpdateState::Failed { error, .. } if error == want)
}

fn downloading(feed: &Arc<FakeFeed>) -> (UpdateRunner, mpsc::Sender<DownloadStep>) {
    feed.answer_check(Ok(newer()));
    let steps = feed.hold_download();
    let runner = UpdateRunner::start_with_timeout(feed.clone(), true, WAIT);
    wait_for(&runner, "downloading", |s| {
        matches!(s, UpdateState::Downloading { .. })
    });
    (runner, steps)
}

#[test]
fn hanging_check_times_out_as_offline() {
    let feed = feed(InstallKind::Installed);
    let _held = feed.hold_check();
    let runner = UpdateRunner::start_with_timeout(feed.clone(), true, SHORT);
    wait_for(&runner, "offline", |s| is_failed(s, &FeedError::Offline));
}

#[test]
fn late_answer_after_timeout_is_ignored() {
    let feed = feed(InstallKind::Installed);
    let held = feed.hold_check();
    let runner = UpdateRunner::start_with_timeout(feed.clone(), true, SHORT);
    wait_for(&runner, "offline", |s| is_failed(s, &FeedError::Offline));
    held.send(Ok(newer())).expect("the check is still waiting");
    thread::sleep(Duration::from_millis(150));
    assert!(is_failed(&runner.snapshot(), &FeedError::Offline));
    assert_eq!(feed.download_calls(), 0);
}

#[test]
fn late_answer_does_not_answer_the_next_check() {
    let feed = feed(InstallKind::Installed);
    let late = feed.hold_check();
    let _second = feed.hold_check();
    let runner = UpdateRunner::start_with_timeout(feed.clone(), true, Duration::from_millis(400));
    wait_for(&runner, "offline", |s| is_failed(s, &FeedError::Offline));
    runner.check_now();
    wait_for(&runner, "second check", |s| *s == UpdateState::Checking);
    late.send(Ok(newer()))
        .expect("the first call is still waiting");
    thread::sleep(Duration::from_millis(100));
    assert_eq!(runner.snapshot(), UpdateState::Checking);
    assert_eq!(feed.download_calls(), 0);
}

#[test]
fn check_now_after_timeout_runs_a_new_check() {
    let feed = feed(InstallKind::Installed);
    let _held = feed.hold_check();
    feed.answer_check(Ok(Found::Nothing));
    let runner = UpdateRunner::start_with_timeout(feed.clone(), true, SHORT);
    wait_for(&runner, "offline", |s| is_failed(s, &FeedError::Offline));
    runner.check_now();
    wait_for(&runner, "up to date", |s| *s == UpdateState::UpToDate);
    assert_eq!(feed.check_calls(), 2);
}

#[test]
fn check_now_during_download_is_ignored() {
    let feed = feed(InstallKind::Installed);
    let (runner, steps) = downloading(&feed);
    runner.check_now();
    steps
        .send(DownloadStep::Progress(40))
        .expect("download waits");
    wait_for(&runner, "40 percent", |s| {
        matches!(s, UpdateState::Downloading { percent: 40, .. })
    });
    assert_eq!(feed.check_calls(), 1);
}

#[test]
fn progress_reaches_the_snapshot_and_download_finishes_ready() {
    let feed = feed(InstallKind::Installed);
    let (runner, steps) = downloading(&feed);
    steps
        .send(DownloadStep::Progress(70))
        .expect("download waits");
    wait_for(&runner, "70 percent", |s| {
        matches!(s, UpdateState::Downloading { percent: 70, .. })
    });
    steps
        .send(DownloadStep::Done(Ok(())))
        .expect("download waits");
    wait_for(&runner, "ready", |s| {
        *s == UpdateState::Ready { version: v(2) }
    });
}

#[test]
fn failed_download_is_failed_not_ready() {
    let feed = feed(InstallKind::Installed);
    let (runner, steps) = downloading(&feed);
    steps
        .send(DownloadStep::Done(Err(FeedError::Interrupted)))
        .expect("download waits");
    wait_for(&runner, "failed", |s| is_failed(s, &FeedError::Interrupted));
}

#[test]
fn panicking_check_fails_instead_of_hanging() {
    struct Panics(FakeFeed);
    impl UpdateFeed for Panics {
        fn kind(&self) -> InstallKind {
            self.0.kind()
        }
        fn current(&self) -> Version {
            self.0.current()
        }
        fn staged(&self) -> Option<Version> {
            None
        }
        fn check(&self) -> Result<Found, FeedError> {
            panic!("feed bug");
        }
        fn download(&self, _: &Release, _: &dyn Fn(u8)) -> Result<(), FeedError> {
            Ok(())
        }
    }
    let feed = Arc::new(Panics(FakeFeed::new(InstallKind::Installed, v(1))));
    let runner = UpdateRunner::start_with_timeout(feed, true, WAIT);
    wait_for(&runner, "failed", |s| {
        matches!(
            s,
            UpdateState::Failed {
                error: FeedError::Other(_),
                ..
            }
        )
    });
}

#[test]
fn staged_package_starts_ready_without_a_request() {
    let feed = Arc::new(FakeFeed::new(InstallKind::Installed, v(1)).with_staged(v(2)));
    let mut runner = UpdateRunner::start_with_timeout(feed.clone(), true, WAIT);
    assert!(runner.stop());
    assert_eq!(runner.snapshot(), UpdateState::Ready { version: v(2) });
    assert_eq!(feed.check_calls(), 0);
}

#[test]
fn stop_while_idle_takes_under_a_second() {
    let feed = feed(InstallKind::Installed);
    let mut runner = UpdateRunner::start_with_timeout(feed, false, WAIT);
    let began = Instant::now();
    assert!(runner.stop());
    assert!(began.elapsed() < Duration::from_secs(1));
}

#[test]
fn stop_does_not_wait_for_a_download() {
    let feed = feed(InstallKind::Installed);
    let (mut runner, _steps) = downloading(&feed);
    let began = Instant::now();
    assert!(runner.stop());
    assert!(began.elapsed() < Duration::from_secs(1));
}

#[test]
fn portable_emits_no_request() {
    let feed = feed(InstallKind::Portable);
    let mut runner = UpdateRunner::start_with_timeout(feed.clone(), true, WAIT);
    runner.check_now();
    runner.set_auto(true);
    assert!(runner.stop());
    assert_eq!(
        runner.snapshot(),
        UpdateState::Unsupported(InstallKind::Portable)
    );
    assert_eq!((feed.check_calls(), feed.download_calls()), (0, 0));
}

#[test]
fn auto_off_waits_for_check_now() {
    let feed = feed(InstallKind::Installed);
    feed.answer_check(Ok(Found::Nothing));
    let runner = UpdateRunner::start_with_timeout(feed.clone(), false, WAIT);
    wait_for(&runner, "off", |s| *s == UpdateState::Off);
    assert_eq!(feed.check_calls(), 0);
    runner.check_now();
    wait_for(&runner, "up to date", |s| *s == UpdateState::UpToDate);
}
