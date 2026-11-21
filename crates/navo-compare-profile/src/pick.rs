//! The Explorer pick store: one selected path, written and consumed once.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Version of the pick file layout.
pub const PICK_VERSION: u32 = 1;

/// Largest pick file read by `take`; bigger files are discarded unread.
pub const MAX_PICK_BYTES: u64 = 64 * 1024;

/// Name of the pick file inside the profile folder.
const FILE_NAME: &str = "explorer-pick.toml";

/// Name of the temporary file used while saving the pick file.
const TEMP_NAME: &str = "explorer-pick.toml.tmp";

/// A store holding the one path the Explorer menu selected.
pub struct PickStore {
    dir: PathBuf,
}

/// Why a pick could not be saved or taken.
#[derive(Debug)]
pub enum PickError {
    /// A file operation failed at the given path.
    Io {
        /// Path of the failed operation.
        path: PathBuf,
        /// Error returned by the filesystem.
        error: std::io::Error,
    },
    /// The path to save is relative.
    NotAbsolute,
    /// The path to save cannot be written as Unicode text.
    NotUnicode,
}

/// The saved document: one layout version and the selected path.
#[derive(Deserialize, Serialize)]
struct PickFile {
    /// Layout version of the file.
    version: u32,
    /// Absolute path selected in the Explorer menu.
    left: String,
}

impl PickStore {
    /// Opens the store at `dir` without touching the disk.
    pub fn open(dir: PathBuf) -> PickStore {
        PickStore { dir }
    }

    /// Returns `<dir>/explorer-pick.toml`.
    pub fn file(&self) -> PathBuf {
        self.dir.join(FILE_NAME)
    }

    /// Stores `left` as the next pick, replacing the previous one.
    ///
    /// Creates the folder and writes the TOML through a temporary file. Fails
    /// with `NotAbsolute` or `NotUnicode` before touching the disk, and `Io`
    /// when creating, writing or renaming fails.
    pub fn save(&self, left: &Path) -> Result<(), PickError> {
        if !left.is_absolute() {
            return Err(PickError::NotAbsolute);
        }
        let left = left.to_str().ok_or(PickError::NotUnicode)?.to_string();
        let path = self.file();
        let text = toml::to_string(&PickFile {
            version: PICK_VERSION,
            left,
        })
        .map_err(|error| PickError::Io {
            path: path.clone(),
            error: std::io::Error::other(error),
        })?;
        fs::create_dir_all(&self.dir).map_err(|error| PickError::Io {
            path: self.dir.clone(),
            error,
        })?;
        let temp = self.dir.join(TEMP_NAME);
        self.write_temp(&temp, text.as_bytes())?;
        fs::rename(&temp, &path).map_err(|error| PickError::Io { path, error })
    }

    /// Removes and returns the saved path, or `None` when there is none.
    ///
    /// A missing file gives `None`. A file over `MAX_PICK_BYTES` is removed
    /// unread. Unparsable content, another version or a relative path is
    /// removed and gives `None`. Fails with `Io` on other file errors.
    pub fn take(&self) -> Result<Option<PathBuf>, PickError> {
        let path = self.file();
        let size = match fs::metadata(&path) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(PickError::Io { path, error }),
        };
        if size > MAX_PICK_BYTES {
            self.remove(&path)?;
            return Ok(None);
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(PickError::Io { path, error }),
        };
        self.remove(&path)?;
        Ok(valid_pick(&bytes))
    }

    /// Writes `bytes` to `path`, flushes it and syncs it to disk.
    fn write_temp(&self, path: &Path, bytes: &[u8]) -> Result<(), PickError> {
        let mut file = File::create(path).map_err(|error| PickError::Io {
            path: path.to_path_buf(),
            error,
        })?;
        file.write_all(bytes).map_err(|error| PickError::Io {
            path: path.to_path_buf(),
            error,
        })?;
        file.flush().map_err(|error| PickError::Io {
            path: path.to_path_buf(),
            error,
        })?;
        file.sync_all().map_err(|error| PickError::Io {
            path: path.to_path_buf(),
            error,
        })
    }

    /// Removes `path`, treating a missing file as already removed.
    fn remove(&self, path: &Path) -> Result<(), PickError> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(PickError::Io {
                path: path.to_path_buf(),
                error,
            }),
        }
    }
}

/// Returns the absolute path in `bytes`, or `None` when the document is not a
/// current pick file with an absolute `left` path.
fn valid_pick(bytes: &[u8]) -> Option<PathBuf> {
    let text = std::str::from_utf8(bytes).ok()?;
    let pick: PickFile = toml::from_str(text).ok()?;
    if pick.version != PICK_VERSION {
        return None;
    }
    let left = PathBuf::from(pick.left);
    left.is_absolute().then_some(left)
}

impl std::fmt::Display for PickError {
    /// Describes the failure in one sentence for the user.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PickError::Io { path, error } => {
                write!(
                    formatter,
                    "could not use the pick file at {}: {error}",
                    path.display()
                )
            }
            PickError::NotAbsolute => formatter.write_str("the picked path must be absolute"),
            PickError::NotUnicode => formatter.write_str("the picked path is not valid Unicode"),
        }
    }
}

impl std::error::Error for PickError {
    /// Returns the filesystem error behind an `Io` failure.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PickError::Io { error, .. } => Some(error),
            PickError::NotAbsolute | PickError::NotUnicode => None,
        }
    }
}
