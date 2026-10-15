use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    TempWritten,
    TempSynced,
    BackupMade,
    Replaced,
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(path, bytes, |_| Ok(()))
}

#[must_use]
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

pub(crate) fn write_atomic_with(
    path: &Path,
    bytes: &[u8],
    mut hook: impl FnMut(Stage) -> io::Result<()>,
) -> io::Result<()> {
    let path = &resolve_symlink(path);
    if fs::metadata(path).is_ok_and(|m| m.is_dir()) {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            format!("{} is a directory", path.display()),
        ));
    }
    let temp = temp_path(path);
    let result = replace(path, &temp, bytes, &mut hook);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn replace(
    path: &Path,
    temp: &Path,
    bytes: &[u8],
    hook: &mut impl FnMut(Stage) -> io::Result<()>,
) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(temp)?;
    file.write_all(bytes)?;
    hook(Stage::TempWritten)?;
    file.sync_all()?;
    drop(file);
    hook(Stage::TempSynced)?;
    if fs::symlink_metadata(path).is_ok() {
        make_backup(path);
        hook(Stage::BackupMade)?;
    }
    fs::rename(temp, path)?;
    sync_folder(folder_of(path))?;
    hook(Stage::Replaced)
}

fn make_backup(path: &Path) {
    let staged = temp_path(path);
    let linked = fs::hard_link(path, &staged).or_else(|_| fs::copy(path, &staged).map(|_| ()));
    let _ = linked.and_then(|()| fs::rename(&staged, backup_path(path)));
    let _ = fs::remove_file(&staged);
}

fn resolve_symlink(path: &Path) -> PathBuf {
    match fs::read_link(path) {
        Ok(target) => folder_of(path).join(target),
        Err(_) => path.to_owned(),
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.tmp-{}-{n}", std::process::id()))
}

fn folder_of(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

#[cfg(unix)]
fn sync_folder(folder: &Path) -> io::Result<()> {
    fs::File::open(folder)?.sync_all()
}

#[cfg(not(unix))]
fn sync_folder(_folder: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests;
