//! The update service the window reads: one runner behind the `UpdateHooks` of Settings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use navo_ui::window_view::{UpdateHooks, UpdateView};
use navo_update::{
    FeedError, Found, InstallKind, Release, UpdateFeed, UpdateRunner, UpdateState, Version,
};

use super::view::view_of;

/// How long after start-up the first automatic check waits (D22c).
pub(crate) const START_DELAY: Duration = Duration::from_secs(10);

/// Shared by every window. Only an installed copy updates; any other copy stays `Hidden`.
pub(crate) struct Updater {
    feed: Arc<dyn UpdateFeed>,
    kind: InstallKind,
    is_auto: AtomicBool,
    runner: Mutex<Option<UpdateRunner>>,
    announced: Mutex<Option<Version>>,
}

impl Updater {
    /// An installed copy starts its runner after `delay`, or at once on Check now.
    pub(crate) fn start(feed: Arc<dyn UpdateFeed>, delay: Duration) -> Arc<Self> {
        let kind = feed.kind();
        let updater = Arc::new(Self {
            feed: Arc::new(Logged(feed)),
            kind,
            is_auto: AtomicBool::new(true),
            runner: Mutex::new(None),
            announced: Mutex::new(None),
        });
        if kind == InstallKind::Installed {
            let later = Arc::downgrade(&updater);
            let spawned = std::thread::Builder::new()
                .name("navo-update-start".into())
                .spawn(move || start_later(&later, delay));
            if let Err(error) = spawned {
                tracing::warn!(%error, "update: the start-up check could not be scheduled");
            }
        }
        updater
    }

    /// The version this copy runs: the installed package's, else the build's.
    pub(crate) fn version_text(updater: Option<&Self>) -> String {
        updater.map_or_else(
            || env!("CARGO_PKG_VERSION").to_owned(),
            |updater| updater.feed.current().to_string(),
        )
    }

    pub(crate) fn hooks(self: &Arc<Self>) -> UpdateHooks {
        let (state, check, ready) = (self.clone(), self.clone(), self.clone());
        UpdateHooks {
            state: Arc::new(move || state.view()),
            check_now: Arc::new(move || check.check_now()),
            take_ready_notice: Arc::new(move || ready.take_ready().map(|v| v.to_string().into())),
        }
    }

    /// Follows `updates.check`: it only decides whether the start-up check runs.
    pub(crate) fn set_auto(&self, auto: bool) {
        self.is_auto.store(auto, Ordering::SeqCst);
        if let Some(runner) = lock(&self.runner).as_ref() {
            runner.set_auto(auto);
        }
    }

    fn view(&self) -> UpdateView {
        if self.kind != InstallKind::Installed {
            return UpdateView::Hidden;
        }
        match lock(&self.runner).as_ref() {
            Some(runner) => view_of(&runner.snapshot(), Instant::now(), chrono::Local::now()),
            None if self.is_auto.load(Ordering::SeqCst) => UpdateView::Idle,
            None => UpdateView::Off,
        }
    }

    fn check_now(&self) {
        if self.kind == InstallKind::Installed {
            self.with_runner(UpdateRunner::check_now);
        }
    }

    /// The ready version, once per version.
    fn take_ready(&self) -> Option<Version> {
        let UpdateState::Ready { version } = lock(&self.runner).as_ref()?.snapshot() else {
            return None;
        };
        let mut announced = lock(&self.announced);
        if announced.as_ref() == Some(&version) {
            return None;
        }
        *announced = Some(version.clone());
        Some(version)
    }

    fn with_runner(&self, act: impl FnOnce(&UpdateRunner)) {
        let mut runner = lock(&self.runner);
        let is_auto = self.is_auto.load(Ordering::SeqCst);
        let runner = runner.get_or_insert_with(|| UpdateRunner::start(self.feed.clone(), is_auto));
        act(runner);
    }
}

fn start_later(updater: &Weak<Updater>, delay: Duration) {
    std::thread::sleep(delay);
    if let Some(updater) = updater.upgrade() {
        updater.with_runner(|_| {});
    }
}

/// Logs each failed feed call once, with its kind (D22h).
struct Logged(Arc<dyn UpdateFeed>);

impl Logged {
    fn note<T>(what: &str, result: Result<T, FeedError>) -> Result<T, FeedError> {
        if let Err(error) = &result {
            tracing::warn!(%error, "update: {what} failed");
        }
        result
    }
}

impl UpdateFeed for Logged {
    fn kind(&self) -> InstallKind {
        self.0.kind()
    }

    fn current(&self) -> Version {
        self.0.current()
    }

    fn staged(&self) -> Option<Version> {
        self.0.staged()
    }

    fn check(&self) -> Result<Found, FeedError> {
        Self::note("check", self.0.check())
    }

    fn download(&self, release: &Release, progress: &dyn Fn(u8)) -> Result<(), FeedError> {
        Self::note("download", self.0.download(release, progress))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
