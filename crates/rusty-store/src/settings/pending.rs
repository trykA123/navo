use std::path::{Path, PathBuf};

use super::{SettingsError, complete::Ledger, io_error, ledger_error, now, read};
use crate::{Db, replace_atomic};

fn path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".defaults-pending");
    PathBuf::from(name)
}

pub(super) fn recover(file: &Path, ledger: &Db) -> Result<(), SettingsError> {
    let Some(text) = read(&path(file))? else {
        return Ok(());
    };
    let pending: Ledger =
        toml::from_str(&text).map_err(|error| SettingsError::Ledger(error.to_string()))?;
    finish(file, ledger, &pending.into_iter().collect::<Vec<_>>())
}

pub(super) fn prepare(file: &Path, rows: &[(String, String)]) -> Result<(), SettingsError> {
    let rows: Ledger = rows.iter().cloned().collect();
    let text = toml::to_string(&rows).map_err(|error| SettingsError::Ledger(error.to_string()))?;
    replace_atomic(&path(file), text.as_bytes()).map_err(io_error)
}

pub(super) fn finish(
    file: &Path,
    ledger: &Db,
    rows: &[(String, String)],
) -> Result<(), SettingsError> {
    ledger
        .record_settings_defaults(rows, now())
        .map_err(ledger_error)?;
    std::fs::remove_file(path(file)).map_err(io_error)
}

pub(super) fn discard(file: &Path) -> Result<(), SettingsError> {
    std::fs::remove_file(path(file)).map_err(io_error)
}
