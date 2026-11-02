//! Moving a damaged database aside and wiping every clipboard file.
//!
//! A damaged file moves to `<name>.corrupt-<stamp>` and stays there; only an explicit
//! [`ClipStore::wipe_all`] deletes these set-aside copies.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::limits::Limits;

use super::{ClipError, ClipStore};

const MAX_ASIDE_ATTEMPTS: u32 = 100;
const ASIDE_MARK: &str = ".corrupt-";
// Companions move before the main file, so a crash never leaves an old WAL beside a fresh file.
// A crash between the -wal move and the main move leaves the WAL under the aside name.
const SUFFIXES: [&str; 3] = ["-wal", "-shm", ""];

/// How [`ClipStore::open`] found the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    Clean,
    /// The damaged file now lives at this path, with its `-wal` and `-shm` beside it.
    MovedAside(PathBuf),
}

#[derive(Debug)]
pub(in crate::store) struct Rename {
    pub from: PathBuf,
    pub to: PathBuf,
}

impl ClipStore {
    /// Closes the store, removes the database with its `-wal` and `-shm` and every
    /// `<name>.corrupt-<stamp>` set-aside copy of a damaged file beside it, and opens it empty.
    /// Aside copies go first, so a failure to remove one keeps the live database; the first
    /// failure is returned as [`ClipError::Io`] and the remaining files stay.
    /// `path` must name the open database; any other path is refused with
    /// [`ClipError::WrongPath`] before anything is touched.
    /// This consumes the store. On error the files stay and the caller must reopen the store.
    pub fn wipe_all(self, path: &Path) -> Result<ClipStore, ClipError> {
        let open_path = self.open_path(path)?;
        let limits = self.limits;
        self.checkpoint()?;
        self.conn.close().map_err(|(_, error)| error)?;
        for aside in find_asides(&open_path)? {
            remove_if_present(&aside)?;
        }
        for suffix in SUFFIXES {
            remove_if_present(&suffixed(&open_path, suffix))?;
        }
        sync_parent(&open_path);
        ClipStore::open(&open_path, limits)
    }

    fn open_path(&self, claimed: &Path) -> Result<PathBuf, ClipError> {
        let open = PathBuf::from(self.conn.path().ok_or(ClipError::WrongPath)?);
        let same = open == claimed
            || matches!(
                (fs::canonicalize(&open), fs::canonicalize(claimed)),
                (Ok(a), Ok(b)) if a == b
            );
        if same {
            Ok(open)
        } else {
            Err(ClipError::WrongPath)
        }
    }
}

pub(in crate::store) struct Moved {
    pub aside: PathBuf,
    renames: Vec<Rename>,
}

pub(super) fn recover(path: &Path, limits: Limits) -> Result<ClipStore, ClipError> {
    let moved = move_aside(path, now_ms())?;
    let absent = absent_live(path);
    match ClipStore::open_once(path, limits) {
        Ok(mut store) => {
            store.opened = Opened::MovedAside(moved.aside);
            Ok(store)
        }
        Err(error) => match restore(path, &moved.renames, &absent) {
            Ok(()) => Err(error),
            Err(_) => Err(ClipError::RestoreIncomplete {
                source: Box::new(error),
            }),
        },
    }
}

/// Renames the database and its companions to a free `<name>.corrupt-<stamp>` name.
pub(in crate::store) fn move_aside(path: &Path, stamp: u128) -> Result<Moved, ClipError> {
    let renames = plan_aside(path, stamp)?;
    rename_all(&renames)?;
    sync_parent(path);
    Ok(Moved {
        aside: aside_of(path, &renames),
        renames,
    })
}

/// Renames in order; on a failure renames the moved files back and returns the error, or
/// `RestoreIncomplete` when a move back failed too.
pub(in crate::store) fn rename_all(renames: &[Rename]) -> Result<(), ClipError> {
    for (done, rename) in renames.iter().enumerate() {
        if let Err(error) = move_no_replace(&rename.from, &rename.to) {
            let mut undone = true;
            for undo in renames[..done].iter().rev() {
                undone &= move_no_replace(&undo.to, &undo.from).is_ok();
            }
            let error = ClipError::Io(error);
            return Err(if undone {
                error
            } else {
                ClipError::RestoreIncomplete {
                    source: Box::new(error),
                }
            });
        }
    }
    Ok(())
}

fn plan_aside(path: &Path, stamp: u128) -> Result<Vec<Rename>, ClipError> {
    let target = free_name(path, stamp)?;
    let mut renames = Vec::new();
    for suffix in SUFFIXES {
        let from = suffixed(path, suffix);
        if is_taken(&from)? {
            renames.push(Rename {
                from,
                to: suffixed(&target, suffix),
            });
        }
    }
    Ok(renames)
}

