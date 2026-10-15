use std::io;
use std::path::{Path, PathBuf};

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
        let _ = path;
        todo!()
    }

    #[must_use]
    pub fn current(&self) -> &Settings {
        todo!()
    }

    pub fn reload(&mut self) -> LoadReport {
        todo!()
    }

    pub fn save(&mut self, settings: Settings) -> Result<(), SettingsError> {
        let _ = settings;
        todo!()
    }
}

#[cfg(test)]
mod tests;
