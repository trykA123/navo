#![forbid(unsafe_code)]

use std::borrow::Cow;
use std::fmt;
use std::io::{self, Read};

pub use navo_compare::model::ObjectId;

pub mod choose;
pub mod cli;
mod engine;
mod error;
pub mod format;
pub mod gix_source;
pub mod status;
pub mod trust;

pub use choose::{
    CLI_ENGINE_ID, Choice, EngineKind, EngineSetting, GIX_ENGINE_ID, GitFound, INSTALL_GIT_NOTE,
    MERGE_BASE_GIX_NOTE, NO_GIT_NOTE, UNSUPPORTED_FORMAT_NOTE, Work, choose,
};
pub use cli::{
    Cancel, CliGit, CliOptions, DEFAULT_TIMEOUT, ProcessOptions, ProcessOutcome, ProcessOutput,
    SYNC_TIMEOUT, StatusLimits, SyncKind, SyncPreview, SyncSummary, SyncTarget, Tracking,
};
pub use engine::{Engine, EngineInfo, EngineRegistry};
pub use error::GitError;
pub use format::{ObjectFormat, RefStorage, RepoFormat};
pub use gix_source::GixGit;

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
pub type WorktreeFinish = Box<dyn FnOnce() -> io::Result<Vec<FilterNotice>> + Send>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterNotice {
    Lfs { detail: String },
    Filter { detail: String },
}

pub struct WorktreeBlob {
    pub reader: BlobReader,
    pub reason: Option<FilterNotice>,
    pub finish: Option<WorktreeFinish>,
}

impl WorktreeBlob {
    pub fn finish(&mut self) -> io::Result<Vec<FilterNotice>> {
        match self.finish.take() {
            Some(finish) => finish(),
            None => Ok(Vec::new()),
        }
    }
}

pub trait GitSource: Send + Sync {
    fn resolve(&self, name: &str) -> Result<Option<ObjectId>, GitError>;

    fn refs(&self) -> Result<Vec<GitRef>, GitError>;

    fn list_tree(&self, rev: &str) -> Result<Vec<TreeEntry>, GitError>;

    fn read_blob(&self, id: &ObjectId) -> Result<BlobReader, GitError>;

    fn read_blob_worktree_form(&self, rev: &str, path: &GitPath) -> Result<WorktreeBlob, GitError>;

    fn changes(&self, a: &str, target: Target<'_>) -> Result<Vec<Change>, GitError>;

    fn merge_base(&self, a: &str, b: &str) -> Result<Option<ObjectId>, GitError>;

    fn index_entries(&self, stage: Stage) -> Result<Vec<IndexEntry>, GitError>;

    fn others(&self, which: Others) -> Result<Vec<OtherPath>, GitError>;
}
