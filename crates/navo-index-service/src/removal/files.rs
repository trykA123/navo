use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

    pub(crate) fn extend(&mut self, other: Self) {
        self.scheduled.extend(other.scheduled);
        self.unknown.extend(other.unknown);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Left {
    Gone,
    Scheduled,
    Kept,
}

/// Deletes the `removable` images and their copies, then the folders; never an unknown file.
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
    let others: Vec<_> = entries
        .into_iter()
        .map(|entry| entry.path)
        .filter(|path| path.as_path() != folders.index)
        .collect();
    if !others.is_empty() {
        cleanup.unknown.extend(others);
        return Ok(cleanup);
    }
    match index {
        Left::Scheduled => schedule(files, folders.parent, &mut cleanup)?,
        _ => remove_folder(files, folders.parent, &mut cleanup)?,
    };
    Ok(cleanup)
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
    tracing::warn!(path = %path.display(), %error, "service file not deleted; scheduling it");
    let renamed = path.with_file_name(format!("{image}.{}.old", limits.process));
    let target = if renamed != path && files.rename(path, &renamed).is_ok() {
        renamed
    } else {
        path.to_path_buf()
    };
    schedule(files, &target, cleanup).map(|_| ())
}

fn delete(files: &mut impl Files, path: &Path, limits: &Limits) -> io::Result<()> {
    let mut attempt = 1;
    loop {
        match files.delete(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) if attempt < limits.tries && Instant::now() < limits.deadline => {
                attempt += 1;
                std::thread::sleep(limits.delay);
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
