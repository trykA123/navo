use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rusty_store::StoreDirs;
use serde::{Deserialize, Serialize};

use super::fs::MigrationFs;
use super::layout::{CORRUPT, free_name, with_suffix};

const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Phase {
    Detached,
    Staged,
    Committed,
    Retired,
}

impl FromStr for Phase {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, ()> {
        match text {
            "detached" => Ok(Self::Detached),
            "staged" => Ok(Self::Staged),
            "committed" => Ok(Self::Committed),
            "retired" => Ok(Self::Retired),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Dirs {
    config: PathBuf,
    data: PathBuf,
}

impl From<&StoreDirs> for Dirs {
    fn from(dirs: &StoreDirs) -> Self {
        Self {
            config: dirs.config.clone(),
            data: dirs.data.clone(),
        }
    }
}

impl From<&Dirs> for StoreDirs {
    fn from(dirs: &Dirs) -> Self {
        Self {
            config: dirs.config.clone(),
            data: dirs.data.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Record {
    version: u32,
    pub(super) phase: Phase,
    pub(super) old: Dirs,
    pub(super) new: Dirs,
}

impl Record {
    pub(super) fn new(phase: Phase, old: &StoreDirs, new: &StoreDirs) -> Self {
        Self {
            version: VERSION,
            phase,
            old: old.into(),
            new: new.into(),
        }
    }

    pub(super) fn at(&self, phase: Phase) -> Self {
        Self {
            phase,
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Ending {
    Migrated,
    StartedFresh,
}

#[derive(Serialize, Deserialize)]
struct Marker {
    version: u32,
    outcome: Ending,
}

pub(super) fn read(fs: &impl MigrationFs, path: &Path) -> io::Result<Option<Record>> {
    let bytes = match fs.read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if record.version != VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown journal version {}", record.version),
        ));
    }
    Ok(Some(record))
}

pub(super) fn set_aside(fs: &impl MigrationFs, path: &Path) -> io::Result<()> {
    fs.rename(path, &free_name(fs, &with_suffix(path, CORRUPT))?)?;
    fs.sync_dir(folder_of(path))
}

pub(super) fn write(fs: &impl MigrationFs, path: &Path, record: &Record) -> io::Result<()> {
    write_durable(fs, path, &serde_json::to_vec_pretty(record)?)
}

pub(super) fn remove(fs: &impl MigrationFs, path: &Path) -> io::Result<()> {
    for file in [temp_of(path), path.to_owned()] {
        if fs.exists(&file) {
            fs.remove_file(&file)?;
        }
    }
    fs.sync_dir(folder_of(path))
}

pub(super) fn read_marker(fs: &impl MigrationFs, path: &Path) -> Option<Ending> {
    if !fs.exists(path) {
        return None;
    }
    let ending = fs
        .read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Marker>(&bytes).ok())
        .map(|marker| marker.outcome);
    Some(ending.unwrap_or(Ending::Migrated))
}

pub(super) fn write_marker(fs: &impl MigrationFs, path: &Path, outcome: Ending) -> io::Result<()> {
    let marker = Marker {
        version: VERSION,
        outcome,
    };
    fs.create_dir(folder_of(path))?;
    write_durable(fs, path, &serde_json::to_vec_pretty(&marker)?)
}

fn write_durable(fs: &impl MigrationFs, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = temp_of(path);
    fs.write_file(&temp, bytes)?;
    fs.sync_file(&temp)?;
    fs.replace(&temp, path)?;
    fs.sync_dir(folder_of(path))
}

fn temp_of(path: &Path) -> PathBuf {
    with_suffix(path, ".tmp")
}

pub(super) fn folder_of(path: &Path) -> &Path {
    path.parent().unwrap_or(Path::new("."))
}
