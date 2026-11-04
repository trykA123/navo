//! Read-only, name-opaque access to the selected folder.
//!
//! Residual risk: a folder swapped for a junction after its final-path check is read through; reads stay read-only.

mod platform;
mod policy;
mod volume;
mod walk;

pub use volume::{DriveKind, FsClass, Volume};

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[derive(Clone)]
pub struct RecordId {
    path: Arc<PathBuf>,
    owner: Arc<()>,
}

impl fmt::Debug for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecordId(..)")
    }
}

impl PartialEq for RecordId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && Arc::ptr_eq(&self.path, &other.path)
    }
}

impl Eq for RecordId {}

impl std::hash::Hash for RecordId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(Arc::as_ptr(&self.owner), state);
        std::ptr::hash(Arc::as_ptr(&self.path), state);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
    SymbolicLink,
    Junction,
    ReparsePoint,
    Other,
}

#[derive(Clone)]
pub struct Record {
    pub id: RecordId,
    pub parent: Option<RecordId>,
    /// Direct children have depth zero.
    pub depth: usize,
    pub kind: EntryKind,
    pub size: u64,
    /// Native Windows attributes or Unix mode bits.
    pub attributes: u32,
    pub mtime: Option<SystemTime>,
    /// In-memory text; reporting must apply the D92 fold.
    pub extension: String,
    pub hard_links: Option<u64>,
    /// Characters in the entry name.
    pub name_chars: usize,
    /// Characters in the absolute path.
    pub path_chars: usize,
    /// The name starts with a dot.
    pub dot_name: bool,
    /// The entry is named `.git`.
    pub git_marker: bool,
}

impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Record")
            .field("depth", &self.depth)
            .field("kind", &self.kind)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WalkConfig {
    /// All entries, including directories, links and failed entries, consume this budget.
    pub max_files: u64,
    pub max_time: Duration,
}

impl Default for WalkConfig {
    fn default() -> Self {
        Self {
            max_files: 2_000_000,
            max_time: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    FileCount,
    Time,
    /// The worker did not finish within `max_time` plus a grace period and was abandoned.
    Timeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalkOutcome {
    Complete,
    StoppedAt { files: u64, reason: StopReason },
}

impl fmt::Display for WalkOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Complete => f.write_str("complete"),
            Self::StoppedAt { files, .. } => write!(f, "stopped at {files}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WalkSummary {
    pub files: u64,
    pub errors: u64,
    pub outcome: WalkOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SourceError {
    #[error("drive roots cannot be selected")]
    DriveRoot,
    #[error("the system drive root cannot be selected")]
    SystemDriveRoot,
    #[error("the Windows folder cannot be selected")]
    WindowsFolder,
    #[error("Program Files cannot be selected")]
    ProgramFiles,
    #[error("Program Files (x86) cannot be selected")]
    ProgramFilesX86,
    #[error("ProgramData cannot be selected")]
    ProgramData,
    #[error("the folder holding navo-bench cannot be selected")]
    ExeFolder,
    #[error("the run folder cannot be selected")]
    ScratchRoot,
    #[error("a user-profile root cannot be selected")]
    UserProfileRoot,
    #[error("UNC server roots cannot be selected")]
    UncServerRoot,
    #[error("system folder lookup failed")]
    FolderLookup,
    #[error("the selected folder is unavailable ({0:?})")]
    Unavailable(io::ErrorKind),
    #[error("the selected entry is not a folder")]
    NotDirectory,
    #[error("links and reparse points cannot be opened")]
    Link,
    #[error("the entry does not belong to this source")]
    ForeignId,
    #[error("the entry is not a regular file")]
    NotFile,
}

pub struct ReadOnlySource {
    root: PathBuf,
    _directory: File,
    owner: Arc<()>,
}

impl fmt::Debug for ReadOnlySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ReadOnlySource(..)")
    }
}

impl ReadOnlySource {
    /// Refuses `path` when it holds or lies in a system folder, the exe folder or `scratch_root`.
    pub fn open(path: &Path, scratch_root: &Path) -> Result<Self, SourceError> {
        Self::open_with(path, &policy::FolderPolicy::load(scratch_root)?)
    }

    fn open_with(path: &Path, policy: &policy::FolderPolicy) -> Result<Self, SourceError> {
        policy.check(path)?;
        platform::check_directory(path)?;
        let root = std::fs::canonicalize(path).map_err(unavailable)?;
        policy.check(&root)?;
        let directory = platform::open_directory(&root).map_err(unavailable)?;
        Ok(Self {
            root,
            _directory: directory,
            owner: Arc::new(()),
        })
    }

    /// The canonical folder, for building a read-only argument; never print it.
    pub(crate) fn folder(&self) -> &Path {
        &self.root
    }

    pub fn walk(&self, config: WalkConfig, visit: impl FnMut(Record)) -> WalkSummary {
        walk::run(self, config, visit)
    }

    /// A second handle to the same folder for a worker thread; its ids stay valid here.
    pub(crate) fn share(&self) -> io::Result<Self> {
        Ok(Self {
            root: self.root.clone(),
            _directory: self._directory.try_clone()?,
            owner: self.owner.clone(),
        })
    }

    /// Path below the folder, for hashing only; never print it.
    pub(crate) fn relative_path<'a>(&self, id: &'a RecordId) -> Result<&'a Path, SourceError> {
        if !Arc::ptr_eq(&self.owner, &id.owner) {
            return Err(SourceError::ForeignId);
        }
        id.path
            .strip_prefix(&self.root)
            .map_err(|_| SourceError::ForeignId)
    }

    /// Raw link text, for hashing only; never print it.
    pub(crate) fn link_target(&self, id: &RecordId) -> Result<PathBuf, SourceError> {
        self.relative_path(id)?;
        std::fs::read_link(id.path.as_path()).map_err(unavailable)
    }

    /// Drive kind and file system class of the selected folder's volume.
    pub fn volume(&self) -> Volume {
        volume::probe(&self.root)
    }

    pub fn open_read(&self, id: &RecordId) -> Result<ReadOnlyFile, SourceError> {
        if !Arc::ptr_eq(&self.owner, &id.owner) {
            return Err(SourceError::ForeignId);
        }
        platform::check_ancestors(&self.root, &id.path)?;
        let file = platform::open_file(&id.path).map_err(unavailable)?;
        let metadata = file.metadata().map_err(unavailable)?;
        if platform::is_link(&metadata) {
            return Err(SourceError::Link);
        }
        if !metadata.is_file() {
            return Err(SourceError::NotFile);
        }
        Ok(ReadOnlyFile { file })
    }
}

pub struct ReadOnlyFile {
    file: File,
}

impl fmt::Debug for ReadOnlyFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ReadOnlyFile(..)")
    }
}

impl ReadOnlyFile {
    /// Link count of the open file.
    pub fn hard_links(&self) -> io::Result<u64> {
        platform::file_hard_links(&self.file)
    }
}

impl Read for ReadOnlyFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file
            .read(buffer)
            .map_err(|error| io::Error::from(error.kind()))
    }
}

fn unavailable(error: io::Error) -> SourceError {
    SourceError::Unavailable(error.kind())
}

/// Opens `path` with a throwaway scratch root that cannot overlap it.
#[cfg(test)]
pub(crate) fn test_open(path: &Path) -> Result<ReadOnlySource, SourceError> {
    let scratch = tempfile::tempdir().map_err(unavailable)?;
    ReadOnlySource::open(path, scratch.path())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod policy_tests;

#[cfg(test)]
mod bounded_tests;

#[cfg(test)]
mod overlap_tests;
