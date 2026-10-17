use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use rusty_store::{Db, DefaultChange, LoadReport, Settings, SettingsStore, StoreDirs};

use super::{describe, report};

const LEDGER: &str = "ferrite.db";

#[derive(Debug)]
pub struct Loaded {
    pub path: PathBuf,
    pub store: SettingsStore,
    pub report: LoadReport,
}

impl Loaded {
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let (store, report) = SettingsStore::load(path);
        Self {
            path: path.to_owned(),
            store,
            report,
        }
    }
}

#[derive(Debug, Default)]
pub struct Started {
    pub loaded: Option<Loaded>,
    pub default_changes: Vec<DefaultChange>,
}

pub struct Starting {
    handle: JoinHandle<Started>,
    file: Option<PathBuf>,
}

impl Starting {
    #[must_use]
    pub fn finish(self) -> Started {
        let started = self.handle.join().unwrap_or_else(|_| {
            tracing::warn!("settings start failed; loading the file as it is");
            Started {
                loaded: self.file.as_deref().map(Loaded::load),
                default_changes: Vec::new(),
            }
        });
        tracing::info!(
            followed = started.default_changes.len(),
            "settings defaults checked"
        );
        started
    }
}

#[must_use]
pub fn start(file: Option<&Path>, ledger: Option<&Path>) -> Started {
    let Some(file) = file else {
        return Started::default();
    };
    let mut loaded = Loaded::load(file);
    let default_changes = match ledger {
        Some(ledger) if loaded.report.error.is_none() => complete(&mut loaded.store, ledger),
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
    let worker_file = file.clone();
    let handle = std::thread::spawn(move || start(worker_file.as_deref(), ledger.as_deref()));
    Starting { handle, file }
}

fn complete(store: &mut SettingsStore, ledger: &Path) -> Vec<DefaultChange> {
    let db = match Db::open(ledger, false) {
        Ok((db, _)) => db,
        Err(error) => {
            report(&format!("{}: {error}", ledger.display()));
            return Vec::new();
        }
    };
    match store.complete(&db, &Settings::default()) {
        Ok(done) => done.changes,
        Err(error) => {
            report(&describe(&error));
            Vec::new()
        }
    }
}
