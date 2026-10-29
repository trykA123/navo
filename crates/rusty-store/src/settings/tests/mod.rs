use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::complete::{
    Ledger, Reconciled, UNUSED_NOTE, complete_text, mark_unused, reconcile, upgrade_text,
};
use super::*;
use crate::{Db, backup_path};

mod complete;
mod complete_props;
mod complete_store;
mod following;
mod layout;
mod model;
mod parse;
mod props;
mod review;
mod safety;
mod snapshot;
mod store;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

fn settings_file(dir: &TempDir) -> PathBuf {
    dir.path().join("navo.toml")
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
}

const MODE: &str = "appearance.mode";

fn text(value: &str) -> SettingValue {
    SettingValue::Text(value.to_owned())
}

fn dark_mode() -> Settings {
    let mut settings = Settings::default();
    settings.appearance.mode = ThemeMode::Dark;
    settings
}

fn invalid_at(line: usize, column: usize) -> impl Fn(&SettingsError) -> bool {
    move |error| matches!(error, SettingsError::Invalid { line: l, column: c, .. } if *l == line && *c == column)
}
