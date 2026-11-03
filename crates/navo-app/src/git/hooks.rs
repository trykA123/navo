use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use navo_core::Location;
use navo_store::GitSettings;
use navo_ui::git::{FolderBadges, GitAction, GitHooks, GitMenu, GitStarted, Previewed, ToolRow};

use super::fetch::{self, FetchTimer};
use super::launch;
use super::launch::start::Ports as LaunchPorts;
use super::previews::Previews;
use super::run::{Job, Rescan};
use super::scan::{DbSaved, GitScan, Ports, Row, SavedBadges, SystemProbe};
use super::source::{CliBackend, GitSource, GixBackend};
use super::wiring::Tools;

type Factory = Box<dyn Fn(&GitSettings) -> Ports + Send + Sync>;
type Clock = Box<dyn Fn() -> Instant + Send + Sync>;
type Timers = Box<dyn Fn(&GitSettings, &GitScan) -> Option<FetchTimer> + Send + Sync>;
type Callback = Arc<dyn Fn() + Send + Sync>;

pub(super) struct Shared {
    subscribers: Mutex<Vec<Callback>>,
    pub(super) stamp: Mutex<Instant>,
    pub(super) clock: Clock,
    /// The highest revision any scanner published; the next scanner numbers above it.
    revision: AtomicU64,
}

impl Shared {
    pub(super) fn touch(&self) {
        *lock(&self.stamp) = (self.clock)();
    }

    fn published(&self, revision: u64) {
        self.revision.fetch_max(revision, Ordering::SeqCst);
        self.touch();
        self.notify_all();
    }

    pub(super) fn notify_all(&self) {
        let subscribers = lock(&self.subscribers).clone();
        for notify in subscribers {
            notify();
        }
    }
}

/// A started scanner, and the fetch timer it replaced. The caller drops `retired` last, with
/// no lock held: a timer's stop can wait for a running fetch.
pub(super) struct Started {
    pub scan: Option<GitScan>,
    pub retired: Option<FetchTimer>,
}

pub(super) struct State {
    pub(super) settings: GitSettings,
    pub(super) scan: Option<GitScan>,
    pub(super) seen: HashMap<Location, (u64, Vec<String>)>,
}

/// Git badges over the scanner, and the menu and actions of the user's Git tool.
pub(crate) struct Hooks {
    factory: Factory,
    pub(super) shared: Arc<Shared>,
    pub(super) state: Mutex<State>,
    fetch: Mutex<Option<FetchTimer>>,
    timers: Timers,
    tools: Arc<Tools>,
    previews: Previews,
}

impl Hooks {
    /// `badge_db` is the profile's cache database, where the last badges are kept.
    pub(crate) fn system(settings: &GitSettings, badge_db: Option<PathBuf>) -> Arc<Self> {
        let saved = badge_db.map(|path| Arc::new(DbSaved::at(path)));
        let factory: Factory = Box::new(move |settings| Ports {
            source: Arc::new(GitSource::new(CliBackend, GixBackend, settings.engine)),
            probe: Arc::new(SystemProbe),
            saved: saved.clone().map(|saved| saved as Arc<dyn SavedBadges>),
        });
        let hooks = Arc::new(Self::new(
            settings,
            factory,
            Box::new(Instant::now),
            Tools::system(),
        ));
        let warm = hooks.clone();
        let started = std::thread::Builder::new()
            .name("navo-git-tools".to_owned())
            .spawn(move || drop(warm.tools.detected()));
        if let Err(error) = started {
            tracing::warn!(%error, "the Git tool detection thread did not start");
        }
        hooks
    }

    /// Where Git tools open a terminal or a URL; set once the terminal settings are loaded.
    pub(crate) fn set_ports(&self, ports: LaunchPorts) {
        *lock(&self.tools.ports) = ports;
    }

    fn new(settings: &GitSettings, factory: Factory, clock: Clock, tools: Tools) -> Self {
        let shared = Arc::new(Shared {
            subscribers: Mutex::default(),
            stamp: Mutex::new(clock()),
            clock,
            revision: AtomicU64::new(0),
        });
        let tools = Arc::new(tools);
        let hooks = Self {
            factory,
            shared,
            state: Mutex::new(State {
                settings: settings.clone(),
                scan: None,
                seen: HashMap::new(),
            }),
            fetch: Mutex::new(None),
            timers: Box::new(system_timer),
            previews: Previews::start(tools.clone()),
            tools,
        };
        lock(&hooks.state).scan = hooks.start(settings).scan;
        hooks
    }

    pub(super) fn start(&self, settings: &GitSettings) -> Started {
        let scan = self.start_scan(settings);
        let retired = self.swap_timer(scan.as_ref(), settings);
        Started { scan, retired }
    }

