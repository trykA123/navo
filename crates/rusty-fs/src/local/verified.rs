use std::fs::File;
use std::io;
use std::path::Path;

use rusty_core::{FsError, FsErrorKind, Location};

use super::{error, io_error, os_path};
use crate::provider::Seen;
use crate::{EntryKind, MoveProof, RenameSkip, VerifiedRename};

pub(super) fn rename(
    from: &Location,
    to: &Location,
    expect: &MoveProof,
) -> Result<VerifiedRename, FsError> {
    let (from_path, to_path) = (os_path(from)?, os_path(to)?);
    let file = match pin(&from_path) {
        Ok(file) => file,
        Err(failure) if failure.kind() == io::ErrorKind::NotFound => {
            return Ok(VerifiedRename::Skipped(RenameSkip::Missing));
        }
        Err(failure) if in_use(&failure) => {
            return Ok(VerifiedRename::Skipped(RenameSkip::InUse));
        }
        Err(failure) => return Err(io_error(&failure, from)),
    };
    let metadata = file.metadata().map_err(|e| io_error(&e, from))?;
    let identity = super::identity::from_file(&file)
        .map_err(|e| io_error(&e, from))?
        .ok_or_else(|| error(FsErrorKind::IdentityUnavailable, from))?;
    let seen = Seen {
        identity,
        kind: if metadata.is_dir() {
            EntryKind::Folder
        } else {
            EntryKind::File
        },
        size: metadata.len(),
        modified_ms: super::modified_ms(&metadata),
    };
    if let Some(skip) = expect.refuses(&seen, from)? {
        return Ok(VerifiedRename::Skipped(skip));
    }
    between();
    match move_pinned(&file, ((from, &from_path), (to, &to_path)), seen) {
        Ok(moved) => Ok(moved),
        Err(failure) if failure.kind() == io::ErrorKind::AlreadyExists => {
            Ok(VerifiedRename::Skipped(RenameSkip::OriginOccupied))
        }
        Err(failure) if in_use(&failure) => Ok(VerifiedRename::Skipped(RenameSkip::InUse)),
        Err(failure) => Err(io_error(&failure, from)),
    }
}

#[cfg(windows)]
fn in_use(failure: &io::Error) -> bool {
    use windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION;
    failure.raw_os_error() == i32::try_from(ERROR_SHARING_VIOLATION).ok()
}

#[cfg(not(windows))]
fn in_use(_: &io::Error) -> bool {
    false
}

#[cfg(windows)]
fn pin(path: &Path) -> io::Result<File> {
    super::win_rename_handle::open(path)
}

#[cfg(windows)]
fn move_pinned(file: &File, (_, (_, to)): Ends<'_>, _: Seen) -> io::Result<VerifiedRename> {
    super::win_rename_handle::rename(file, to).map(|()| VerifiedRename::Renamed)
}

#[cfg(target_os = "linux")]
fn pin(path: &Path) -> io::Result<File> {
    use rustix::fs::{Mode, OFlags, open};
    let flags = OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    Ok(open(path, flags, Mode::empty())?.into())
}

#[cfg(all(unix, not(target_os = "linux")))]
fn pin(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits().cast_signed())
        .open(path)
}

#[cfg(unix)]
fn move_pinned(
    _: &File,
    ((from, from_path), (to, to_path)): Ends<'_>,
    seen: Seen,
) -> io::Result<VerifiedRename> {
    let unresolved = |at: &Location| VerifiedRename::Unresolved { at: at.clone() };
    super::platform::rename_new(from_path, to_path)?;
    let landed = match super::identity::read(to_path) {
        Ok(Some(found)) if found == seen.identity => return Ok(VerifiedRename::Renamed),
        Ok(Some(found)) => found,
        Ok(None) | Err(_) => return Ok(unresolved(to)),
    };
    before_moving_back();
    if super::platform::rename_new(to_path, from_path).is_err() {
        return Ok(unresolved(to));
    }
    match super::identity::read(from_path) {
        Ok(Some(found)) if found == landed => Ok(VerifiedRename::Skipped(RenameSkip::Replaced)),
        _ => Ok(unresolved(from)),
    }
}

type Ends<'a> = ((&'a Location, &'a Path), (&'a Location, &'a Path));

#[cfg(test)]
type Hook = std::cell::RefCell<Option<Box<dyn FnOnce()>>>;

#[cfg(test)]
thread_local! {
    static BETWEEN: Hook = const { std::cell::RefCell::new(None) };
    static MOVE_BACK: Hook = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) fn before_rename(hook: impl FnOnce() + 'static) {
    BETWEEN.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(all(test, unix))]
pub(super) fn before_move_back(hook: impl FnOnce() + 'static) {
    MOVE_BACK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn between() {
    #[cfg(test)]
    if let Some(hook) = BETWEEN.with(|slot| slot.borrow_mut().take()) {
        hook();
    }
}

#[cfg(unix)]
fn before_moving_back() {
    #[cfg(test)]
    if let Some(hook) = MOVE_BACK.with(|slot| slot.borrow_mut().take()) {
        hook();
    }
}
