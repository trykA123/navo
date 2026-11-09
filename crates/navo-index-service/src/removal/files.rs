//! File cleanup for service removal: deletes known images, then the folders they leave empty.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Navo's own change-lock file in the install parent; it outlives the removal until the lock drops.
pub(crate) const CHANGE_LOCK: &str = "change.lock";

pub(crate) struct Entry {
    pub path: PathBuf,
    pub is_file: bool,
}

/// The file calls remove makes; Windows implements them, tests fake them.
pub(crate) trait Files {
    fn list(&mut self, folder: &Path) -> io::Result<Vec<Entry>>;
    fn delete(&mut self, path: &Path) -> io::Result<()>;
    /// Renames without replacing: an existing `to` is an error.
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()>;
    /// Asks Windows to delete `path` at the next restart.
    fn schedule(&mut self, path: &Path) -> io::Result<()>;
    fn remove_folder(&mut self, path: &Path) -> io::Result<()>;
}

pub(crate) struct Limits {
    pub tries: u32,
    pub delay: Duration,
    pub deadline: Instant,
    /// Names the renamed `<image>.<process>.old` copy.
    pub process: u32,
}

/// The install folder and its parent, `Program Files\Navo\Index` and `Program Files\Navo`.
#[derive(Clone, Copy)]
pub(crate) struct Folders<'a> {
    pub parent: &'a Path,
    pub index: &'a Path,
    /// Navo's own file in the parent, which the held lock keeps until `remove_parent`.
    pub lock: Option<&'a str>,
}

/// What remove left: paths scheduled for deletion at restart, in order, and files it kept.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Cleanup {
    pub scheduled: Vec<PathBuf>,
    pub unknown: Vec<PathBuf>,
}

impl Cleanup {
    pub(crate) fn is_complete(&self) -> bool {
        self.scheduled.is_empty() && self.unknown.is_empty()
    }

    /// Deferred when anything waits for a restart; kept unknown files do not defer.
    pub(crate) fn outcome(&self) -> crate::Elevated {
        if self.scheduled.is_empty() {
            crate::Elevated::Done
        } else {
            crate::Elevated::Deferred
        }
    }

    pub(crate) fn extend(&mut self, other: Self) {
        self.scheduled.extend(other.scheduled);
        self.unknown.extend(other.unknown);
    }
}

#[derive(Debug, thiserror::Error)]
#[error("cannot delete or rename service image {path}: {source}", path = .path.display())]
pub(crate) struct LockedImage {
    pub path: PathBuf,
    #[source]
    pub source: io::Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Left {
    Gone,
    Scheduled,
    Kept,
}

/// Deletes the `removable` images and their copies, then the folders; never an unknown file.
/// `Folders::lock` names Navo's own change file, which is kept until its handle is dropped.
pub(crate) fn clean(
    files: &mut impl Files,
    folders: Folders<'_>,
    removable: &[&str],
    limits: &Limits,
) -> io::Result<Cleanup> {
    let mut cleanup = Cleanup::default();
    let index = clean_folder(files, folders.index, (removable, limits), &mut cleanup)?;
    if index == Left::Kept {
        return Ok(cleanup);
    }
    let entries = match files.list(folders.parent) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(cleanup),
        entries => entries?,
    };
    let mut held_lock = false;
    let mut others = Vec::new();
    for entry in entries {
        if entry.path == folders.index {
            continue;
        }
        if folders
            .lock
            .is_some_and(|name| entry.is_file && is_lock_name(&entry.path, name))
        {
            held_lock = true;
        } else {
            others.push(entry.path);
        }
    }
    if !others.is_empty() {
        cleanup.unknown.extend(others);
        return Ok(cleanup);
    }
    if held_lock {
        if index == Left::Scheduled {
            // Index is already first in the reboot list; the parent must follow it.
            schedule(files, folders.parent, &mut cleanup)?;
        } else {
            // The delete-on-close handle removes change.lock; cleanup then tries the parent.
            tracing::debug!(path = %folders.parent.display(), "service folder kept while change.lock is held");
        }
        return Ok(cleanup);
    }
    match index {
        Left::Scheduled => schedule(files, folders.parent, &mut cleanup)?,
        _ => remove_folder(files, folders.parent, &mut cleanup)?,
    };
    Ok(cleanup)
}

/// Best effort after the lock handle closes: removes an empty Index, then the parent.
/// Every failure is debug-only; RemoveDirectory refuses a non-empty folder, so another lock survives.
pub(crate) fn remove_parent(files: &mut impl Files, folders: Folders<'_>) {
    let entries = match files.list(folders.parent) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Ok(entries) => entries,
        Err(error) => {
            tracing::debug!(path = %folders.parent.display(), %error, "service folder not listed; keeping it");
            return;
        }
    };
    let only_index = entries
        .iter()
        .all(|entry| entry.path == folders.index && !entry.is_file);
    if !only_index {
        tracing::debug!(path = %folders.parent.display(), "service folder holds other files; keeping it");
        return;
    }
    match files.list(folders.index) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(items) if items.is_empty() => {}
        Ok(_) => {
            tracing::debug!(path = %folders.index.display(), "index folder is not empty; keeping it");
            return;
        }
        Err(error) => {
            tracing::debug!(path = %folders.index.display(), %error, "index folder not listed; keeping it");
            return;
        }
    }
    remove_folder_best_effort(files, folders.index);
    remove_folder_best_effort(files, folders.parent);
}