fn free_name(path: &Path, stamp: u128) -> Result<PathBuf, ClipError> {
    for attempt in 0..MAX_ASIDE_ATTEMPTS {
        let candidate = suffixed(path, &aside_suffix(stamp, attempt));
        if !any_taken(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(io::Error::from(io::ErrorKind::AlreadyExists).into())
}

/// The only aside names Navo makes; [`is_aside_tail`] accepts exactly these.
fn aside_suffix(stamp: u128, attempt: u32) -> String {
    match attempt {
        0 => format!("{ASIDE_MARK}{stamp}"),
        _ => format!("{ASIDE_MARK}{stamp}-{attempt}"),
    }
}

fn any_taken(base: &Path) -> io::Result<bool> {
    for suffix in SUFFIXES {
        if is_taken(&suffixed(base, suffix))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Anything at `path` counts, a dangling symlink too.
fn is_taken(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Moves `from` to `to` and fails with `AlreadyExists` when anything is at `to`.
pub(in crate::store) fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    match fs::hard_link(from, to) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Err(error),
        // A folder without hard links (FAT, some shares) falls back to a checked rename.
        Err(_) => return rename_if_free(from, to),
    }
    if let Err(error) = fs::remove_file(from) {
        let _ = fs::remove_file(to);
        return Err(error);
    }
    Ok(())
}

pub(in crate::store) fn rename_if_free(from: &Path, to: &Path) -> io::Result<()> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    fs::rename(from, to).inspect_err(|_| {
        let _ = fs::remove_file(to);
    })
}

fn aside_of(path: &Path, renames: &[Rename]) -> PathBuf {
    renames
        .iter()
        .find(|rename| rename.from == path)
        .map_or_else(|| path.to_path_buf(), |rename| rename.to.clone())
}

/// Which live paths are free before a reopen, in [`SUFFIXES`] order; a rollback removes
/// only these. A path that cannot be checked counts as taken.
pub(in crate::store) fn absent_live(path: &Path) -> [bool; 3] {
    SUFFIXES.map(|suffix| matches!(is_taken(&suffixed(path, suffix)), Ok(false)))
}

/// Removes the live files the failed reopen made and moves the damaged ones back.
/// Every step runs; the first failure is returned.
pub(in crate::store) fn restore(
    path: &Path,
    renames: &[Rename],
    absent: &[bool; 3],
) -> io::Result<()> {
    let mut first = None;
    for (suffix, was_absent) in SUFFIXES.iter().zip(absent) {
        if *was_absent {
            keep_first(&mut first, remove_if_present(&suffixed(path, suffix)));
        }
    }
    for rename in renames.iter().rev() {
        keep_first(&mut first, move_no_replace(&rename.to, &rename.from));
    }
    sync_parent(path);
    first.map_or(Ok(()), Err)
}

fn keep_first(first: &mut Option<io::Error>, result: io::Result<()>) {
    if let Err(error) = result {
        first.get_or_insert(error);
    }
}

/// Files beside `path` named exactly as [`move_aside`] names them, with `-wal` or `-shm`.
fn find_asides(path: &Path) -> io::Result<Vec<PathBuf>> {
    let Some(file) = path.file_name() else {
        return Ok(Vec::new());
    };
    let prefix = file.as_encoded_bytes();
    let folder = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let mut asides = Vec::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let name = entry.file_name();
        let tail = name.as_encoded_bytes().strip_prefix(prefix);
        if tail.is_some_and(is_aside_tail) {
            asides.push(entry.path());
        }
    }
    Ok(asides)
}

/// `tail` is a file name after the database's own name; it must be one [`aside_suffix`]
/// makes, optionally followed by `-wal` or `-shm`.
fn is_aside_tail(tail: &[u8]) -> bool {
    let Ok(tail) = std::str::from_utf8(tail) else {
        return false;
    };
    let base = SUFFIXES
        .iter()
        .filter(|suffix| !suffix.is_empty())
        .find_map(|suffix| tail.strip_suffix(suffix))
        .unwrap_or(tail);
    let Some(numbers) = base.strip_prefix(ASIDE_MARK) else {
        return false;
    };
    let (stamp, attempt) = match numbers.split_once('-') {
        Some((stamp, attempt)) => (stamp, attempt.parse::<u32>().ok()),
        None => (numbers, Some(0)),
    };
    match (stamp.parse::<u128>(), attempt) {
        (Ok(stamp), Some(attempt)) if attempt < MAX_ASIDE_ATTEMPTS => {
            aside_suffix(stamp, attempt) == base
        }
        _ => false,
    }
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis())
}

#[cfg(unix)]
fn sync_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        // Best effort: the renames already happened, and a failed sync must not undo them.
        let _ = fs::File::open(parent).and_then(|folder| folder.sync_all());
    }
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) {}
