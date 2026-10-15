use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    TempWritten,
    TempSynced,
    BackupMade,
    Replaced,
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let _ = (path, bytes);
    todo!()
}

pub fn backup_path(path: &Path) -> PathBuf {
    let _ = path;
    todo!()
}

pub(crate) fn write_atomic_with(
    path: &Path,
    bytes: &[u8],
    hook: impl FnMut(Stage) -> io::Result<()>,
) -> io::Result<()> {
    let _ = (path, bytes, hook);
    todo!()
}

#[cfg(test)]
mod tests;
