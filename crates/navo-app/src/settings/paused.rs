use std::path::{Path, PathBuf};

use navo_store::SettingsError;

use super::Loaded;

pub(super) fn guarded(loaded: Option<&Loaded>) -> Option<PathBuf> {
    loaded
        .filter(|loaded| loaded.guards_deletes)
        .map(|loaded| loaded.path.clone())
}

pub(super) fn describe(error: &SettingsError, guarded: Option<&Path>) -> String {
    let problem = super::describe(error);
    if guarded.is_some_and(|path| crate::jobs::read_retention(path).is_err()) {
        navo_l10n::t!("settings-deletes-paused", problem = problem)
    } else {
        problem
    }
}
