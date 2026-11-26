//! The update service the window reads: one runner behind the `UpdateHooks` of Settings.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use navo_store::{
    Db, DowngradeCheck, SESSION_VERSION, SettingValue, SettingsStore, StoreDirs,
    check_downgrade_current,
};
use navo_ui::prelude::SharedString;
use navo_ui::window_view::{
    RollbackCompatibilityView, RollbackTargetView, UpdateFailure, UpdateHooks, UpdateView,
};
use navo_update::{
    FeedError, Found, InstallKind, Release, RollbackCompatibility, RollbackError, RollbackTarget,
    UpdateFeed, UpdateRunner, UpdateState, Version, clear_skipped_if_newer, prepare_rollback,
    recent_targets,
};

use super::schedule::{Schedule, Timer};
use super::view::view_of;

/// How long after start-up the first automatic check waits (D22c).
pub(crate) const START_DELAY: Duration = Duration::from_secs(10);
const SKIPPED_KEY: &str = "updates.skipped-version";

/// Storage and compatibility evaluator for rollback operations.
pub(crate) struct RollbackConfig {
    pub(crate) settings: Option<Arc<Mutex<SettingsStore>>>,
    pub(crate) compat: Arc<dyn Fn(&RollbackTarget) -> RollbackCompatibility + Send + Sync>,
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            settings: None,
            compat: Arc::new(|_| RollbackCompatibility::Compatible),
        }
    }
}

#[derive(Clone)]
struct CachedTarget {
    target: RollbackTarget,
    compat: RollbackCompatibility,
}

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
    rollback_settings: Option<Arc<Mutex<SettingsStore>>>,
    rollback_compat: Arc<dyn Fn(&RollbackTarget) -> RollbackCompatibility + Send + Sync>,
    skipped_version: Mutex<Option<String>>,
    rollback_cache: Mutex<Vec<CachedTarget>>,
    rollback_in_flight: AtomicBool,
}

impl Updater {
    /// An installed copy starts its runner after `delay`, or at once on Check now.
    #[cfg(test)]
    pub(crate) fn start(feed: Arc<dyn UpdateFeed>, delay: Duration) -> Arc<Self> {
        Self::start_with_rollback(feed, delay, RollbackConfig::default())
    }

    /// Starts the updater backed by `dirs` for persisted skip state and schema checks.
    pub(crate) fn start_with_dirs(
        feed: Arc<dyn UpdateFeed>,
        delay: Duration,
        dirs: Option<&StoreDirs>,
    ) -> Arc<Self> {
        let settings = dirs
            .map(StoreDirs::settings_file)
            .map(|path| Arc::new(Mutex::new(SettingsStore::load(&path).0)));
        let session_path = dirs.map(StoreDirs::session_file);
        let compat = Arc::new(move |_target: &RollbackTarget| {
            evaluate_compatibility(session_path.as_deref())
        });
        Self::start_with_rollback(feed, delay, RollbackConfig { settings, compat })
    }

