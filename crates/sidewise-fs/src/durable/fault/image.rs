//! The disk image a `FaultDisk` crash leaves: the folder-entry changes it undoes, the
//! snapshots of unlinked files it brings back, and the truncation of unsynced bytes.

use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::super::{Disk, RealDisk};

/// The bytes and times of a path a removal or replace unlinked, so a crash can bring it back.
pub(super) enum Snap {
    File {
        bytes: Vec<u8>,
        modified: Option<SystemTime>,
        permissions: Permissions,
    },
    Dir(Vec<(String, Snap)>),
}

/// A folder-entry change a crash undoes until every folder it touched was synced.
pub(super) enum Change {
    Created(PathBuf),
    Renamed {
        from: PathBuf,
        to: PathBuf,
    },
    Replaced {
        staged: PathBuf,
        target: PathBuf,
        old: Option<Snap>,
    },
    Removed {
        path: PathBuf,
        old: Snap,
    },
}

/// Reads `path` (a file or a folder tree) as a crash would bring it back: a tracked file
/// only up to its synced length. Fails closed on a symlink or special file.
pub(super) fn snap(path: &Path, synced: &dyn Fn(&Path) -> Option<u64>) -> io::Result<Snap> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        let mut entries = Vec::new();
        for name in RealDisk.list(path)? {
            let child = snap(&path.join(&name), synced)?;
            entries.push((name, child));
        }
        return Ok(Snap::Dir(entries));
    }
    if !meta.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "fault layer cannot snapshot a symlink or special file",
        ));
    }
    let mut bytes = fs::read(path)?;
    if let Some(len) = synced(path) {
        bytes.truncate(usize::try_from(len).unwrap_or(usize::MAX));
    }
    Ok(Snap::File {
        bytes,
        modified: meta.modified().ok(),
        permissions: meta.permissions(),
    })
}

/// Truncates `path` to `len` bytes and keeps its modified time; a missing file is skipped.
pub(super) fn cut(path: &Path, len: u64) -> io::Result<()> {
    let file = match OpenOptions::new().write(true).open(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        opened => opened?,
    };
    let modified = file.metadata()?.modified()?;
    file.set_len(len)?;
    file.set_modified(modified)
}

/// Undoes one folder-entry change on the disk. A missing path fails with NotFound: settling
/// a durable unlink drops the changes it explains, so a gap here means the image is wrong.
pub(super) fn undo(change: Change) -> io::Result<()> {
    match change {
        Change::Created(path) => {
            if fs::symlink_metadata(&path)?.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            }
        }
        Change::Renamed { from, to } => RealDisk.rename_new(&to, &from),
        Change::Replaced {
            staged,
            target,
            old,
        } => {
            RealDisk.rename_new(&target, &staged)?;
            old.map_or(Ok(()), |old| restore(&target, &old))
        }
        Change::Removed { path, old } => restore(&path, &old),
    }
}

/// Writes a snapshot back at `path`, which must not exist.
fn restore(path: &Path, snap: &Snap) -> io::Result<()> {
    match snap {
        Snap::Dir(entries) => {
            fs::create_dir(path)?;
            entries
                .iter()
                .try_for_each(|(name, child)| restore(&path.join(name), child))
        }
        Snap::File {
            bytes,
            modified,
            permissions,
        } => {
            let mut file = File::create_new(path)?;
            file.write_all(bytes)?;
            if let Some(modified) = modified {
                file.set_modified(*modified)?;
            }
            file.set_permissions(permissions.clone())
        }
    }
}
