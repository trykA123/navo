use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    SettingsError,
    complete::{Ledger, Reconciled},
    io_error, ledger_error, now, read,
};
use crate::{Db, replace_atomic};

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Stage {
    Prepared,
    Committed,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Pending {
    stage: Stage,
    before: Option<String>,
    written: String,
    rows: Ledger,
}

fn path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".defaults-pending");
    PathBuf::from(name)
}

pub(super) fn recover(file: &Path, ledger: &Db) -> Result<Option<SettingsError>, SettingsError> {
    let Some(text) = read(&path(file))? else {
        return Ok(None);
    };
    let mut pending: Pending = match toml::from_str(&text) {
        Ok(pending) => pending,
        Err(error) => {
            let bad = path(file).with_extension("defaults-pending.bad");
            std::fs::rename(path(file), &bad).map_err(io_error)?;
            return Ok(Some(SettingsError::Ledger(format!(
                "invalid pending settings file moved to {}: {error}",
                bad.display()
            ))));
        }
    };
    if pending.stage == Stage::Prepared {
        let current = read(file)?;
        if current.as_deref() != Some(pending.written.as_str()) {
            if current == pending.before {
                discard(file)?;
                return Ok(None);
            }
            for value in pending.rows.values_mut() {
                value.clear();
            }
        }
    }
    let report = pending.rows.values().any(String::is_empty).then(|| {
        SettingsError::Ledger(
            "ambiguous interrupted settings write; pending keys were kept as the user's own"
                .to_owned(),
        )
    });
    match pending.stage {
        Stage::Prepared => commit(file, ledger, pending)?,
        Stage::Committed => finish(file, ledger, &pending.rows)?,
    }
    Ok(report)
}

pub(super) fn prepare(
    file: &Path,
    before: Option<&str>,
    done: &Reconciled,
) -> Result<Pending, SettingsError> {
    let pending = Pending {
        stage: Stage::Prepared,
        before: before.map(str::to_owned),
        written: done.text.clone(),
        rows: done.written.iter().cloned().collect(),
    };
    save(file, &pending)?;
    Ok(pending)
}

pub(super) fn commit(file: &Path, ledger: &Db, mut pending: Pending) -> Result<(), SettingsError> {
    pending.stage = Stage::Committed;
    let marked = save(file, &pending);
    finish(file, ledger, &pending.rows)?;
    marked
}

fn save(file: &Path, pending: &Pending) -> Result<(), SettingsError> {
    let text =
        toml::to_string(pending).map_err(|error| SettingsError::Ledger(error.to_string()))?;
    replace_atomic(&path(file), text.as_bytes()).map_err(io_error)
}

fn finish(file: &Path, ledger: &Db, rows: &Ledger) -> Result<(), SettingsError> {
    let recorded = ledger.settings_defaults().map_err(ledger_error)?;
    let changed: Vec<_> = rows
        .iter()
        .filter(|(key, value)| recorded.get(*key) != Some(*value))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    ledger
        .record_settings_defaults(&changed, now())
        .map_err(ledger_error)?;
    discard(file)
}

pub(super) fn discard(file: &Path) -> Result<(), SettingsError> {
    std::fs::remove_file(path(file)).map_err(io_error)
}
