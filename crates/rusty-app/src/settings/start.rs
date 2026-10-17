use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use rusty_store::{DefaultChange, LoadReport, SettingsStore};

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
        let _ = (self.handle, self.file);
        todo!()
    }
}

#[must_use]
pub fn start(file: Option<&Path>, ledger: Option<&Path>) -> Started {
    let _ = (file, ledger);
    todo!()
}

#[must_use]
pub fn start_in_background(file: Option<PathBuf>, ledger: Option<PathBuf>) -> Starting {
    let _ = (file, ledger);
    todo!()
}
