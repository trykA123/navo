//! Flushes a local file to the device (`sync_all`) so a later step can rely on its bytes.
//! Used before a rename is made durable and before a copied file is verified.

use std::fs::File;
use std::io;
use std::path::Path;

/// Syncs the file at `path` to the device; fails when it cannot be opened or synced.
#[cfg(not(windows))]
pub(super) fn flush(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

/// Syncs the file at `path` to the device; a folder is skipped. Windows needs write access to
/// flush, so a read-only file has the attribute cleared for the flush and restored on every path.
#[cfg(windows)]
pub(super) fn flush(path: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        return Ok(());
    }
    if !metadata.permissions().readonly() {
        return File::options().write(true).open(path)?.sync_all();
    }
    let mut writable = metadata.permissions();
    #[expect(
        clippy::permissions_set_readonly_false,
        reason = "on Windows this clears only the read-only attribute"
    )]
    writable.set_readonly(false);
    std::fs::set_permissions(path, writable)?;
    let flushed = File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all());
    let restored = std::fs::set_permissions(path, metadata.permissions());
    flushed.and(restored)
}