/// Removes one folder; a refusal is debug-only and never changes the action result.
fn remove_folder_best_effort(files: &mut impl Files, folder: &Path) {
    if let Err(error) = files.remove_folder(folder)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::debug!(path = %folder.display(), %error, "service folder not removed; keeping it");
    }
}

/// True when `path` is Navo's lock file; Windows names ignore case.
fn is_lock_name(path: &Path, lock: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(lock))
}

fn clean_folder(
    files: &mut impl Files,
    folder: &Path,
    (removable, limits): (&[&str], &Limits),
    cleanup: &mut Cleanup,
) -> io::Result<Left> {
    let entries = match files.list(folder) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Left::Gone),
        entries => entries?,
    };
    let (scheduled, unknown) = (cleanup.scheduled.len(), cleanup.unknown.len());
    for entry in entries {
        let image = entry
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| removable_image(removable, name));
        match image {
            Some(image) if entry.is_file => {
                remove_file(files, (&entry.path, image), limits, cleanup)?;
            }
            _ => cleanup.unknown.push(entry.path),
        }
    }
    if cleanup.unknown.len() > unknown {
        return Ok(Left::Kept);
    }
    if cleanup.scheduled.len() > scheduled {
        return schedule(files, folder, cleanup);
    }
    remove_folder(files, folder, cleanup)
}

fn remove_file(
    files: &mut impl Files,
    (path, image): (&Path, &str),
    limits: &Limits,
    cleanup: &mut Cleanup,
) -> io::Result<()> {
    let Err(error) = delete(files, path, limits) else {
        return Ok(());
    };
    let renamed = path.with_file_name(format!("{image}.{}.old", limits.process));
    let target = if renamed != path && files.rename(path, &renamed).is_ok() {
        renamed
    } else {
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(image))
        {
            tracing::error!(path = %path.display(), %error, "service image is locked; close the program using it and retry");
            return Err(io::Error::other(LockedImage {
                path: path.to_path_buf(),
                source: error,
            }));
        }
        path.to_path_buf()
    };
    tracing::warn!(path = %target.display(), %error, "service file not deleted; scheduling it");
    schedule(files, &target, cleanup).map(|_| ())
}

fn delete(files: &mut impl Files, path: &Path, limits: &Limits) -> io::Result<()> {
    let mut attempt = 1;
    loop {
        match files.delete(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) if attempt < limits.tries && Instant::now() < limits.deadline => {
                attempt += 1;
                std::thread::sleep(
                    limits
                        .delay
                        .min(limits.deadline.saturating_duration_since(Instant::now())),
                );
            }
            result => return result,
        }
    }
}

fn remove_folder(files: &mut impl Files, folder: &Path, cleanup: &mut Cleanup) -> io::Result<Left> {
    match files.remove_folder(folder) {
        Ok(()) => Ok(Left::Gone),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Left::Gone),
        Err(error) => {
            tracing::warn!(path = %folder.display(), %error, "service folder not removed; scheduling it");
            schedule(files, folder, cleanup)
        }
    }
}

fn schedule(files: &mut impl Files, path: &Path, cleanup: &mut Cleanup) -> io::Result<Left> {
    files.schedule(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "cannot schedule {} for deletion at the next restart: {error}",
                path.display()
            ),
        )
    })?;
    cleanup.scheduled.push(path.to_path_buf());
    Ok(Left::Scheduled)
}

/// The image `name` is, or is a numbered `.old` or `.tmp` copy of; Windows names ignore case.
pub(crate) fn removable_image<'a>(removable: &[&'a str], name: &str) -> Option<&'a str> {
    removable
        .iter()
        .copied()
        .find(|image| is_copy_of(image, name))
}

fn is_copy_of(image: &str, name: &str) -> bool {
    let Some((head, rest)) = name.split_at_checked(image.len()) else {
        return false;
    };
    if !head.eq_ignore_ascii_case(image) {
        return false;
    }
    if rest.is_empty() {
        return true;
    }
    rest.strip_prefix('.')
        .and_then(|rest| rest.rsplit_once('.'))
        .is_some_and(|(nonce, suffix)| {
            is_nonce(nonce)
                && ["old", "tmp"]
                    .iter()
                    .any(|kind| suffix.eq_ignore_ascii_case(kind))
        })
}

fn is_nonce(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
