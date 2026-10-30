#![forbid(unsafe_code)]

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use rusty_compare::model::ObjectId;

pub mod cli;
pub mod format;

pub use cli::{CliGit, CliOptions, DEFAULT_TIMEOUT};
pub use format::{ObjectFormat, RefStorage, RepoFormat};

#[derive(Debug)]
pub enum GitError {
    Io {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    NotARepository {
        path: PathBuf,
    },
    InvalidRepositoryFile {
        path: PathBuf,
        detail: String,
    },
    InvalidConfig {
        path: PathBuf,
        detail: String,
    },
    UnsupportedObjectFormat {
        value: String,
    },
    UnsupportedRefStorage {
        value: String,
    },
    Unsupported {
        operation: String,
    },
    GitNotFound,
    GitTooOld {
        version: String,
    },
    Command {
        args: String,
        status: Option<i32>,
        stderr: String,
    },
    UnsafeDirectory {
        path: PathBuf,
    },
    Timeout,
    Protocol {
        detail: String,
    },
    SessionBusy,
    SessionClosed,
    MissingObject {
        id: ObjectId,
    },
    DuplicateEngine {
        id: String,
    },
}

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, kind } => write!(formatter, "{}: {kind:?}", path.display()),
            Self::NotARepository { path } => {
                write!(formatter, "not a Git repository: {}", path.display())
            }
            Self::InvalidRepositoryFile { path, detail } => {
                write!(formatter, "invalid Git file {}: {detail}", path.display())
            }
            Self::InvalidConfig { path, detail } => {
                write!(formatter, "invalid Git config {}: {detail}", path.display())
            }
            Self::UnsupportedObjectFormat { value } => {
                write!(formatter, "unsupported Git object format: {value}")
            }
            Self::UnsupportedRefStorage { value } => {
                write!(formatter, "unsupported Git ref storage: {value}")
            }
            Self::Unsupported { operation } => {
                write!(formatter, "the Git engine does not support {operation}")
            }
            Self::GitNotFound => formatter.write_str("git was not found on PATH"),
            Self::GitTooOld { version } => {
                write!(formatter, "git {version} is older than 2.36")
            }
            Self::Command {
                args,
                status,
                stderr,
            } => write!(formatter, "git {args} exited with {status:?}: {stderr}"),
            Self::UnsafeDirectory { path } => {
                write!(
                    formatter,
                    "git refuses to use a directory owned by someone else: {}",
                    path.display()
                )
            }
            Self::Timeout => formatter.write_str("git did not answer before the deadline"),
            Self::Protocol { detail } => write!(formatter, "unexpected git output: {detail}"),
            Self::SessionBusy => formatter.write_str("the git cat-file session is busy"),
            Self::SessionClosed => formatter.write_str("the git cat-file session closed"),
            Self::MissingObject { id } => write!(formatter, "git object is missing: {id:?}"),
            Self::DuplicateEngine { id } => {
                write!(
                    formatter,
                    "a Git engine with id {id:?} is already registered"
                )
            }
        }
    }
}

impl Error for GitError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GitPath(Vec<u8>);

impl GitPath {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }
}

impl From<&str> for GitPath {
    fn from(path: &str) -> Self {
        Self(path.as_bytes().to_vec())
    }
}

impl From<String> for GitPath {
    fn from(path: String) -> Self {
        Self(path.into_bytes())
    }
}

impl fmt::Display for GitPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_string_lossy())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    File,
    Link,
    Folder,
    Submodule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Deleted,
    Modified,
    TypeChanged,
    Conflict { stages: StageSet },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Base,
    Ours,
    Theirs,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct StageSet(u8);

impl StageSet {
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0b111)
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn has(self, stage: Stage) -> bool {
        self.0 & stage.bit() != 0
    }

    #[must_use]
    pub const fn with(self, stage: Stage) -> Self {
        Self(self.0 | stage.bit())
    }
}

impl Stage {
    const fn bit(self) -> u8 {
        match self {
            Self::Base => 0b001,
            Self::Ours => 0b010,
            Self::Theirs => 0b100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Branch,
    Tag,
    Remote,
    Head,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    Rev(&'a str),
    Index,
    Worktree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Others {
    Untracked,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherPath {
    pub path: GitPath,
    pub is_folder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRef {
    pub name: String,
    pub kind: RefKind,
    pub target: ObjectId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: GitPath,
    pub id: ObjectId,
    pub kind: TreeKind,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: GitPath,
    pub status: ChangeStatus,
    pub old_id: Option<ObjectId>,
    pub new_id: Option<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub path: GitPath,
    pub id: ObjectId,
    pub kind: TreeKind,
}

pub type BlobReader = Box<dyn Read + Send>;

pub trait GitSource: Send + Sync {
    fn resolve(&self, name: &str) -> Result<Option<ObjectId>, GitError>;

    fn refs(&self) -> Result<Vec<GitRef>, GitError>;

    fn list_tree(&self, rev: &str) -> Result<Vec<TreeEntry>, GitError>;

    fn read_blob(&self, id: &ObjectId) -> Result<BlobReader, GitError>;

    fn read_blob_worktree_form(&self, rev: &str, path: &GitPath) -> Result<BlobReader, GitError>;

    fn changes(&self, a: &str, target: Target<'_>) -> Result<Vec<Change>, GitError>;

    fn merge_base(&self, a: &str, b: &str) -> Result<Option<ObjectId>, GitError>;

    fn index_entries(&self, stage: Stage) -> Result<Vec<IndexEntry>, GitError>;

    fn others(&self, which: Others) -> Result<Vec<OtherPath>, GitError>;
}

type Factory = dyn Fn(&Path) -> Result<Box<dyn GitSource>, GitError> + Send + Sync;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInfo {
    pub id: String,
    pub label: String,
}

#[derive(Clone)]
pub struct Engine {
    info: EngineInfo,
    factory: Arc<Factory>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl Engine {
    #[must_use]
    pub fn info(&self) -> &EngineInfo {
        &self.info
    }

    pub fn open(&self, repo: &Path) -> Result<Box<dyn GitSource>, GitError> {
        (self.factory)(repo)
    }
}

#[derive(Debug, Clone, Default)]
pub struct EngineRegistry {
    engines: Vec<Engine>,
}

impl EngineRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(&mut self, id: &str, label: &str, factory: F) -> Result<(), GitError>
    where
        F: Fn(&Path) -> Result<Box<dyn GitSource>, GitError> + Send + Sync + 'static,
    {
        if self.engines.iter().any(|engine| engine.info.id == id) {
            return Err(GitError::DuplicateEngine { id: id.to_owned() });
        }
        self.engines.push(Engine {
            info: EngineInfo {
                id: id.to_owned(),
                label: label.to_owned(),
            },
            factory: Arc::new(factory),
        });
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &EngineInfo> {
        self.engines.iter().map(Engine::info)
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Engine> {
        self.engines.iter().find(|engine| engine.info.id == id)
    }
}
