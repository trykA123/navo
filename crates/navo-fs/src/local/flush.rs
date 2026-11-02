use std::fs::File;
use std::io;
use std::path::Path;

#[cfg(not(windows))]
pub(super) fn flush(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
pub(super) fn flush(path: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(path)?.is_dir() {
        return Ok(());
    }
    File::options().write(true).open(path)?.sync_all()
}
