//! Temp files of atomic writes; the start-up sweep covers only the top level of the config and data folders.

use std::fs::{self, DirEntry, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use super::{Op, step};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const TEMP_MARK: &str = ".tmp-";
pub(super) const BACKUP_MARK: &str = ".bak-tmp-";
pub(super) const ORPHAN_AGE: Duration = Duration::from_secs(60 * 60);

/// A temp file next to its target; dropping it removes whatever is left at its name, also on panic.
pub(super) struct TempFile {
    path: PathBuf,
}

impl TempFile {
    pub(super) fn reserve(target: &Path, mark: &str) -> Self {
        Self {
            path: temp_path(target, mark),
        }
    }

    pub(super) fn create(target: &Path) -> io::Result<(Self, File)> {
        let path = temp_path(target, TEMP_MARK);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok((Self { path }, file))
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn commit(self, target: &Path, op: Op) -> io::Result<()> {
        step(op)?;
        navo_fs::native_replace(&self.path, target)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // POSIX rename between two links of one file succeeds and keeps both names.
        let _ = fs::remove_file(&self.path);
    }
}

fn temp_path(path: &Path, mark: &str) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}{mark}{}-{n}", std::process::id()))
}

/// The process id in a temp name this module makes (`.<name>.tmp-<pid>-<n>`), if it is one.
pub(super) fn temp_owner(name: &str) -> Option<u32> {
    owner(name, TEMP_MARK)
}

/// The process id in a staged backup name (`.<name>.bak-tmp-<pid>-<n>`), if it is one.
pub(super) fn staged_owner(name: &str) -> Option<u32> {
    owner(name, BACKUP_MARK)
}

fn owner(name: &str, mark: &str) -> Option<u32> {
    let (base, tail) = name.strip_prefix('.')?.rsplit_once(mark)?;
    let (pid, n) = tail.split_once('-')?;
    let valid = !base.is_empty() && is_number(pid) && is_number(n);
    valid.then(|| pid.parse().ok()).flatten()
}

fn is_number(text: &str) -> bool {
    let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    digits && (text == "0" || !text.starts_with('0'))
}

/// Removes temp files that interrupted atomic writes of other processes left in `folder`.
/// Only plain files with this module's names go: temps older than an hour, and staged backups
/// of any age, since those hard links carry the old file's age. Run it under the instance guard.
pub fn remove_orphan_temps(folder: &Path) -> io::Result<usize> {
    let entries = match fs::read_dir(folder) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        entries => entries?,
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten().filter(|entry| is_orphan(entry, now)) {
        match fs::remove_file(entry.path()) {
            Ok(()) => removed += 1,
            Err(error) => tracing::warn!(%error, "an orphan temp file could not be removed"),
        }
    }
    Ok(removed)
}

fn is_orphan(entry: &DirEntry, now: SystemTime) -> bool {
    let file_name = entry.file_name();
    let Some(name) = file_name.to_str() else {
        return false;
    };
    let Ok(metadata) = entry.metadata() else {
        return false;
    };
    let other = |owner: u32| owner != std::process::id();
    if !metadata.is_file() {
        return false;
    }
    if let Some(owner) = staged_owner(name) {
        return other(owner);
    }
    let age = metadata
        .modified()
        .ok()
        .and_then(|modified| now.duration_since(modified).ok());
    temp_owner(name).is_some_and(other) && age.is_some_and(|age| age >= ORPHAN_AGE)
}
