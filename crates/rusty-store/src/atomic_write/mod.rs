use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    TempWritten,
    TempSynced,
    BackupMade,
    BeforeRename,
    Replaced,
}

pub(crate) enum WriteOutcome {
    Durable,
    Committed(io::Error),
}

impl WriteOutcome {
    fn into_result(self) -> io::Result<()> {
        match self {
            Self::Durable => Ok(()),
            Self::Committed(error) => Err(error),
        }
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(path, bytes, |_| Ok(()))
}

pub fn replace_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let payload = Payload {
        bytes,
        backup: false,
    };
    write_payload(path, &payload, &mut |_| Ok(())).and_then(WriteOutcome::into_result)
}

struct Payload<'a> {
    bytes: &'a [u8],
    backup: bool,
}

#[must_use]
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

pub(crate) fn write_checked(
    path: &Path,
    bytes: &[u8],
    backup: bool,
    mut check: impl FnMut() -> io::Result<()>,
) -> io::Result<WriteOutcome> {
    let payload = Payload { bytes, backup };
    write_payload(path, &payload, &mut |stage| match stage {
        Stage::TempSynced | Stage::BeforeRename => check(),
        _ => Ok(()),
    })
}

pub(crate) fn write_atomic_with(
    path: &Path,
    bytes: &[u8],
    mut hook: impl FnMut(Stage) -> io::Result<()>,
) -> io::Result<()> {
    let payload = Payload {
        bytes,
        backup: true,
    };
    write_payload(path, &payload, &mut hook).and_then(WriteOutcome::into_result)
}

fn write_payload(
    path: &Path,
    payload: &Payload<'_>,
    hook: &mut impl FnMut(Stage) -> io::Result<()>,
) -> io::Result<WriteOutcome> {
    let path = &resolve_symlink(path);
    if fs::metadata(path).is_ok_and(|m| m.is_dir()) {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            format!("{} is a directory", path.display()),
        ));
    }
    let temp = temp_path(path);
    let result = replace(path, &temp, payload, &mut |stage| {
        #[cfg(test)]
        tests::at_stage(path, stage)?;
        hook(stage)
    });
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn replace(
    path: &Path,
    temp: &Path,
    payload: &Payload<'_>,
    hook: &mut impl FnMut(Stage) -> io::Result<()>,
) -> io::Result<WriteOutcome> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(temp)?;
    file.write_all(payload.bytes)?;
    if let Ok(old) = fs::metadata(path) {
        fs::set_permissions(temp, old.permissions())?;
    }
    hook(Stage::TempWritten)?;
    file.sync_all()?;
    drop(file);
    hook(Stage::TempSynced)?;
    if payload.backup && fs::symlink_metadata(path).is_ok() {
        make_backup(path);
        hook(Stage::BackupMade)?;
    }
    hook(Stage::BeforeRename)?;
    fs::rename(temp, path)?;
    Ok(
        match hook(Stage::Replaced).and_then(|()| sync_folder(folder_of(path))) {
            Ok(()) => WriteOutcome::Durable,
            Err(error) => WriteOutcome::Committed(error),
        },
    )
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
pub(crate) mod tests;
