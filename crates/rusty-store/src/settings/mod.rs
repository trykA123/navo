use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::atomic_write::write_checked;
use crate::{Db, DbError};

mod complete;
mod edit;
mod model;
mod parse;

pub use complete::DefaultChange;
use complete::reconcile;
pub use edit::{SettingValue, key_line};

pub use model::{
    Appearance, Density, Editor, Files, HiddenFiles, SelectionSettings, Settings, SortKey,
    Terminal, ThemeMode,
};
pub use parse::{parse, to_toml};

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
    #[error("the file is read-only; Ferrite does not change it")]
    ReadOnly,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Saved {
    pub was_present: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SessionBackup(Arc<AtomicBool>);

impl SessionBackup {
    fn is_made(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn made(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    current: Settings,
    generation: u64,
    backup: SessionBackup,
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

    pub fn save(&mut self, key: &str, value: Option<SettingValue>) -> Result<Saved, SettingsError> {
        let text = read(&self.path)?;
        if let Some(text) = &text {
            parse(text).map_err(|_| SettingsError::FileInvalid)?;
        }
        let base = text.as_deref();
        let was_present = base.is_some_and(|text| key_line(text, key).is_some());
        let source = match base {
            Some(text) => Cow::Borrowed(text),
            None => Cow::Owned(complete::complete_text("", &self.current)?.text),
        };
        let patched = edit::patch(&source, key, value)?;
        let (settings, _) = parse(&patched)?;
        if base != Some(patched.as_str()) {
            self.write(&patched, base)?;
        }
        self.take(settings);
        Ok(Saved { was_present })
    }

    pub fn complete(
        &mut self,
        ledger: &Db,
        defaults: &Settings,
    ) -> Result<Completion, SettingsError> {
        if is_read_only(&self.path) {
            return Err(SettingsError::ReadOnly);
        }
        let text = read(&self.path)?;
        let recorded = ledger.settings_defaults().map_err(ledger_error)?;
        let done = reconcile(text.as_deref().unwrap_or_default(), &recorded, defaults)?;
        let written = text.as_deref() != Some(done.text.as_str());
        if written {
            let (settings, _) = parse(&done.text)?;
            self.write(&done.text, text.as_deref())?;
            self.take(settings);
        }
        let ledger_error = ledger
            .record_settings_defaults(&done.written, now())
            .map_err(ledger_error)
            .err();
        Ok(Completion {
            changes: done.changes,
            written,
            ledger_error,
        })
    }
}

impl SettingsStore {
    fn take(&mut self, settings: Settings) {
        self.current = settings;
        self.generation += 1;
    }

    fn write(&self, text: &str, base: Option<&str>) -> Result<(), SettingsError> {
        if is_read_only(&self.path) {
            return Err(SettingsError::ReadOnly);
        }
        #[cfg(test)]
        tests::before_write(&self.path);
        let mut is_changed = false;
        let check = || match unchanged(&self.path, base) {
            Ok(true) => Ok(()),
            Ok(false) => {
                is_changed = true;
                Err(io::Error::other("the file changed on disk"))
            }
            Err(error) => Err(error),
        };
        let backup = !self.backup.is_made();
        match write_checked(&self.path, text.as_bytes(), backup, check) {
            Ok(()) => {
                self.backup.made();
                Ok(())
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
