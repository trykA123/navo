use std::fs;
use std::path::{Path, PathBuf};

use rusty_store::{Db, DbError, OpenReport};

pub(crate) fn open_db(path: &Path) -> Result<(Db, OpenReport), DbError> {
    let lease = super::store::access::lease(path)?;
    if lease.is_none() {
        return open_legacy(path, false);
    }
    let sentinel = sentinel_path(path);
    match fs::symlink_metadata(&sentinel) {
        Ok(_) => {
            return Err(DbError::Corrupt(format!(
                "database access is disabled after a failed startup check: {}",
                path.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => {
            return Err(DbError::Corrupt(format!(
                "reading {}: {error}",
                sentinel.display()
            )));
        }
    }
    let (mut db, report) = Db::open(path, false)?;
    if let Some(lease) = lease {
        db.hold_access_guard(lease);
    }
    Ok((db, report))
}

pub(super) fn open_legacy(path: &Path, unclean: bool) -> Result<(Db, OpenReport), DbError> {
    let sentinel = sentinel_path(path);
    match Db::open(path, unclean || sentinel.exists()) {
        Ok((db, report)) => {
            if report.checked
                && sentinel.exists()
                && let Err(error) = fs::remove_file(&sentinel)
            {
                tracing::warn!(%error, "could not clear the integrity sentinel");
            }
            Ok((db, report))
        }
        Err(error) => {
            if error.is_corruption() {
                record_failed_check(path);
            }
            Err(error)
        }
    }
}

pub(crate) fn record_failed_check(db: &Path) {
    let sentinel = sentinel_path(db);
    let write = match fs::symlink_metadata(&sentinel) {
        Ok(metadata) if !metadata.is_file() => Err(std::io::Error::other(
            "the integrity sentinel is not a regular file",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            rusty_store::replace_atomic(&sentinel, b"")
        }
        Err(error) => Err(error),
    };
    if let Err(error) = write {
        tracing::error!(%error, "could not record the failed integrity check");
    }
}

pub(crate) fn sentinel_path(db: &Path) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push(".check-failed");
    PathBuf::from(name)
}