    /// Starts the updater with rollback storage and compatibility checks.
    pub(crate) fn start_with_rollback(
        feed: Arc<dyn UpdateFeed>,
        delay: Duration,
        rollback: RollbackConfig,
    ) -> Arc<Self> {
        let kind = feed.kind();
        if kind != InstallKind::Installed {
            tracing::info!(?kind, "update: no automatic checks for this copy");
        }
        let current = feed.current();
        let skipped = initial_skipped(rollback.settings.as_ref(), &current);
        let updater = Arc::new(Self {
            feed: Arc::new(Logged(feed)),
            kind,
            is_auto: AtomicBool::new(true),
            keep_in_tray: AtomicBool::new(false),
            runner: Mutex::new(None),
            announced: Mutex::new(None),
            restart_failure: Mutex::new(None),
            timer: Mutex::new(None),
            rollback_settings: rollback.settings,
            rollback_compat: rollback.compat,
            skipped_version: Mutex::new(skipped),
            rollback_cache: Mutex::new(Vec::new()),
            rollback_in_flight: AtomicBool::new(false),
        });
        if kind == InstallKind::Installed {
            match Timer::start(Arc::downgrade(&updater), delay) {
                Ok(timer) => *lock(&updater.timer) = Some(timer),
                Err(error) => {
                    tracing::warn!(%error, "update: automatic checks could not be scheduled")
                }
            }
            updater.spawn_rollback_refresh();
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
        let (restart, skipped, clear) = (self.clone(), self.clone(), self.clone());
        let (targets, apply) = (self.clone(), self.clone());
        UpdateHooks {
            state: Arc::new(move || state.view()),
            check_now: Arc::new(move || check.check_now()),
            restart: Arc::new(move || restart.restart_with(super::apply::restart)),
            take_ready_notice: Arc::new(move || ready.take_ready().map(|v| v.to_string().into())),
            skipped_version: Arc::new(move || skipped.skipped_version()),
            clear_skipped_version: Arc::new(move || clear.clear_skipped_version()),
            rollback_targets: Arc::new(move || targets.rollback_targets()),
            apply_rollback: Arc::new(move |version| apply.apply_rollback(version)),
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

    pub(super) fn tick(self: &Arc<Self>, timer: &mut Schedule, now: Duration) {
        let is_start = timer.is_start;
        if !timer.take_due(now) || self.kind != InstallKind::Installed {
            return;
        }
        let mut runner = lock(&self.runner);
        let auto = self.is_auto.load(Ordering::SeqCst);
        if !is_start && (!auto || !self.keep_in_tray.load(Ordering::SeqCst)) {
            return;
        }
        let skipped = lock(&self.skipped_version).clone();
        match runner.as_ref() {
            None => {
                *runner = Some(UpdateRunner::start_with_skipped(
                    self.feed.clone(),
                    auto,
                    skipped,
                ))
            }
            Some(runner) if !is_start => {
                *lock(&self.restart_failure) = None;
                runner.set_skipped_version(skipped);
                runner.check_auto();
                self.spawn_rollback_refresh();
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

    fn check_now(self: &Arc<Self>) {
        if self.kind == InstallKind::Installed {
            *lock(&self.restart_failure) = None;
            self.with_runner(UpdateRunner::check_now);
            self.spawn_rollback_refresh();
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

    fn skipped_version(&self) -> Option<SharedString> {
        if self.kind != InstallKind::Installed {
            return None;
        }
        lock(&self.skipped_version).clone().map(Into::into)
    }

    fn clear_skipped_version(&self) {
        if self.kind != InstallKind::Installed {
            return;
        }
        persist_skipped(self.rollback_settings.as_ref(), None);
        *lock(&self.skipped_version) = None;
        if let Some(runner) = lock(&self.runner).as_ref() {
            runner.set_skipped_version(None);
        }
    }

    fn rollback_targets(&self) -> Vec<RollbackTargetView> {
        if self.kind != InstallKind::Installed {
            return Vec::new();
        }
        lock(&self.rollback_cache).iter().map(target_view).collect()
    }

    fn apply_rollback(self: &Arc<Self>, version: &str) -> Result<(), SharedString> {
        if self.kind != InstallKind::Installed {
            return Err(unavailable_message());
        }
        let cached = lock(&self.rollback_cache)
            .iter()
            .find(|entry| entry.target.version.to_string() == version)
            .cloned()
            .ok_or_else(unavailable_message)?;
        if let Err(error) = prepare_rollback(&cached.target, &cached.compat) {
            let message = match error {
                RollbackError::Incompatible(reason) => reason,
                RollbackError::Unsupported => navo_l10n::tr("settings-update-rollback-unavailable"),
            };
            return Err(message.into());
        }
        if self.rollback_in_flight.swap(true, Ordering::SeqCst) {
            return Err(unavailable_message());
        }
        let current = self.feed.current().to_string();
        persist_skipped(self.rollback_settings.as_ref(), Some(current.clone()));
        *lock(&self.skipped_version) = Some(current.clone());
        if let Some(runner) = lock(&self.runner).as_ref() {
            runner.set_skipped_version(Some(current));
        }
        let release = Release {
            version: cached.target.version,
            size: cached.target.size,
        };
        let this = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("navo-update-rollback".into())
            .spawn(move || this.run_rollback_worker(&release));
        if let Err(error) = spawned {
            self.rollback_in_flight.store(false, Ordering::SeqCst);
            tracing::warn!(%error, "update: could not start rollback worker");
            return Err(unavailable_message());
        }
        Ok(())
    }

    fn run_rollback_worker(&self, release: &Release) {
        let _guard = InFlightGuard(&self.rollback_in_flight);
        match catch_unwind(AssertUnwindSafe(|| self.feed.rollback(release))) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(%error, version = %release.version, "update: rollback failed");
            }
            Err(_) => {
                tracing::warn!(version = %release.version, "update: rollback panicked");
            }
        }
    }

    fn spawn_rollback_refresh(self: &Arc<Self>) {
        let this = Arc::downgrade(self);
        let spawned = std::thread::Builder::new()
            .name("navo-update-rollback-list".into())
            .spawn(move || {
                let Some(this) = this.upgrade() else {
                    return;
                };
                let Ok(releases) = this.feed.recent_releases() else {
                    return;
                };
                let current = this.feed.current();
                let targets = recent_targets(&releases, &current)
                    .into_iter()
                    .map(|target| {
                        let compat = (this.rollback_compat)(&target);
                        CachedTarget { target, compat }
                    })
                    .collect();
                *lock(&this.rollback_cache) = targets;
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "update: could not start rollback refresh");
        }
    }

    fn with_runner(&self, act: impl FnOnce(&UpdateRunner)) {
        let mut runner = lock(&self.runner);
        let is_auto = self.is_auto.load(Ordering::SeqCst);
        let skipped = lock(&self.skipped_version).clone();
        let runner = runner.get_or_insert_with(|| {
            UpdateRunner::start_with_skipped(self.feed.clone(), is_auto, skipped)
        });
        act(runner);
    }
}

struct InFlightGuard<'a>(&'a AtomicBool);

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn unavailable_message() -> SharedString {
    navo_l10n::tr("settings-update-rollback-unavailable").into()
}

fn target_view(entry: &CachedTarget) -> RollbackTargetView {
    let status = match &entry.compat {
        RollbackCompatibility::Compatible => RollbackCompatibilityView::Compatible,
        RollbackCompatibility::CompatibleWithBackup { .. } => {
            RollbackCompatibilityView::CompatibleWithBackup
        }
        RollbackCompatibility::Incompatible { reason } => RollbackCompatibilityView::Incompatible {
            reason: reason.clone().into(),
        },
    };
    RollbackTargetView {
        version: entry.target.version.to_string().into(),
        status,
    }
}

fn initial_skipped(
    settings: Option<&Arc<Mutex<SettingsStore>>>,
    current: &Version,
) -> Option<String> {
    let mut skipped =
        settings.and_then(|store| lock(store).current().updates.skipped_version.clone());
    if clear_skipped_if_newer(&mut skipped, &current.to_string()) {
        persist_skipped(settings, None);
    }
    skipped
}

fn persist_skipped(settings: Option<&Arc<Mutex<SettingsStore>>>, value: Option<String>) {
    let Some(store) = settings else {
        return;
    };
    let setting = value.map(SettingValue::Text);
    if let Err(error) = lock(store).save_preserving_edits(SKIPPED_KEY, setting) {
        tracing::warn!(%error, "update: failed to persist skipped version");
    }
}

fn evaluate_compatibility(session_path: Option<&Path>) -> RollbackCompatibility {
    if let Some(reason) = index_ipc_incompatible() {
        return RollbackCompatibility::Incompatible { reason };
    }
    let Some(session_path) = session_path else {
        return RollbackCompatibility::Compatible;
    };
    match check_downgrade_current(Db::VERSION, SESSION_VERSION, session_path) {
        DowngradeCheck::Compatible => RollbackCompatibility::Compatible,
        DowngradeCheck::CompatibleWithBackup { backup_path } => {
            RollbackCompatibility::CompatibleWithBackup { backup_path }
        }
        DowngradeCheck::Incompatible { reason } => RollbackCompatibility::Incompatible { reason },
    }
}

#[cfg(windows)]
fn index_ipc_incompatible() -> Option<String> {
    use navo_index::pipe::{SERVICE_REVISION, ServiceVersion};
    let query = navo_index_service::query().ok()?;
    if query.running != Some(true) {
        return None;
    }
    match query.version {
        Some(ServiceVersion::Known { revision, .. }) if revision > SERVICE_REVISION => {
            Some(format!(
                "index service protocol revision {revision} is newer than supported {SERVICE_REVISION}"
            ))
        }
        _ => None,
    }
}

#[cfg(not(windows))]
fn index_ipc_incompatible() -> Option<String> {
    None
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

    fn recent_releases(&self) -> Result<Vec<Release>, FeedError> {
        Self::note("recent_releases", self.0.recent_releases())
    }

    fn rollback(&self, target: &Release) -> Result<(), FeedError> {
        Self::note("rollback", self.0.rollback(target))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
