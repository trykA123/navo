use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use tempfile::NamedTempFile;

use super::{RecoveryError, io_error, regular};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Pending {
    pub slot: u64,
    pub database: bool,
}

pub(super) fn read(path: &Path) -> Result<Option<Pending>, RecoveryError> {
    if !regular(path)? {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(65).read_to_end(&mut bytes))
        .map_err(|error| io_error("reading recovery record", path, error))?;
    let pending = (bytes.len() <= 64)
        .then(|| std::str::from_utf8(&bytes).ok().and_then(parse))
        .flatten()
        .ok_or_else(|| RecoveryError::InvalidRecord(path.to_owned()))?;
    Ok(Some(pending))
}

fn parse(text: &str) -> Option<Pending> {
    let mut parts = text.split_whitespace();
    let slot = parts.next()?.parse::<u64>().ok().filter(|slot| *slot > 0)?;
    let database = match parts.next()? {
        "database" => true,
        "sidecars" => false,
        _ => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(Pending { slot, database })
}

pub(super) fn write(path: &Path, pending: Pending) -> Result<(), RecoveryError> {
    prepare(path, pending)?
        .persist_noclobber(path)
        .map(drop)
        .map_err(|error| io_error("publishing recovery record", path, error.error))
}

pub(super) fn prepare(path: &Path, pending: Pending) -> Result<NamedTempFile, RecoveryError> {
    let parent = path
        .parent()
        .ok_or_else(|| RecoveryError::InvalidRecord(path.to_owned()))?;
    let mut file = tempfile::Builder::new()
        .prefix(&format!(".{}-quarantine-record-", rusty_core::brand::SLUG))
        .tempfile_in(parent)
        .map_err(|error| io_error("creating recovery record", path, error))?;
    let kind = if pending.database {
        "database"
    } else {
        "sidecars"
    };
    writeln!(file, "{} {kind}", pending.slot)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|error| io_error("saving recovery record", path, error))?;
    Ok(file)
}
