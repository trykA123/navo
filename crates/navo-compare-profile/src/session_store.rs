//! The sessions folder: one TOML file per session, version-checked reads and
//! atomic writes. Implements NS5, NS9 and the B4 store contract.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::session::{
    RECENT_KEPT, SESSION_VERSION, Session, SessionKind, filter_valid, is_valid_id, rules_valid,
    sides_valid,
};

/// A sessions folder holding one TOML file per saved session.
pub struct SessionStore {
    dir: PathBuf,
}

/// Why a session could not be stored, read or removed.
#[derive(Debug)]
pub enum StoreError {
    /// A file operation failed at the given path.
    Io {
        /// Path of the failed operation.
        path: PathBuf,
        /// Error returned by the filesystem.
        error: std::io::Error,
    },
    /// The id does not match NS5.
    BadId(String),
    /// A side, rule or filter value cannot be stored.
    BadSide,
    /// A "Two texts" session is never written to disk (NC26).
    NotStorable,
    /// The file was written by a newer app and is left untouched.
    Newer {
        /// Id of the session.
        id: String,
        /// Version found in the file.
        found: u32,
    },
    /// The file does not parse, has no version, or names another id.
    Broken {
        /// Id of the session.
        id: String,
    },
}

impl SessionStore {
    /// Opens the store at `dir` without touching the disk.
    pub fn open(dir: PathBuf) -> SessionStore {
        SessionStore { dir }
    }

    /// Builds a unique id: the time in hex, a dash, then a counter.
    ///
    /// `now_ms` is Unix time in milliseconds. The counter grows until no
    /// `<id>.toml` file is in the folder. Never fails.
    pub fn new_id(&self, now_ms: u64) -> String {
        let stamp = format!("{now_ms:x}");
        let mut counter = 0u64;
        loop {
            let id = format!("{stamp}-{counter:x}");
            if !self.session_file(&id).exists() {
                return id;
            }
            counter = counter.wrapping_add(1);
        }
    }

    /// Saves `session` as `<id>.toml`, replacing a same-version file.
    ///
    /// Fails with `BadId`, `BadSide` or `NotStorable` before touching the disk.
    /// An existing newer, broken or unreadable file refuses the save with that
    /// error and keeps its bytes. Filesystem failures give `Io`.
    pub fn save(&self, session: &Session) -> Result<(), StoreError> {
        if !is_valid_id(&session.id) {
            return Err(StoreError::BadId(session.id.clone()));
        }
        if session.kind == SessionKind::Texts {
            return Err(StoreError::NotStorable);
        }
        if !sides_valid(session) || !rules_valid(&session.rules) || !filter_valid(&session.filter) {
            return Err(StoreError::BadSide);
        }
        let _existing = self.read_existing(&session.id)?;
        let path = self.session_file(&session.id);
        let mut stored = session.clone();
        stored.version = SESSION_VERSION;
        let text = toml::to_string(&stored).map_err(|error| StoreError::Io {
            path: path.clone(),
            error: std::io::Error::other(error),
        })?;
        fs::create_dir_all(&self.dir).map_err(|error| StoreError::Io {
            path: self.dir.clone(),
            error,
        })?;
        let temp = self.temp_file(&session.id);
        self.write_temp(&temp, text.as_bytes())?;
        fs::rename(&temp, &path).map_err(|error| StoreError::Io { path, error })
    }

    /// Loads the session with `id`.
    ///
    /// Fails with `BadId` for an invalid id, `Newer` for a higher version and
    /// `Broken` when the file does not parse, has no version, or names another
    /// id. A missing file gives `Io`.
    pub fn load(&self, id: &str) -> Result<Session, StoreError> {
        if !is_valid_id(id) {
            return Err(StoreError::BadId(id.to_string()));
        }
        match self.read_existing(id)? {
            Some(session) => Ok(session),
            None => Err(StoreError::Io {
                path: self.session_file(id),
                error: std::io::Error::new(std::io::ErrorKind::NotFound, "session file is missing"),
            }),
        }
    }

