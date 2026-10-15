use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::write_atomic;

mod model;
mod parse;

pub use model::{
    Appearance, Density, Files, SelectionSettings, Settings, SortKey, Terminal, ThemeMode,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadReport {
    pub unknown_keys: Vec<String>,
    pub error: Option<SettingsError>,
}

#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    current: Settings,
}

impl SettingsStore {
    pub fn load(path: &Path) -> (Self, LoadReport) {
        let mut store = Self {
            path: path.to_owned(),
            current: Settings::default(),
        };
        let report = store.reload();
        (store, report)
    }

    #[must_use]
    pub fn current(&self) -> &Settings {
        &self.current
    }

    pub fn reload(&mut self) -> LoadReport {
        match read(&self.path).and_then(|text| text.map(|t| parse(&t)).transpose()) {
            Ok(None) => LoadReport::default(),
            Ok(Some((settings, unknown_keys))) => {
                self.current = settings;
                LoadReport {
                    unknown_keys,
                    error: None,
                }
            }
            Err(error) => LoadReport {
                unknown_keys: Vec::new(),
                error: Some(error),
            },
        }
    }

    pub fn save(&mut self, settings: Settings) -> Result<(), SettingsError> {
        if let Some(text) = read(&self.path)? {
            parse(&text).map_err(|_| SettingsError::FileInvalid)?;
        }
        let text = parse::try_to_toml(&settings).map_err(|e| SettingsError::Io {
            kind: io::ErrorKind::InvalidData,
            message: e.to_string(),
        })?;
        write_atomic(&self.path, text.as_bytes()).map_err(io_error)?;
        self.current = settings;
        Ok(())
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
