use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{Db, replace_atomic, write_atomic};

mod complete;
mod edit;
mod model;
mod parse;

pub use complete::DefaultChange;
pub(crate) use complete::{
    Ledger, Reconciled, UNUSED_NOTE, complete_text, mark_unused, reconcile, upgrade_text,
};
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Saved {
    pub was_present: bool,
}

#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    current: Settings,
    is_backed_up: bool,
}

impl SettingsStore {
    pub fn load(path: &Path) -> (Self, LoadReport) {
        let mut store = Self {
            path: path.to_owned(),
            current: Settings::default(),
            is_backed_up: false,
        };
        let report = store.reload();
        (store, report)
    }

    #[must_use]
    pub fn current(&self) -> &Settings {
        &self.current
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
                self.current = settings;
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
        let text = text.unwrap_or_default();
        let was_present = key_line(&text, key).is_some();
        let patched = edit::patch(&text, key, value)?;
        let (settings, _) = parse(&patched)?;
        if patched != text {
            self.write(&patched)?;
        }
        self.current = settings;
        Ok(Saved { was_present })
    }

    pub fn complete(
        &mut self,
        ledger: &Db,
        defaults: &Settings,
    ) -> Result<Completion, SettingsError> {
        let _ = (ledger, defaults);
        todo!()
    }
}

impl SettingsStore {
    fn write(&mut self, text: &str) -> Result<(), SettingsError> {
        if self.is_backed_up {
            replace_atomic(&self.path, text.as_bytes()).map_err(io_error)
        } else {
            write_atomic(&self.path, text.as_bytes()).map_err(io_error)?;
            self.is_backed_up = true;
            Ok(())
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

fn io_error(error: io::Error) -> SettingsError {
    SettingsError::Io {
        kind: error.kind(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests;
