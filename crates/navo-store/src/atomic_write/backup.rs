use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

use super::temp::{BACKUP_MARK, TempFile};
use super::{Op, backup_path, folder_of, step, sync_folder};

/// Makes `<path>.bak` a durable copy of `path`; on error the old backup stays as it was.
pub(super) fn make(path: &Path) -> io::Result<()> {
    let staged = TempFile::reserve(path, BACKUP_MARK);
    let linked = step(Op::BackupLink).and_then(|()| fs::hard_link(path, staged.path()));
    match linked {
        Ok(()) => sync_link(staged.path())?,
        Err(_) => copy_synced(path, staged.path())?,
    }
    staged.commit(&backup_path(path), Op::BackupRename)?;
    best_effort(step(Op::BackupFolderSync).and_then(|()| sync_folder(folder_of(path))))
}

fn sync_link(staged: &Path) -> io::Result<()> {
    step(Op::LinkSync)?;
    match OpenOptions::new().write(true).open(staged) {
        Ok(file) => file.sync_all(),
        // Best effort: a read-only or locked file cannot be opened for write; the save goes on.
        Err(error) if is_refused(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

fn is_refused(error: &io::Error) -> bool {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    let shared = cfg!(windows) && error.raw_os_error() == Some(ERROR_SHARING_VIOLATION);
    error.kind() == io::ErrorKind::PermissionDenied || shared
}

fn copy_synced(path: &Path, staged: &Path) -> io::Result<()> {
    step(Op::BackupCopy)?;
    let mut source = File::open(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(source.metadata()?.permissions().mode());
    }
    let mut copy = options.open(staged)?;
    io::copy(&mut source, &mut copy)?;
    step(Op::BackupSync)?;
    copy.sync_all()
}

/// Folder sync is refused by some FUSE and network mounts; the final folder sync still runs.
pub(super) fn best_effort(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if is_unsupported(&error) => Ok(()),
        other => other,
    }
}

fn is_unsupported(error: &io::Error) -> bool {
    // 95 is Linux ENOTSUP, which std may not map to `Unsupported`.
    let not_supported = cfg!(target_os = "linux") && error.raw_os_error() == Some(95);
    let kind = matches!(
        error.kind(),
        io::ErrorKind::InvalidInput | io::ErrorKind::Unsupported
    );
    kind || not_supported
}
