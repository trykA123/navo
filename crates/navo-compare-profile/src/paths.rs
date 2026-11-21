//! Profile folders: resolving the config and data directories from the
//! environment, and opening a profile after reading its version marker.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::brand;

/// The layout version written to `profile.toml`.
pub const PROFILE_VERSION: u32 = 1;

/// The marker file name inside the data folder.
const MARKER: &str = "profile.toml";

/// The data folders created beside the marker.
const DATA_FOLDERS: [&str; 5] = ["sessions", "cache", "journal", "logs", "crashes"];

/// Where a profile keeps settings and data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folders {
    /// Folder for settings files.
    pub config: PathBuf,
    /// Folder for the marker, sessions and caches.
    pub data: PathBuf,
}

/// An opened profile with its folder paths.
#[derive(Debug)]
pub struct Profile {
    folders: Folders,
}

/// Why a profile could not be found or opened.
#[derive(Debug)]
pub enum ProfileError {
    /// Neither a profile flag nor the needed environment variable is set.
    NoHome,
    /// A file operation failed at the given path.
    Io {
        /// Path of the failed operation.
        path: PathBuf,
        /// Error returned by the filesystem.
        error: std::io::Error,
    },
    /// The profile was written by a newer app.
    Newer {
        /// Version found in the marker.
        found: u32,
        /// Version this build supports.
        supported: u32,
    },
    /// The marker is not TOML, has no version, or has a version under 1.
    Broken {
        /// Path of the marker file.
        path: PathBuf,
    },
}

/// Resolves the config and data folders from the profile flag or the environment.
///
/// `explicit` puts both folders in one directory. `windows` picks the Windows
/// variables. `var` reads an environment variable; an empty value counts as
/// missing. Fails with `NoHome` when no usable variable is set.
pub fn resolve(
    explicit: Option<&Path>,
    windows: bool,
    var: &dyn Fn(&str) -> Option<String>,
) -> Result<Folders, ProfileError> {
    if let Some(dir) = explicit {
        return Ok(Folders {
            config: dir.to_path_buf(),
            data: dir.to_path_buf(),
        });
    }
    if windows {
        let roaming = required(var, "APPDATA")?;
        let local = required(var, "LOCALAPPDATA")?;
        return Ok(Folders {
            config: Path::new(&roaming).join(brand::FOLDER),
            data: Path::new(&local).join(brand::FOLDER),
        });
    }
    let home = || required(var, "HOME").map(PathBuf::from);
    let config_base = match optional(var, "XDG_CONFIG_HOME") {
        Some(value) => PathBuf::from(value),
        None => home()?.join(".config"),
    };
    let data_base = match optional(var, "XDG_DATA_HOME") {
        Some(value) => PathBuf::from(value),
        None => home()?.join(".local").join("share"),
    };
    Ok(Folders {
        config: config_base.join(brand::SLUG),
        data: data_base.join(brand::SLUG),
    })
}

/// Resolves the profile folders (from `explicit`, or the OS defaults) and opens the profile.
/// Fails with the same `ProfileError` that `resolve` or `Profile::open` gives.
pub fn open_profile(explicit: Option<&Path>) -> Result<Profile, ProfileError> {
    resolve(explicit, cfg!(windows), &|name: &str| {
        std::env::var(name).ok()
    })
    .and_then(Profile::open)
}

impl Profile {
    /// Opens the profile, reading the marker before creating anything.
    ///
    /// A missing marker creates both folders and the five data folders, then
    /// writes `version = 1`. A same-version marker recreates missing data
    /// folders only. A newer marker gives `Newer`; broken TOML gives `Broken`;
    /// both leave the file untouched. Fails with `Io` on filesystem errors.
    pub fn open(folders: Folders) -> Result<Profile, ProfileError> {
        let marker = folders.data.join(MARKER);
        match read_marker(&marker)? {
            None => {
                create_dir_all(&folders.config)?;
                create_data_dirs(&folders.data)?;
                write_marker(&marker)?;
            }
            Some(found) if found > PROFILE_VERSION => {
                return Err(ProfileError::Newer {
                    found,
                    supported: PROFILE_VERSION,
                });
            }
            Some(found) if found < 1 => return Err(ProfileError::Broken { path: marker }),
            Some(_) => {
                create_dir_all(&folders.config)?;
                create_data_dirs(&folders.data)?;
            }
        }
        Ok(Profile { folders })
    }

