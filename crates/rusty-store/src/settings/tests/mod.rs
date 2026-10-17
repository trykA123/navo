use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::complete::{
    Ledger, Reconciled, UNUSED_NOTE, complete_text, mark_unused, reconcile, upgrade_text,
};
use super::*;
use crate::backup_path;

mod complete;
mod complete_props;
mod complete_store;
mod model;
mod parse;
mod props;
mod safety;
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

type Meddle = Box<dyn FnOnce(&Path)>;

thread_local! {
    static BEFORE_WRITE: RefCell<Option<Meddle>> = const { RefCell::new(None) };
}

pub(super) fn before_write(path: &Path) {
    if let Some(action) = BEFORE_WRITE.with(|hook| hook.borrow_mut().take()) {
        action(path);
    }
}

fn on_next_write(action: impl FnOnce(&Path) + 'static) {
    BEFORE_WRITE.with(|hook| *hook.borrow_mut() = Some(Box::new(action)));
}