    /// Lists the readable sessions, newest `opened` first and ties by id.
    ///
    /// Skips broken and newer files and every name that is not `<id>.toml`.
    /// A missing folder lists nothing; other read errors give `Io`.
    pub fn list(&self) -> Result<Vec<Session>, StoreError> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(StoreError::Io {
                    path: self.dir.clone(),
                    error,
                });
            }
        };
        let mut sessions = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| StoreError::Io {
                path: self.dir.clone(),
                error,
            })?;
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_file() {
                continue;
            }
            let Some(id) = file_id(&entry.file_name()) else {
                continue;
            };
            match self.read_existing(&id) {
                Ok(Some(session)) => sessions.push(session),
                Ok(None) => {}
                Err(StoreError::Broken { .. } | StoreError::Newer { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        sessions.sort_by(|left, right| {
            right
                .opened
                .cmp(&left.opened)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(sessions)
    }

    /// Deletes the session file for `id`, treating a missing file as done.
    ///
    /// Fails with `BadId` for an invalid id and `Io` when the file cannot be
    /// removed for another reason.
    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        if !is_valid_id(id) {
            return Err(StoreError::BadId(id.to_string()));
        }
        let path = self.session_file(id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(StoreError::Io { path, error }),
        }
    }

    /// Deletes unnamed sessions beyond the newest `RECENT_KEPT`.
    ///
    /// Named sessions are never pruned. Returns how many files were removed;
    /// fails with `Io` when listing or deleting fails.
    pub fn prune_recent(&self) -> Result<usize, StoreError> {
        let unnamed: Vec<Session> = self
            .list()?
            .into_iter()
            .filter(|session| session.name.is_none())
            .collect();
        let mut pruned = 0;
        for session in unnamed.into_iter().skip(RECENT_KEPT) {
            self.delete(&session.id)?;
            pruned += 1;
        }
        Ok(pruned)
    }

    /// Returns `<dir>/<id>.toml`.
    fn session_file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.toml"))
    }

    /// Returns `<dir>/<id>.toml.tmp`.
    fn temp_file(&self, id: &str) -> PathBuf {
        self.dir
            .join(format!("{id}.toml.{}.tmp", std::process::id()))
    }

    /// Writes `bytes` to `path` and syncs the file to disk.
    fn write_temp(&self, path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
        let mut file = File::create(path).map_err(|error| StoreError::Io {
            path: path.to_path_buf(),
            error,
        })?;
        file.write_all(bytes).map_err(|error| StoreError::Io {
            path: path.to_path_buf(),
            error,
        })?;
        file.sync_all().map_err(|error| StoreError::Io {
            path: path.to_path_buf(),
            error,
        })
    }

    /// Reads and validates a session file, or returns `None` when it is absent.
    ///
    /// The version is read first, so a newer file gives `Newer` before the rest
    /// is parsed. Fails with `Broken` on bad TOML, a missing version, another
    /// inner id, a side that cannot be stored, or an unknown whitespace or
    /// filter value.
    fn read_existing(&self, id: &str) -> Result<Option<Session>, StoreError> {
        let path = self.session_file(id);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(StoreError::Io { path, error }),
        };
        let text =
            std::str::from_utf8(&bytes).map_err(|_| StoreError::Broken { id: id.to_string() })?;
        let found = read_version(text).map_err(|()| StoreError::Broken { id: id.to_string() })?;
        if found > SESSION_VERSION {
            return Err(StoreError::Newer {
                id: id.to_string(),
                found,
            });
        }
        let session: Session =
            toml::from_str(text).map_err(|_| StoreError::Broken { id: id.to_string() })?;
        if session.id != id
            || !sides_valid(&session)
            || !rules_valid(&session.rules)
            || !filter_valid(&session.filter)
        {
            return Err(StoreError::Broken { id: id.to_string() });
        }
        Ok(Some(session))
    }
}

/// Returns the id of a `<valid id>.toml` file name, or `None` for any other name.
fn file_id(name: &OsStr) -> Option<String> {
    let text = name.to_str()?;
    let id = text.strip_suffix(".toml")?;
    is_valid_id(id).then(|| id.to_string())
}

/// Reads only the `version` field of a session document.
fn read_version(text: &str) -> Result<u32, ()> {
    toml::from_str::<VersionOnly>(text)
        .map(|marker| marker.version)
        .map_err(|_| ())
}

/// The version-only view of a session file; every other field is ignored.
#[derive(Deserialize)]
struct VersionOnly {
    /// Layout version of the session.
    version: u32,
}
