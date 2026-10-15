use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::*;
use crate::backup_path;

mod model;
mod parse;
mod props;
mod snapshot;
mod store;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

fn settings_file(dir: &TempDir) -> PathBuf {
    dir.path().join("ferrite.toml")
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
}

fn dark_mode() -> Settings {
    let mut settings = Settings::default();
    settings.appearance.mode = ThemeMode::Dark;
    settings
}

fn invalid_at(line: usize, column: usize) -> impl Fn(&SettingsError) -> bool {
    move |error| matches!(error, SettingsError::Invalid { line: l, column: c, .. } if *l == line && *c == column)
}
