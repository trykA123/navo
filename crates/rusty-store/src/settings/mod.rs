use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::atomic_write::{WriteOutcome, write_checked};
use crate::{Db, DbError};

mod complete;
mod edit;
mod following;
mod model;
mod parse;
mod pending;
mod saving;
mod view;

pub use complete::DefaultChange;
use complete::{Ledger, reconcile};
pub use edit::{SettingValue, key_line, section_line};

pub use model::{
    Appearance, Density, Editor, Files, HiddenFiles, ModifierHints, SelectionSettings, Settings,
    SortKey, Terminal, ThemeMode,
};
pub use parse::{parse, to_toml};
pub use view::{
    Automotive, AutomotiveView, ImageView, ImageViewer, TextView, TextViewer, ViewPreset,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("invalid settings at line {line}, column {column}: {message}")]
    Invalid {
        line: usize,
        column: usize,
        message: String,
    },
    #[error("settings file error ({kind}): {message}")]
    Io {
        kind: io::ErrorKind,
        message: String,
    },
    #[error("the settings file on disk is invalid; fix it before saving")]
    FileInvalid,
    #[error("settings defaults ledger: {0}")]
    Ledger(String),
    #[error("completing the settings file would change a value; it was left as it is")]
    Refused,
    #[error("the file changed on disk while saving; nothing was written")]
    ChangedOnDisk,
    #[error("the setting changed in ferrite.toml while saving; the hand edit was kept")]
    FieldChangedOnDisk,
    #[error("the file is read-only; Ferrite does not change it")]
    ReadOnly,
    #[error("settings were saved, but durability could not be confirmed: {0}")]
    Durability(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadReport {
    pub unknown_keys: Vec<String>,
    pub error: Option<SettingsError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Completion {
    pub changes: Vec<DefaultChange>,
    pub written: bool,
    pub ledger_error: Option<SettingsError>,
    pub durability_error: Option<SettingsError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub was_present: bool,
    pub durability_error: Option<SettingsError>,
    pub ledger_error: Option<SettingsError>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionBackup(Arc<Mutex<bool>>);

#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    current: Settings,
    generation: u64,
    backup: SessionBackup,
    followed: Ledger,
}

impl SettingsStore {
    pub fn load(path: &Path) -> (Self, LoadReport) {
        Self::load_sharing(path, SessionBackup::default())
    }

    pub fn load_sharing(path: &Path, backup: SessionBackup) -> (Self, LoadReport) {
        let mut store = Self {
            path: path.to_owned(),
            current: Settings::default(),
            generation: 0,
            backup,
            followed: Ledger::new(),
        };
        let report = store.reload();
        (store, report)
    }

    #[must_use]
    pub fn current(&self) -> &Settings {
        &self.current
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn reload(&mut self) -> LoadReport {
        let text = match read(&self.path) {
            Ok(text) => text,
            Err(error) => return failed(error),
        };
        let Some(text) = text else {
            return LoadReport::default();
        };
        match parse(&text) {
            Ok((settings, unknown_keys)) => {
                self.take(settings);
                LoadReport {
                    unknown_keys,
                    error: None,
                }
            }
            Err(error) => failed(error),
        }
    }

    pub fn complete(
        &mut self,
        ledger: &Db,
        defaults: &Settings,
    ) -> Result<Completion, SettingsError> {
        let session = self.backup.clone();
        let mut backup = session.0.lock().unwrap_or_else(PoisonError::into_inner);
        if is_read_only(&self.path) {
            return Err(SettingsError::ReadOnly);
        }
        let recovery_error = pending::recover(&self.path, ledger)?;
        let text = read(&self.path)?;
        let recorded = ledger.settings_defaults().map_err(ledger_error)?;
        let done = reconcile(text.as_deref().unwrap_or_default(), &recorded, defaults)?;
        let mut followed = recorded;
        followed.extend(done.written.iter().cloned());
        let written = text.as_deref() != Some(done.text.as_str());
        let mut durability_error = None;
        let recorded = if written {
            let (settings, _) = parse(&done.text)?;
            let prepared = pending::prepare(&self.path, text.as_deref(), &done)?;
            durability_error = match self.write(&done.text, text.as_deref(), &mut backup) {
                Ok(error) => error,
                Err(error) => {
                    let _ = pending::discard(&self.path);
                    return Err(error);
                }
            };
            self.take(settings);
            pending::commit(&self.path, ledger, prepared)
        } else {
            ledger
                .record_settings_defaults(&done.written, now())
                .map_err(ledger_error)
        };
        self.followed = followed;
        let ledger_error = recorded.err().or(recovery_error);
        Ok(Completion {
            changes: done.changes,
            written,
            ledger_error,
            durability_error,
        })
    }
}

impl SettingsStore {
    fn take(&mut self, settings: Settings) {
        self.current = settings;
        self.generation += 1;
    }

    fn write(
        &self,
        text: &str,
        base: Option<&str>,
        backup: &mut bool,
    ) -> Result<Option<SettingsError>, SettingsError> {
        if is_read_only(&self.path) {
            return Err(SettingsError::ReadOnly);
        }
        let mut is_changed = false;
        let check = || match unchanged(&self.path, base) {
            Ok(true) => Ok(()),
            Ok(false) => {
                is_changed = true;
                Err(io::Error::other("the file changed on disk"))
            }
            Err(error) => Err(error),
        };
        match write_checked(&self.path, text.as_bytes(), !*backup, check) {
            Ok(outcome) => {
                *backup = true;
                Ok(match outcome {
                    WriteOutcome::Durable => None,
                    WriteOutcome::Committed(error) => {
                        Some(SettingsError::Durability(error.to_string()))
                    }
                })
            }
            Err(_) if is_changed => Err(SettingsError::ChangedOnDisk),
            Err(error) => Err(io_error(error)),
        }
    }
}

fn failed(error: SettingsError) -> LoadReport {
    LoadReport {
        unknown_keys: Vec::new(),
        error: Some(error),
    }
}

fn read(path: &Path) -> Result<Option<String>, SettingsError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error(e)),
    }
}

fn unchanged(path: &Path, base: Option<&str>) -> io::Result<bool> {
    match fs::read(path) {
        Ok(bytes) => Ok(base.is_some_and(|base| base.as_bytes() == bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(base.is_none()),
        Err(e) => Err(e),
    }
}

fn is_read_only(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.permissions().readonly())
}

fn ledger_error(error: DbError) -> SettingsError {
    SettingsError::Ledger(error.to_string())
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

fn io_error(error: io::Error) -> SettingsError {
    SettingsError::Io {
        kind: error.kind(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests;
