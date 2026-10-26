use std::fs;
use std::path::Path;

use rusty_fs::{LocalFs, ProviderOps};

use super::state::{Identity, Notice, candidate};
use super::{Error, files};

pub(super) fn identity(path: &Path) -> Result<Identity, Error> {
    if !files::regular(path)? {
        return Err(Error::Invalid(format!("{} is missing", path.display())));
    }
    let location = crate::startup::path_location(path)
        .ok_or_else(|| Error::Invalid("database path is not usable".into()))?;
    let found = LocalFs::new()
        .file_identity(&location)
        .map_err(|error| Error::Invalid(error.message()))?
        .ok_or_else(|| Error::Invalid("database file identity is unavailable".into()))?;
    Ok(Identity {
        kind: found.kind,
        volume: found.volume,
        file: found.file,
    })
}

pub(super) fn matches(path: &Path, expected: &Identity) -> Result<bool, Error> {
    if !files::regular(path)? {
        return Ok(false);
    }
    let actual = identity(path)?;
    Ok(actual.kind == expected.kind
        && actual.volume == expected.volume
        && actual.file == expected.file)
}

pub(super) fn install(data: &Path, notice: &Notice, expected: &Identity) -> Result<(), Error> {
    let stage = candidate(data, notice.case, notice.attempt);
    let target = rusty_store::StoreDirs::journal_in(data);
    if files::regular(&stage)? && !matches(&stage, expected)? {
        return Err(Error::Invalid(
            "the recovery candidate was replaced; both files were kept".into(),
        ));
    }
    let mut consumed = false;
    if !files::regular(&target)? {
        if !matches(&stage, expected)? {
            return Err(Error::Invalid("the recovery candidate is missing".into()));
        }
        for suffix in ["-wal", "-shm", "-journal"] {
            if files::regular(&files::suffix(&target, suffix))? {
                return Err(Error::Invalid(
                    "unexpected database sidecars prevent promotion".into(),
                ));
            }
        }
        consumed = publish(&stage, &target)?;
    }
    finish((&stage, &target), expected, consumed)
}

pub(super) fn finish(
    (stage, target): (&Path, &Path),
    expected: &Identity,
    consumed: bool,
) -> Result<(), Error> {
    if !matches(target, expected)? {
        return Err(Error::Invalid(
            "the database identity could not be confirmed after promotion; existing files were kept".into(),
        ));
    }
    if !consumed && files::regular(stage)? {
        if !matches(stage, expected)? {
            return Err(Error::Invalid(
                "the recovery candidate changed before cleanup; existing files were kept".into(),
            ));
        }
        fs::remove_file(stage)?;
    }
    Ok(())
}

fn publish(stage: &Path, target: &Path) -> Result<bool, Error> {
    if rusty_fs::native_rename_atomic(stage, target).is_ok() {
        return Ok(true);
    }
    fs::hard_link(stage, target)?;
    Ok(false)
}
