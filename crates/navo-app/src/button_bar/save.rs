//! Atomic save, 50-step undo, and changed-on-disk guard for `buttonbar.toml`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::{fs, io};

use navo_icons::io::{ReadError, read_capped};
use navo_ui::window_view::BarEdit;

use super::{FILE, MAX_FILE_BYTES, edit, model};

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;

const MAX_UNDO: usize = 50;
const CHANGED_OUTSIDE: &str = "buttonbar.toml changed outside Navo; reloaded";
const NOTHING_TO_UNDO: &str = "nothing to undo";

/// Persists `buttonbar.toml` edits atomically with a 50-step undo stack and outside-change guard.
pub(crate) struct Saver {
    path: PathBuf,
    last: Mutex<String>,
    undo: Mutex<Vec<String>>,
}

impl Saver {
    /// Creates a saver tracking `buttonbar.toml` at `path`.
    #[must_use]
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            last: Mutex::new(String::new()),
            undo: Mutex::new(Vec::new()),
        }
    }

    /// Records the last text successfully loaded from disk, clearing undo on external changes.
    pub(crate) fn record_loaded(&self, text: &str) {
        let mut last = lock(&self.last);
        if *last != text {
            *last = text.to_owned();
            lock(&self.undo).clear();
        }
    }

    /// Applies `edit` to `buttonbar.toml`, validates it, writes atomically, and calls `reload`.
    pub(crate) fn edit(&self, edit: &BarEdit, reload: &dyn Fn()) -> Result<(), String> {
        let current = self.read_unchanged(reload)?;
        let next = edit::apply(&current, edit).map_err(|err| err.to_string())?;
        if let Err(errors) = model::parse(&next, None) {
            let message = errors
                .first()
                .map_or_else(|| format!("{FILE}: invalid"), ToString::to_string);
            return Err(message);
        }
        write_file(&self.path, &next)?;
        {
            let mut last = lock(&self.last);
            let mut undo = lock(&self.undo);
            if undo.len() >= MAX_UNDO {
                undo.remove(0);
            }
            undo.push(current);
            *last = next;
        }
        reload();
        Ok(())
    }

    /// Restores the previous `buttonbar.toml` text from the undo stack and calls `reload`.
    pub(crate) fn undo(&self, reload: &dyn Fn()) -> Result<(), String> {
        let _ = self.read_unchanged(reload)?;
        let Some(prev) = lock(&self.undo).last().cloned() else {
            return Err(NOTHING_TO_UNDO.to_owned());
        };
        write_file(&self.path, &prev)?;
        {
            let mut last = lock(&self.last);
            let mut undo = lock(&self.undo);
            let _ = undo.pop();
            *last = prev;
        }
        reload();
        Ok(())
    }

    fn read_unchanged(&self, reload: &dyn Fn()) -> Result<String, String> {
        let current = read_current(&self.path)?;
        let changed = {
            let last = lock(&self.last);
            if current == *last {
                false
            } else {
                lock(&self.undo).clear();
                true
            }
        };
        if changed {
            reload();
            return Err(CHANGED_OUTSIDE.to_owned());
        }
        Ok(current)
    }
}

fn read_current(path: &Path) -> Result<String, String> {
    let bytes = match read_capped(path, MAX_FILE_BYTES) {
        Ok(bytes) => bytes,
        Err(ReadError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(String::new());
        }
        Err(ReadError::TooLarge) => return Err(format!("{FILE}: file exceeds 256 KiB")),
        Err(ReadError::Io(error)) => return Err(format!("{FILE}: {error}")),
    };
    String::from_utf8(bytes).map_err(|error| format!("{FILE}: {error}"))
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|error| format!("{FILE}: {error}"))?;
    }
    navo_store::write_atomic(path, text.as_bytes()).map_err(|error| format!("{FILE}: {error}"))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
