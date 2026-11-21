//! A Viewed marks store backed by an append-only log file, so marks survive a restart.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use super::viewed::{ViewedError, ViewedMarks};

/// Bytes per log record: one flag byte and the 64-byte content key.
const RECORD_LEN: usize = 65;

/// Viewed marks kept in memory and in an append-only log file, so they survive a restart.
pub struct FileViewed {
    path: PathBuf,
    state: Mutex<State>,
}

struct State {
    marks: HashSet<[u8; 64]>,
    log: Option<std::fs::File>, // opened for append on the first write
    records: usize,             // records in the log file now
}

impl FileViewed {
    /// Opens the log at `path` (created on first write), replays it, and compacts it when it
    /// holds more than twice the live marks plus 1024 records. Fails on a read or write error.
    pub fn open(path: &Path) -> Result<Self, ViewedError> {
        let (marks, records, dirty) = match fs::read(path) {
            Ok(bytes) => replay(&bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (HashSet::new(), 0, false),
            Err(error) => return Err(log_error(path, error)),
        };
        let store = Self {
            path: path.to_path_buf(),
            state: Mutex::new(State {
                marks,
                log: None,
                records,
            }),
        };
        let over_limit = {
            let state = store.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.records > state.marks.len() * 2 + 1024
        };
        if dirty || over_limit {
            store.compact()?;
        }
        Ok(store)
    }

    /// Removes every mark, in memory and on disk; returns how many marks were removed.
    pub fn clear_all(&self) -> Result<usize, ViewedError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.log = None;
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(log_error(&self.path, error)),
        }
        let removed = state.marks.len();
        state.marks.clear();
        state.records = 0;
        Ok(removed)
    }

    /// How many files are marked viewed now.
    pub fn len(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .marks
            .len()
    }

    /// True when no file is marked viewed.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Writes the live marks to `<path>.tmp`, syncs it, and renames it over the log.
    fn compact(&self) -> Result<(), ViewedError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let temp = temp_path(&self.path);
        let result = write_all_marks(&temp, &state.marks).and_then(|file| file.sync_all());
        if let Err(error) = result {
            let _ = fs::remove_file(&temp);
            return Err(log_error(&temp, error));
        }
        fs::rename(&temp, &self.path).map_err(|error| log_error(&self.path, error))?;
        state.records = state.marks.len();
        state.log = None;
        Ok(())
    }
}

impl ViewedMarks for FileViewed {
    fn get(&self, key: &[u8; 64]) -> Result<bool, ViewedError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .marks
            .contains(key))
    }

    fn set(&self, key: &[u8; 64], viewed: bool) -> Result<(), ViewedError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.marks.contains(key) == viewed {
            return Ok(());
        }
        if state.log.is_none() {
            create_parent(&self.path)?;
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .map_err(|error| log_error(&self.path, error))?;
            state.log = Some(file);
        }
        let mut record = [0u8; RECORD_LEN];
        record[0] = u8::from(viewed);
        record[1..].copy_from_slice(key);
        let log = match state.log.as_mut() {
            Some(log) => log,
            None => {
                return Err(ViewedError::new(format!(
                    "viewed log {}: handle not open",
                    self.path.display()
                )));
            }
        };
        log.write_all(&record)
            .and_then(|()| log.flush())
            .map_err(|error| log_error(&self.path, error))?;
        if viewed {
            state.marks.insert(*key);
        } else {
            state.marks.remove(key);
        }
        state.records += 1;
        Ok(())
    }

    fn clear_all(&self) -> Result<usize, ViewedError> {
        FileViewed::clear_all(self)
    }
}

/// Replays log bytes into marks, returning the marks, record count and whether a
/// partial or bad record forces a compaction.
fn replay(bytes: &[u8]) -> (HashSet<[u8; 64]>, usize, bool) {
    let mut marks = HashSet::new();
    let mut records = 0;
    let mut dirty = !bytes.len().is_multiple_of(RECORD_LEN);
    for record in bytes.as_chunks::<RECORD_LEN>().0 {
        let mut key = [0u8; 64];
        key.copy_from_slice(&record[1..]);
        match record[0] {
            1 => {
                marks.insert(key);
            }
            0 => {
                marks.remove(&key);
            }
            _ => {
                dirty = true;
                break;
            }
        }
        records += 1;
    }
    (marks, records, dirty)
}

/// Writes one `1` record per mark to `path`, truncating it, and returns the open file.
fn write_all_marks(path: &Path, marks: &HashSet<[u8; 64]>) -> io::Result<std::fs::File> {
    let mut file = std::fs::File::create(path)?;
    for key in marks {
        let mut record = [0u8; RECORD_LEN];
        record[0] = 1;
        record[1..].copy_from_slice(key);
        file.write_all(&record)?;
    }
    Ok(file)
}

/// Returns `<path>.tmp`, the compaction target beside the log.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

/// Creates the log's parent folder, unless the log sits in the current folder.
fn create_parent(path: &Path) -> Result<(), ViewedError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(parent).map_err(|error| log_error(parent, error))
}

/// Maps an I/O error to a Viewed error that names the log path.
fn log_error(path: &Path, error: io::Error) -> ViewedError {
    ViewedError::new(format!("viewed log {}: {error}", path.display()))
}

#[cfg(test)]
#[path = "viewed_file_tests.rs"]
mod tests;
