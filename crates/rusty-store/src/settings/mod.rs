use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::write_atomic;

mod model;
mod parse;

pub use model::{
    Appearance, Density, Editor, Files, SelectionSettings, Settings, SortKey, Terminal, ThemeMode,
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
    #[error("the settings file changed on disk; reload it before saving")]
    FileChanged,
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
    on_disk: Option<String>,
}

impl SettingsStore {
    pub fn load(path: &Path) -> (Self, LoadReport) {
        let mut store = Self {
            path: path.to_owned(),
            current: Settings::default(),
            on_disk: None,
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
            self.on_disk = None;
            return LoadReport::default();
        };
        match parse(&text) {
            Ok((settings, unknown_keys)) => {
                self.current = settings;
                self.on_disk = Some(text);
                LoadReport {
                    unknown_keys,
                    error: None,
                }
            }
            Err(error) => failed(error),
        }
    }

    pub fn save(&mut self, settings: Settings) -> Result<(), SettingsError> {
        if let Some(text) = read(&self.path)? {
            parse(&text).map_err(|_| SettingsError::FileInvalid)?;
            if self.on_disk.as_deref() != Some(text.as_str()) {
                return Err(SettingsError::FileChanged);
            }
        }
        let text = parse::try_to_toml(&settings).map_err(|e| SettingsError::Io {
            kind: io::ErrorKind::InvalidData,
            message: e.to_string(),
        })?;
        write_atomic(&self.path, text.as_bytes()).map_err(io_error)?;
        self.current = settings;
        self.on_disk = Some(text);
        Ok(())
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
