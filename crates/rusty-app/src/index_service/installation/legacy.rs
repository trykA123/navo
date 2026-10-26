use std::io;
use std::path::Path;

use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, FOLDERID_ProgramFiles};

use super::super::config::Config;
use super::super::directory::known_folder;
use super::super::install_plan::{CopyPlan, FERRITE};
use super::super::scm::query_running;
use super::super::storage::Storage;
use super::super::upgrade::{Legacy, carry_legacy_config};

pub(super) fn detect() -> io::Result<Legacy> {
    let service = query_running(FERRITE)?.is_some();
    let plan = CopyPlan::of(FERRITE, &known_folder(&FOLDERID_ProgramFiles)?, 0);
    let [data, _] = FERRITE.data_directories(&known_folder(&FOLDERID_ProgramData)?);
    let files = is_there(&plan.directory)? || is_there(&data)?;
    Ok(Legacy { service, files })
}

pub(super) fn carried_config() -> Option<Config> {
    let current = Storage::open(false).and_then(|storage| storage.read_config());
    carry_legacy_config(&current, read_legacy_config)
}

fn read_legacy_config() -> Option<Config> {
    match Storage::open_for(FERRITE, false).and_then(|storage| storage.read_config()) {
        Ok(config) => Some(config),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            tracing::warn!(%error, "old index service settings not carried over; using the installing user's");
            None
        }
    }
}

fn is_there(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