    /// Installs a timer for `scan` and returns the one it replaced.
    pub(super) fn swap_timer(
        &self,
        scan: Option<&GitScan>,
        settings: &GitSettings,
    ) -> Option<FetchTimer> {
        let timer = scan.and_then(|scan| (self.timers)(settings, scan));
        std::mem::replace(&mut *lock(&self.fetch), timer)
    }

    fn start_scan(&self, settings: &GitSettings) -> Option<GitScan> {
        if !settings.badges {
            return None;
        }
        let shared = self.shared.clone();
        let notify = Box::new(move |revision| shared.published(revision));
        match GitScan::start((self.factory)(settings), settings, notify) {
            Ok(scan) => {
                scan.resume_after(self.shared.revision.load(Ordering::SeqCst));
                Some(scan)
            }
            Err(error) => {
                tracing::warn!(%error, "the Git scanner did not start");
                None
            }
        }
    }

    /// Cancels every running Pull and Push; called once at quit.
    pub(crate) fn shutdown(&self) {
        self.tools.cancel_runs();
    }

    #[cfg(test)]
    fn running(&self) -> bool {
        lock(&self.state).scan.is_some()
    }

    #[cfg(test)]
    fn fetching(&self) -> bool {
        lock(&self.fetch).is_some()
    }
}

impl Hooks {
    fn settings(&self) -> GitSettings {
        lock(&self.state).settings.clone()
    }

    /// Rescans the listing that holds `repo`'s badge; does nothing once the scanner is replaced.
    fn rescan_of(&self, repo: &Location) -> Rescan {
        let view = lock(&self.state).scan.as_ref().map(GitScan::view);
        let listing = repo.parent();
        Arc::new(move || {
            if let (Some(view), Some(listing)) = (&view, &listing) {
                view.rescan(listing);
            }
        })
    }
}

fn system_timer(settings: &GitSettings, scan: &GitScan) -> Option<FetchTimer> {
    FetchTimer::start(settings, fetch::Ports::system(scan.view(), scan.slots()))
}

pub(super) fn rows(names: &[String]) -> Vec<Row> {
    names
        .iter()
        .map(|name| Row {
            name: name.clone(),
            on_screen: false,
        })
        .collect()
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl GitHooks for Hooks {
    fn observe(&self, listing: &Location, revision: u64, names: Vec<String>) {
        let mut state = lock(&self.state);
        if let Some(scan) = &state.scan {
            scan.observe(listing, revision, rows(&names));
        }
        state.seen.insert(listing.clone(), (revision, names));
    }

    fn forget(&self, listing: &Location) {
        let mut state = lock(&self.state);
        state.seen.remove(listing);
        if let Some(scan) = &state.scan {
            scan.forget(listing);
        }
    }

    fn badges(&self, listing: &Location) -> Option<Arc<FolderBadges>> {
        lock(&self.state).scan.as_ref()?.badges(listing)
    }

    fn subscribe(&self, notify: Box<dyn Fn() + Send + Sync>) {
        lock(&self.shared.subscribers).push(Arc::from(notify));
    }

    fn rescan(&self, listing: &Location) {
        if let Some(scan) = &lock(&self.state).scan {
            scan.rescan(listing);
        }
    }

    fn focus_regained(&self) {
        Hooks::focus_regained(self);
    }

    fn menu(&self, repo: &Location) -> GitMenu {
        self.tools.menu(repo, &self.settings())
    }

    fn previews(&self, repo: &Location, done: Box<dyn FnOnce(Previewed) + Send>) {
        self.previews.request(repo, self.settings(), done);
    }

    fn run(&self, repo: &Location, action: GitAction) -> Result<GitStarted, String> {
        self.run_confirmed(repo, action, None, None)
    }

    fn run_confirmed(
        &self,
        repo: &Location,
        action: GitAction,
        shown: Option<&str>,
        confirm_id: Option<&str>,
    ) -> Result<GitStarted, String> {
        let settings = self.settings();
        let job = Job {
            repo,
            action,
            shown,
            confirm_id,
            settings: &settings,
            rescan: self.rescan_of(repo),
        };
        self.tools.run(&job)
    }

    fn tools(&self) -> Vec<ToolRow> {
        self.tools.rows(&self.settings())
    }

    fn check_command(&self, template: &str) -> Result<(), String> {
        launch::check_template(template).map_err(|error| error.to_string())
    }

    fn refresh_tools(&self) {
        let (tools, shared) = (self.tools.clone(), self.shared.clone());
        let started = std::thread::Builder::new()
            .name("navo-git-tools".to_owned())
            .spawn(move || {
                tools.redetect();
                shared.notify_all();
            });
        if let Err(error) = started {
            tracing::warn!(%error, "the Git tool detection thread did not start");
        }
    }

    fn can_diff(&self, _repo: &Location) -> bool {
        self.tools.can_diff(&self.settings())
    }

    fn open_diff(&self, _repo: &Location, _path: &Location) -> Result<GitStarted, String> {
        Err(navo_l10n::tr("git-off"))
    }
}

#[cfg(test)]
mod tests;
