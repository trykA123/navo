use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

mod backup;
mod temp;

use temp::TempFile;
pub use temp::remove_orphan_temps;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Write,
    Sync,
    BackupLink,
    LinkSync,
    BackupCopy,
    BackupSync,
    BackupRename,
    BackupFolderSync,
    Rename,
    FolderSync,
}

#[cfg(test)]
use tests::before as step;

#[cfg(not(test))]
fn step(_: Op) -> io::Result<()> {
    Ok(())
}

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

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(path, bytes, |_| Ok(()))
}

/// Like `write_atomic`, but `write` streams the content, so a large file never sits in memory.
pub fn write_atomic_streamed(
    path: &Path,
    write: &dyn Fn(&mut dyn io::Write) -> io::Result<()>,
) -> io::Result<()> {
    let payload = Payload {
        body: Body::Streamed(write),
        backup: true,
        follow_links: true,
    };
    write_payload(path, &payload, &mut |_| Ok(())).and_then(WriteOutcome::into_result)
}

pub fn replace_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let payload = Payload {
        body: Body::Bytes(bytes),
        backup: false,
        follow_links: true,
    };
    write_payload(path, &payload, &mut |_| Ok(())).and_then(WriteOutcome::into_result)
}

pub fn replace_entry_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let payload = Payload {
        body: Body::Bytes(bytes),
        backup: false,
        follow_links: false,
    };
    write_payload(path, &payload, &mut |_| Ok(())).and_then(WriteOutcome::into_result)
}

enum Body<'a> {
    Bytes(&'a [u8]),
    Streamed(&'a dyn Fn(&mut dyn io::Write) -> io::Result<()>),
}

struct Payload<'a> {
    body: Body<'a>,
    backup: bool,
    follow_links: bool,
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
    let payload = Payload {
        body: Body::Bytes(bytes),
        backup,
        follow_links: true,
    };
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
        body: Body::Bytes(bytes),
        backup: true,
        follow_links: true,
    };
    write_payload(path, &payload, &mut hook).and_then(WriteOutcome::into_result)
}

fn write_payload(
    path: &Path,
    payload: &Payload<'_>,
    hook: &mut impl FnMut(Stage) -> io::Result<()>,
) -> io::Result<WriteOutcome> {
    let path = &if payload.follow_links {
        resolve_symlink(path)
    } else {
        path.to_path_buf()
    };
    if fs::metadata(path).is_ok_and(|m| m.is_dir()) {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            format!("{} is a directory", path.display()),
        ));
    }
    replace(path, payload, &mut |stage| {
        #[cfg(test)]
        tests::at_stage(path, stage)?;
        hook(stage)
    })
}

fn replace(
    path: &Path,
    payload: &Payload<'_>,
    hook: &mut impl FnMut(Stage) -> io::Result<()>,
) -> io::Result<WriteOutcome> {
    let (temp, mut file) = TempFile::create(path)?;
    write_body(&mut file, &payload.body)?;
    if let Ok(old) = fs::metadata(path) {
        fs::set_permissions(temp.path(), old.permissions())?;
    }
    hook(Stage::TempWritten)?;
    step(Op::Sync)?;
    file.sync_all()?;
    drop(file);
    hook(Stage::TempSynced)?;
    if payload.backup && fs::symlink_metadata(path).is_ok() {
        backup::make(path)?;
        hook(Stage::BackupMade)?;
    }
    hook(Stage::BeforeRename)?;
    temp.commit(path, Op::Rename)?;
    let settled = hook(Stage::Replaced)
        .and_then(|()| step(Op::FolderSync))
        .and_then(|()| sync_folder(folder_of(path)));
    Ok(match settled {
        Ok(()) => WriteOutcome::Durable,
        Err(error) => WriteOutcome::Committed(error),
    })
}

fn write_body(file: &mut File, body: &Body<'_>) -> io::Result<()> {
    step(Op::Write)?;
    match body {
        Body::Bytes(bytes) => file.write_all(bytes),
        Body::Streamed(write) => write(file),
    }
}

fn resolve_symlink(path: &Path) -> PathBuf {
    match fs::read_link(path) {
        Ok(target) => folder_of(path).join(target),
        Err(_) => path.to_owned(),
    }
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
