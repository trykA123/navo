//! Three-stage duplicate file finder (size -> xxhash 4 KiB head/tail -> full BLAKE3)
//! with safety guards for cloud placeholders, symlinks/junctions, hard links, and network shares.

mod hash;
mod walk;

use std::fs::File;
use std::io::{Read, Seek};
use std::ops::ControlFlow;

use navo_core::{FsError, FsErrorKind, Location};

use crate::{
    CancelToken, DriveClass, FileIdentity, ListSink, LocalFs, Provider, ProviderOps, drive_class,
    os_path,
};

/// Number of bytes hashed from the start and end of a file during Stage 2 partial hashing.
pub const PARTIAL_HASH_BYTES: usize = 4096;

/// Buffer size for streaming Stage 3 BLAKE3 hashing.
pub const FULL_HASH_CHUNK_BYTES: usize = 256 * 1024;

/// A single file within a confirmed [`DuplicateGroup`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateEntry {
    pub location: Location,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub identity: Option<FileIdentity>,
}

/// Two or more distinct files with identical size and full BLAKE3 content hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub size: u64,
    pub hash: [u8; 32],
    pub entries: Vec<DuplicateEntry>,
}

impl DuplicateGroup {
    /// Total bytes reclaimable by keeping one copy and removing the remaining entries.
    #[must_use]
    pub fn reclaimable_bytes(&self) -> u64 {
        let removable = u64::try_from(self.entries.len().saturating_sub(1)).unwrap_or(u64::MAX);
        self.size.saturating_mul(removable)
    }
}

/// Counts collected while scanning a tree for duplicate files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DuplicateSummary {
    pub scanned_files: u64,
    pub scanned_folders: u64,
    pub duplicate_groups: u64,
    pub duplicate_files: u64,
    pub reclaimable_bytes: u64,
    pub skipped_placeholders: u64,
    pub skipped_links: u64,
    pub skipped_hard_links: u64,
    pub skipped_small: u64,
    pub skipped_errors: u64,
    pub truncated_by_cap: bool,
}

/// Policy controlling whether a duplicate scan may crawl a [`DriveClass::Network`] root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetworkScope {
    /// Refuse network roots immediately before reading any directory.
    #[default]
    Refuse,
    /// Allow crawling a bounded network subfolder up to `max_items` visited entries.
    BoundedSubfolder { max_items: usize },
}

/// Options governing candidate filtering and network safety for a duplicate scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateOptions {
    /// Minimum file size in bytes to consider (defaults to `1`, skipping 0-byte files).
    pub min_size: u64,
    /// Network root policy (defaults to [`NetworkScope::Refuse`]).
    pub network_scope: NetworkScope,
}

impl Default for DuplicateOptions {
    fn default() -> Self {
        Self {
            min_size: 1,
            network_scope: NetworkScope::Refuse,
        }
    }
}

/// Errors that stop a duplicate scan.
#[derive(Debug, thiserror::Error)]
pub enum DupesError {
    #[error(
        "Duplicate scan refused on network root {}; scope a bounded subfolder with an item cap instead.",
        .0.as_str()
    )]
    NetworkRootRefused(Location),
    #[error("Bounded network duplicate scan requires max_items > 0.")]
    InvalidNetworkCap,
    #[error("Duplicate scan works on local and network drives: {}.", .0.as_str())]
    UnsupportedRoot(Location),
    #[error("The duplicate scan was cancelled.")]
    Cancelled,
    #[error("{}", .0.message())]
    Filesystem(FsError),
}

/// Seekable reader returned by [`DupesSource::open_read`] for partial and full hashing.
pub trait DupesRead: Read + Seek + Send {}

impl<T: Read + Seek + Send> DupesRead for T {}

/// Filesystem seam used by the duplicate finder for walking, identity lookup, and byte reads.
pub trait DupesSource: Send + Sync {
    /// Classifies the drive hosting `root` so network shares can be refused before crawling.
    fn drive_class(&self, root: &Location) -> DriveClass;

    /// Lists direct children of `folder` without following links or hydrating cloud placeholders.
    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError>;

    /// Reads the physical file identity (volume + inode/file ID) without following links.
    fn file_identity(&self, location: &Location) -> Result<Option<FileIdentity>, FsError>;

    /// Opens `location` for reading bytes during Stage 2 partial hashing and Stage 3 full hashing.
    fn open_read(&self, location: &Location) -> Result<Box<dyn DupesRead>, FsError>;
}

/// Default [`DupesSource`] backed by [`LocalFs`] and OS drive classification.
#[derive(Debug, Default, Clone)]
pub struct LocalDupesSource {
    fs: LocalFs,
}

impl LocalDupesSource {
    /// Creates a local duplicate source backed by [`LocalFs`].
    #[must_use]
    pub fn new() -> Self {
        Self { fs: LocalFs::new() }
    }
}

impl DupesSource for LocalDupesSource {
    fn drive_class(&self, root: &Location) -> DriveClass {
        drive_class(root)
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.fs.list(folder, sink, cancel)
    }

    fn file_identity(&self, location: &Location) -> Result<Option<FileIdentity>, FsError> {
        self.fs.file_identity(location)
    }

    fn open_read(&self, location: &Location) -> Result<Box<dyn DupesRead>, FsError> {
        let path = os_path(location)?;
        let file = File::open(&path).map_err(|error| {
            let os_code = error.raw_os_error();
            let kind = match error.kind() {
                std::io::ErrorKind::NotFound => FsErrorKind::NotFound,
                std::io::ErrorKind::PermissionDenied => FsErrorKind::AccessDenied,
                _ => FsErrorKind::Unknown,
            };
            FsError {
                kind,
                location: Some(location.clone()),
                os_code,
                holders: Vec::new(),
            }
        })?;
        Ok(Box::new(file))
    }
}

/// Scans `root` on the local filesystem for duplicate files and streams confirmed groups to `sink`.
pub fn find_duplicates(
    root: &Location,
    options: &DuplicateOptions,
    cancel: &CancelToken,
    sink: impl FnMut(DuplicateGroup) -> ControlFlow<()>,
) -> Result<DuplicateSummary, DupesError> {
    let source = LocalDupesSource::new();
    find_duplicates_with(&source, root, options, cancel, sink)
}

/// Scans `root` through `source` for duplicate files and streams confirmed groups to `sink`.
pub fn find_duplicates_with(
    source: &dyn DupesSource,
    root: &Location,
    options: &DuplicateOptions,
    cancel: &CancelToken,
    mut sink: impl FnMut(DuplicateGroup) -> ControlFlow<()>,
) -> Result<DuplicateSummary, DupesError> {
    let mut summary = DuplicateSummary::default();
    let buckets = walk::collect_size_buckets(source, root, options, cancel, &mut summary)?;
    hash::process_buckets(source, buckets, cancel, &mut summary, &mut sink)?;
    Ok(summary)
}

#[cfg(test)]
mod tests;
