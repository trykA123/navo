//! The update service the window reads: one runner behind the `UpdateHooks` of Settings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use navo_ui::window_view::{UpdateFailure, UpdateHooks, UpdateView};
use navo_update::{
    FeedError, Found, InstallKind, Release, UpdateFeed, UpdateRunner, UpdateState, Version,
};

use super::schedule::{Schedule, Timer};
use super::view::view_of;

/// How long after start-up the first automatic check waits (D22c).
pub(crate) const START_DELAY: Duration = Duration::from_secs(10);

/// Shared by every window. Only an installed copy updates; any other copy stays `Hidden`.
pub(crate) struct Updater {
    feed: Arc<dyn UpdateFeed>,
    kind: InstallKind,
    is_auto: AtomicBool,
    keep_in_tray: AtomicBool,
    runner: Mutex<Option<UpdateRunner>>,
    announced: Mutex<Option<Version>>,
    restart_failure: Mutex<Option<UpdateFailure>>,
    timer: Mutex<Option<Timer>>,
}

impl Updater {
    /// An installed copy starts its runner after `delay`, or at once on Check now.
    pub(crate) fn start(feed: Arc<dyn UpdateFeed>, delay: Duration) -> Arc<Self> {
        let kind = feed.kind();
        let updater = Arc::new(Self {
            feed: Arc::new(Logged(feed)),
            kind,
            is_auto: AtomicBool::new(true),
            keep_in_tray: AtomicBool::new(false),
            runner: Mutex::new(None),
            announced: Mutex::new(None),
            restart_failure: Mutex::new(None),
            timer: Mutex::new(None),
        });
        if kind == InstallKind::Installed {
            match Timer::start(Arc::downgrade(&updater), delay) {
                Ok(timer) => *lock(&updater.timer) = Some(timer),
                Err(error) => {
                    tracing::warn!(%error, "update: automatic checks could not be scheduled")
                }
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
        let restart = self.clone();
        UpdateHooks {
            state: Arc::new(move || state.view()),
            check_now: Arc::new(move || check.check_now()),
            restart: Arc::new(move || restart.restart_with(super::apply::restart)),
            take_ready_notice: Arc::new(move || ready.take_ready().map(|v| v.to_string().into())),
        }
    }

    /// Follows `updates.check` for start-up and daily checks.
    pub(crate) fn set_auto(&self, auto: bool) {
        self.is_auto.store(auto, Ordering::SeqCst);
        if let Some(runner) = lock(&self.runner).as_ref() {
            runner.set_auto(auto);
        }
    }

    pub(crate) fn set_keep_in_tray(&self, keep: bool) {
        self.keep_in_tray.store(keep, Ordering::SeqCst);
    }

    pub(super) fn tick(&self, timer: &mut Schedule, now: Duration) {
        let is_start = timer.is_start;
        if !timer.take_due(now) || self.kind != InstallKind::Installed {
            return;
        }
        let mut runner = lock(&self.runner);
        let auto = self.is_auto.load(Ordering::SeqCst);
        if !is_start && (!auto || !self.keep_in_tray.load(Ordering::SeqCst)) {
            return;
        }
        match runner.as_ref() {
            None => *runner = Some(UpdateRunner::start(self.feed.clone(), auto)),
            Some(runner) if !is_start => {
                *lock(&self.restart_failure) = None;
                runner.check_now();
            }
            Some(_) => {}
        }
    }

    fn view(&self) -> UpdateView {
        if self.kind != InstallKind::Installed {
            return UpdateView::Hidden;
        }
        if let Some(failure) = lock(&self.restart_failure).clone() {
            return UpdateView::Failed(failure);
        }
        match lock(&self.runner).as_ref() {
            Some(runner) => view_of(&runner.snapshot(), Instant::now(), chrono::Local::now()),
            None if self.is_auto.load(Ordering::SeqCst) => UpdateView::Idle,
            None => UpdateView::Off,
        }
    }

    fn restart_with(
        &self,
        apply: impl FnOnce() -> Result<(), UpdateFailure>,
    ) -> Result<(), UpdateFailure> {
        if self.kind != InstallKind::Installed || !matches!(self.view(), UpdateView::Ready { .. }) {
            return Err(UpdateFailure::Busy);
        }
        let result = apply();
        if let Err(failure) = &result {
            *lock(&self.restart_failure) = Some(failure.clone());
        }
        result
    }

    fn check_now(&self) {
        if self.kind == InstallKind::Installed {
            *lock(&self.restart_failure) = None;
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
