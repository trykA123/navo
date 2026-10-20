use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use rusty_store::{DefaultChange, LoadReport, SessionBackup, Settings, SettingsStore, StoreDirs};

use super::{describe, report};

const LEDGER: &str = "ferrite.db";
const WAIT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub struct Loaded {
    pub path: PathBuf,
    pub store: SettingsStore,
    pub report: LoadReport,
    pub ledger: Option<PathBuf>,
    pub default_changes: Vec<DefaultChange>,
    pub(super) late: Option<Receiver<Started>>,
}

impl Loaded {
    #[cfg(test)]
    #[must_use]
    pub fn load(path: &Path) -> Self {
        Self::load_sharing(path, SessionBackup::default())
    }

    fn load_sharing(path: &Path, backup: SessionBackup) -> Self {
        let (store, report) = SettingsStore::load_sharing(path, backup);
        Self {
            path: path.to_owned(),
            store,
            report,
            ledger: None,
            default_changes: Vec::new(),
            late: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Started {
    pub loaded: Option<Loaded>,
    pub default_changes: Vec<DefaultChange>,
}

impl Started {
    #[must_use]
    pub fn into_loaded(self) -> Option<Loaded> {
        let mut loaded = self.loaded?;
        loaded.default_changes = self.default_changes;
        Some(loaded)
    }
}

pub struct Starting {
    receiver: Receiver<Started>,
    file: Option<PathBuf>,
    ledger: Option<PathBuf>,
    backup: SessionBackup,
    wait: Duration,
}

impl Starting {
    #[must_use]
    pub fn finish(self) -> Started {
        let started = match self.receiver.recv_timeout(self.wait) {
            Ok(started) => started,
            Err(RecvTimeoutError::Timeout) => {
                tracing::warn!("settings completion is slow; loading the file as it is");
                self.into_fallback(true)
            }
            Err(RecvTimeoutError::Disconnected) => {
                tracing::warn!("settings start failed; loading the file as it is");
                self.into_fallback(false)
            }
        };
        tracing::info!(
            followed = started.default_changes.len(),
            "settings defaults checked"
        );
        started
    }

    fn into_fallback(self, pending: bool) -> Started {
        let mut loaded = self
            .file
            .as_deref()
            .map(|file| Loaded::load_sharing(file, self.backup.clone()));
        if let Some(loaded) = &mut loaded {
            loaded.ledger = self.ledger;
            loaded.late = pending.then_some(self.receiver);
        }
        Started {
            loaded,
            default_changes: Vec::new(),
        }
    }
}

#[cfg(test)]
#[must_use]
pub fn start(file: Option<&Path>, ledger: Option<&Path>) -> Started {
    start_sharing(file, ledger, SessionBackup::default())
}

pub(super) fn start_sharing(
    file: Option<&Path>,
    ledger: Option<&Path>,
    backup: SessionBackup,
) -> Started {
    let Some(file) = file else {
        return Started::default();
    };
    let mut loaded = Loaded::load_sharing(file, backup);
    loaded.ledger = ledger.map(Path::to_owned);
    let default_changes = match ledger {
        Some(ledger) if loaded.report.error.is_none() => {
            follow_defaults(&mut loaded.store, ledger, &Settings::default())
        }
        _ => Vec::new(),
    };
    Started {
        loaded: Some(loaded),
        default_changes,
    }
}

#[must_use]
pub fn start_from(dirs: Option<&StoreDirs>) -> Starting {
    let ledger = dirs
        .filter(|dirs| dirs.ensure().is_ok())
        .map(|dirs| dirs.data.join(LEDGER));
    start_in_background(dirs.map(StoreDirs::settings_file), ledger)
}

#[must_use]
pub fn start_in_background(file: Option<PathBuf>, ledger: Option<PathBuf>) -> Starting {
    start_waiting(file, ledger, WAIT)
}

pub(super) fn start_waiting(
    file: Option<PathBuf>,
    ledger: Option<PathBuf>,
    wait: Duration,
) -> Starting {
    let worker_file = file.clone();
    let worker_ledger = ledger.clone();
    let mut starting = spawn(file, wait, move |backup| {
        start_sharing(worker_file.as_deref(), worker_ledger.as_deref(), backup)
    });
    starting.ledger = ledger;
    starting
}

pub(super) fn spawn(
    file: Option<PathBuf>,
    wait: Duration,
    work: impl FnOnce(SessionBackup) -> Started + Send + 'static,
) -> Starting {
    let backup = SessionBackup::default();
    let shared = backup.clone();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let started = work(shared);
        let followed = started.default_changes.len();
        if sender.send(started).is_err() {
            tracing::info!(followed, "settings completion finished after the start");
        }
    });
    Starting {
        receiver,
        file,
        ledger: None,
        backup,
        wait,
    }
}

pub(super) fn follow_defaults(
    store: &mut SettingsStore,
    ledger: &Path,
    defaults: &Settings,
) -> Vec<DefaultChange> {
    let db = match crate::crash::open_db(ledger) {
        Ok((db, _)) => db,
        Err(error) => {
            report(&format!("{}: {error}", ledger.display()));
            return Vec::new();
        }
    };
    match store.complete(&db, defaults) {
        Ok(done) => {
            if let Some(error) = &done.durability_error {
                report(&describe(error));
            }
            if let Some(error) = &done.ledger_error {
                report(&describe(error));
            }
            done.changes
        }
        Err(error) => {
            report(&describe(&error));
            Vec::new()
        }
    }
}