    /// Returns the settings file path, `config/settings.toml`.
    pub fn settings_file(&self) -> PathBuf {
        self.folders.config.join("settings.toml")
    }

    /// Returns the saved sessions folder, `data/sessions`.
    pub fn sessions_dir(&self) -> PathBuf {
        self.folders.data.join("sessions")
    }

    /// Returns the cache folder, `data/cache`.
    pub fn cache_dir(&self) -> PathBuf {
        self.folders.data.join("cache")
    }

    /// Returns the Viewed marks log, `data/viewed.log`.
    pub fn viewed_file(&self) -> PathBuf {
        self.folders.data.join("viewed.log")
    }

    /// Returns the journal folder, `data/journal`.
    pub fn journal_dir(&self) -> PathBuf {
        self.folders.data.join("journal")
    }

    /// Returns the log folder, `data/logs`.
    pub fn logs_dir(&self) -> PathBuf {
        self.folders.data.join("logs")
    }

    /// Returns the crash report folder, `data/crashes`.
    pub fn crashes_dir(&self) -> PathBuf {
        self.folders.data.join("crashes")
    }

    /// Returns the update check state file, `data/update-check.toml`.
    pub fn update_state_file(&self) -> PathBuf {
        self.folders.data.join("update-check.toml")
    }
}

/// Reads the marker version, or `None` when the file does not exist.
///
/// Fails with `Broken` when the bytes are not UTF-8 TOML or carry no version,
/// and with `Io` for other read errors.
fn read_marker(path: &Path) -> Result<Option<u32>, ProfileError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(path, error)),
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| ProfileError::Broken {
        path: path.to_path_buf(),
    })?;
    let marker: Marker = toml::from_str(text).map_err(|_| ProfileError::Broken {
        path: path.to_path_buf(),
    })?;
    Ok(Some(marker.version))
}

/// Writes the marker to a per-process temporary file, syncs it, then renames
/// it into place.
fn write_marker(path: &Path) -> Result<(), ProfileError> {
    let temp = path.with_file_name(format!("{MARKER}.{}.tmp", std::process::id()));
    let text = format!("version = {PROFILE_VERSION}\n");
    let mut file = File::create(&temp).map_err(|error| io(&temp, error))?;
    file.write_all(text.as_bytes())
        .map_err(|error| io(&temp, error))?;
    file.sync_all().map_err(|error| io(&temp, error))?;
    drop(file);
    fs::rename(&temp, path).map_err(|error| io(path, error))
}

/// Creates `dir` and the five data folders under it.
fn create_data_dirs(dir: &Path) -> Result<(), ProfileError> {
    create_dir_all(dir)?;
    for name in DATA_FOLDERS {
        create_dir_all(&dir.join(name))?;
    }
    Ok(())
}

/// Creates one folder and its parents, naming the folder in the error.
fn create_dir_all(dir: &Path) -> Result<(), ProfileError> {
    fs::create_dir_all(dir).map_err(|error| io(dir, error))
}

/// Wraps an I/O error with the path it happened on.
fn io(path: &Path, error: std::io::Error) -> ProfileError {
    ProfileError::Io {
        path: path.to_path_buf(),
        error,
    }
}

/// Reads a required variable, an empty value counting as missing.
fn required(var: &dyn Fn(&str) -> Option<String>, name: &str) -> Result<String, ProfileError> {
    optional(var, name).ok_or(ProfileError::NoHome)
}

/// Reads a variable, returning `None` when it is unset or empty.
fn optional(var: &dyn Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    var(name).filter(|value| !value.is_empty())
}

/// The parsed marker file; unknown keys are ignored.
#[derive(Deserialize)]
struct Marker {
    /// Layout version of the profile.
    version: u32,
}
