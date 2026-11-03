//! The clipboard module: history store, service and watcher behind `ClipboardHooks`.

mod capture;
mod edit;
mod follow;
mod hooks;
mod intro;
mod jobs;
mod lifecycle;
mod listeners;
mod notes;
mod rows;
mod running;
mod saved;
mod sealer;
mod sink;

#[cfg(test)]
mod privacy_tests;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use navo_clipboard::{ClipService, Ports, Snapshot, SystemClock};
use navo_core::Location;
use navo_shell::{ClipboardError, ShellCapture};
use navo_store::StoreDirs;
use navo_ui::settings::Values;

use intro::Intro;
use jobs::Jobs;
use listeners::Listeners;
use running::Running;

pub(crate) use follow::follow;

pub(super) type OnCapture = Box<dyn Fn(ShellCapture) + Send>;
/// Starts a clipboard watcher; dropping the returned guard stops it.
pub(super) type StartWatch =
    Arc<dyn Fn(OnCapture) -> Result<Box<dyn Send>, ClipboardError> + Send + Sync>;

const DB_NAME: &str = "clipboard.db";

/// What the module talks to: the OS watcher and the service ports.
pub(super) struct Backends {
    pub(super) watch: StartWatch,
    pub(super) ports: Ports,
    /// Whether the sealer can seal; false shows "Secrets need Windows".
    pub(super) secrets: bool,
}

impl Backends {
    fn shell() -> Self {
        Self {
            watch: Arc::new(|on_change| {
                let watcher = navo_shell::Watcher::start(on_change)?;
                Ok(Box::new(watcher) as Box<dyn Send>)
            }),
            ports: Ports {
                sink: Arc::new(sink::ShellSink),
                sealer: Arc::new(sealer::ShellSealer),
                clock: Arc::new(SystemClock),
            },
            secrets: cfg!(windows),
        }
    }
}

/// The settings the module follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Config {
    pub(super) enabled: bool,
    pub(super) record: bool,
}

impl Config {
    fn from_values(values: &Values) -> Self {
        Self {
            enabled: values.clipboard_on(),
            record: values.records_clipboard(),
        }
    }
}

enum Active {
    Off { notice: Option<String> },
    Starting,
    On(Running),
}

struct State {
    config: Option<Config>,
    active: Active,
    /// Bumped on every change of `active`; a start job installs its result only if it still
    /// matches.
    generation: u64,
    /// Added to the service's revision, so revisions never go back across a stop and start.
    epoch: u64,
    /// Set by `stop`; later settings changes are ignored.
    stopped: bool,
}

/// What the hooks see, read under one lock.
struct View {
    live: Option<(Arc<ClipService>, Arc<Snapshot>)>,
    notice: Option<String>,
    revision: u64,
}

/// Records the clipboard while the setting is on. It stops and starts live, off the window
/// thread, and keeps its files when it stops. Listeners stay for the module's lifetime.
pub(crate) struct ClipModule {
    db: Option<PathBuf>,
    backends: Backends,
    state: Mutex<State>,
    listeners: Arc<Listeners>,
    jobs: Jobs,
    intro: Intro,
}

impl ClipModule {
    /// Builds the module and applies the settings. Without profile folders it stays off.
    pub(crate) fn start(dirs: Option<&StoreDirs>, values: &Values) -> Arc<Self> {
        let module = Arc::new(Self::new(
            dirs.map(|dirs| dirs.data.clone()),
            Backends::shell(),
        ));
        module.apply(values);
        module
    }

    fn new(data: Option<PathBuf>, backends: Backends) -> Self {
        Self {
            intro: Intro::new(data.as_deref()),
            db: data.map(|data| data.join(DB_NAME)),
            backends,
            state: Mutex::new(State {
                config: None,
                active: Active::Off { notice: None },
                generation: 0,
                epoch: 0,
                stopped: false,
            }),
            listeners: Arc::new(Listeners::default()),
            jobs: Jobs::default(),
        }
    }

    /// Called from one thread only (the window thread); it never blocks on the store.
    pub(crate) fn apply(self: &Arc<Self>, values: &Values) {
        self.apply_config(Config::from_values(values));
    }

    fn peek(&self) -> View {
        let state = lock(&self.state);
        match &state.active {
            Active::On(running) => {
                let snapshot = running.service().snapshot();
                View {
                    revision: state.epoch + 1 + snapshot.revision,
                    live: Some((running.service().clone(), snapshot)),
                    notice: running.notice(),
                }
            }
            Active::Starting => View {
                live: None,
                notice: Some(navo_l10n::tr("clipboard-busy")),
                revision: state.epoch,
            },
            Active::Off { notice } => View {
                live: None,
                notice: notice
                    .clone()
                    .or_else(|| Some(navo_l10n::tr("clipboard-off"))),
                revision: state.epoch,
            },
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn folder_path(folder: Option<&Location>) -> Option<PathBuf> {
    let path = navo_fs::os_path(folder?).ok()?;
    Some(PathBuf::from(crate::shell_menu::without_verbatim(
        path.into_os_string(),
    )))
}
