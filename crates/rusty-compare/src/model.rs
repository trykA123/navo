use std::io::Read;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rusty_core::{FileIdentity, ProviderId, ProviderPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FluentId(&'static str);

impl FluentId {
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectId {
    Sha1([u8; Self::SHA1_LEN]),
    Sha256([u8; Self::SHA256_LEN]),
}

impl ObjectId {
    pub const SHA1_LEN: usize = 20;
    pub const SHA256_LEN: usize = 32;

    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() == Self::SHA1_LEN {
            return bytes.try_into().ok().map(Self::Sha1);
        }
        if bytes.len() == Self::SHA256_LEN {
            return bytes.try_into().ok().map(Self::Sha256);
        }
        None
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Sha1(bytes) => bytes,
            Self::Sha256(bytes) => bytes,
        }
    }

    #[must_use]
    pub const fn byte_len(self) -> usize {
        match self {
            Self::Sha1(_) => Self::SHA1_LEN,
            Self::Sha256(_) => Self::SHA256_LEN,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    File,
    Folder,
    Link,
    Submodule,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ContentClass {
    #[default]
    Unknown,
    Text,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelPath(ProviderPath);

impl RelPath {
    #[must_use]
    pub fn new(path: ProviderPath) -> Self {
        Self(path)
    }

    #[must_use]
    pub fn root() -> Self {
        Self(ProviderPath::from_text(""))
    }

    #[must_use]
    pub fn as_path(&self) -> &ProviderPath {
        &self.0
    }

    #[must_use]
    pub fn name(&self) -> ProviderPath {
        let text = self.0.to_text();
        let name = text
            .rsplit_once('/')
            .map_or(text.as_str(), |(_, name)| name);
        ProviderPath::from_text(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideEntry {
    pub rel: RelPath,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<i64>,
    pub ignored: bool,
}

impl SideEntry {
    #[must_use]
    pub fn new(rel: RelPath, kind: EntryKind, size: u64, modified: Option<i64>) -> Self {
        Self {
            rel,
            kind,
            size,
            modified,
            ignored: false,
        }
    }

    #[must_use]
    pub fn name(&self) -> ProviderPath {
        self.rel.name()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(u64);

impl Generation {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Budget(u64);

impl Budget {
    pub const BYTES_PER_MIB: u64 = 1024 * 1024;
    pub const DEFAULT_MIB: u32 = 1024;
    pub const MIN_MIB: u32 = 256;
    pub const MAX_MIB: u32 = 16_384;

    #[must_use]
    pub const fn from_mib(mib: u32) -> Self {
        Self(mib as u64 * Self::BYTES_PER_MIB)
    }

    #[must_use]
    pub const fn from_bytes(bytes: u64) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.0
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::from_mib(Self::DEFAULT_MIB)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffStep {
    Whole,
    Sections,
    Coarse,
    PagedHunks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadHint {
    Buffered,
    Map,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    AccessDenied,
    NotFound,
    ReadError,
    Stopped,
    Link,
    Submodule,
    Unsupported,
    ChangedOnDisk,
    WorkerPanicked,
}

impl Reason {
    #[must_use]
    pub const fn fluent_id(self) -> FluentId {
        FluentId::new(match self {
            Self::AccessDenied => "compare-reason-access-denied",
            Self::NotFound => "compare-reason-not-found",
            Self::ReadError => "compare-reason-read-error",
            Self::Stopped => "compare-reason-stopped",
            Self::Link => "compare-reason-link",
            Self::Submodule => "compare-reason-submodule",
            Self::Unsupported => "compare-reason-unsupported",
            Self::ChangedOnDisk => "compare-reason-changed-on-disk",
            Self::WorkerPanicked => "compare-reason-worker-panicked",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    Same,
    SameAfterRules,
    Different { first_difference: Option<u64> },
    OnlyLeft,
    OnlyRight,
    TypeConflict,
    CaseConflict,
    Checking,
    SameSizeTime,
    NotChecked(Reason),
    Unavailable(Reason),
}

impl Status {
    #[must_use]
    pub const fn fluent_id(self) -> FluentId {
        FluentId::new(match self {
            Self::Same => "compare-status-same",
            Self::SameAfterRules => "compare-status-same-after-rules",
            Self::Different { .. } => "compare-status-different",
            Self::OnlyLeft => "compare-status-only-left",
            Self::OnlyRight => "compare-status-only-right",
            Self::TypeConflict => "compare-status-type-conflict",
            Self::CaseConflict => "compare-status-case-conflict",
            Self::Checking => "compare-status-checking",
            Self::SameSizeTime => "compare-status-same-size-time",
            Self::NotChecked(_) => "compare-status-not-checked",
            Self::Unavailable(_) => "compare-status-unavailable",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: u64,
    pub left: Option<Arc<SideEntry>>,
    pub right: Option<Arc<SideEntry>>,
    pub raw_status: Status,
    pub display_status: Status,
    pub generation: Generation,
    pub content: ContentClass,
}

#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stamp {
    pub size: u64,
    pub modified: Option<i64>,
    pub file_id: Option<FileIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentKey {
    pub provider: ProviderId,
    pub full_path: ProviderPath,
    pub size: u64,
    pub modified: Option<i64>,
    pub file_id: Option<FileIdentity>,
}

pub trait SideSource: Send + Sync {
    fn provider(&self) -> &ProviderId;

    fn volume(&self, _rel: &RelPath) -> u64 {
        0
    }

    fn list(
        &self,
        folder: &RelPath,
        sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), Reason>;

    fn open(&self, rel: &RelPath, hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason>;

    fn stamp(&self, rel: &RelPath) -> Result<Stamp, Reason>;

    fn object_id(&self, rel: &RelPath) -> Option<ObjectId>;

    fn file_id(&self, rel: &RelPath) -> Option<FileIdentity>;

    fn cache_path(&self, rel: &RelPath) -> ProviderPath;

    fn case_sensitive(&self) -> bool;

    fn read_only(&self) -> bool;
}

pub trait HashCache: Send + Sync {
    fn get(&self, key: &ContentKey) -> Option<[u8; 32]>;

    fn put(&self, key: &ContentKey, hash: [u8; 32]);
}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseRule {
    Sensitive,
    Insensitive,
}

impl CaseRule {
    #[must_use]
    pub const fn for_sides(left_case_sensitive: bool, right_case_sensitive: bool) -> Self {
        if left_case_sensitive && right_case_sensitive {
            Self::Sensitive
        } else {
            Self::Insensitive
        }
    }
}
