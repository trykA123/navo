//! Byte sources for the file diff (D8): local files read positionally, Git blobs spooled.
//!
//! Every caller opens its own instance. A process spools under its own locked folder, and a
//! returned spool source owns its file: the last clone deletes it.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use navo_compare::model::{CancelToken, ReadHint, Reason, SideEntry, SideSource};
use navo_compare::text::side::WHOLE_LIMIT;
use navo_core::Location;
use navo_core::paging::ByteSource;
use navo_fs::{PagedFile, os_path};
use navo_store::StoreDirs;

use super::super::settings::CompareSettings;
use super::sides::side_of;

mod spool;

pub(super) use spool::sweep;
#[cfg(test)]
pub(super) use spool::sweep_with;
use spool::{Owner, SpoolFile, owner_for, spool_copy, write_reason};

/// The cache folder that holds one sub-folder per Navo process.
pub(crate) const SPOOL_DIR: &str = "compare-spool";
const COPY_CHUNK: usize = 1024 * 1024;

/// The spool root for `dirs`: Navo's cache folder, which holds `cache.db` too.
pub(super) fn spool_root(dirs: &StoreDirs) -> PathBuf {
    dirs.data.join(SPOOL_DIR)
}

/// The sources of one screen; the returned spool sources outlive it until they drop.
#[derive(Clone)]
pub(crate) struct Sources {
    owner: Option<Arc<Owner>>,
    opens: Arc<AtomicU64>,
}

impl Sources {
    /// Sources with no spool folder; a Git blob over the limit reports a read error.
    pub(crate) fn without_spool() -> Self {
        Self {
            owner: None,
            opens: Arc::default(),
        }
    }

    /// Sources that spool under this process's folder in `base`.
    pub(crate) fn new(base: Option<PathBuf>) -> Self {
        Self {
            owner: base.and_then(|base| owner_for(&base)),
            opens: Arc::default(),
        }
    }

    /// Remove spool folders left by processes that are no longer running.
    pub(crate) fn sweep_startup() {
        if let Some(dirs) = StoreDirs::detect() {
            Self::sweep_in(&dirs);
        }
    }

    /// Sweep the spool root of `dirs`, the same folder spooling sources are created in.
    pub(super) fn sweep_in(dirs: &StoreDirs) {
        sweep(&spool_root(dirs));
    }

    /// How many sources this screen opened, for the separate-instance test.
    #[cfg(test)]
    pub(crate) fn opens(&self) -> u64 {
        self.opens.load(Ordering::Relaxed)
    }

    /// Open one fresh source for `location`; `cancel` stops a large Git copy.
    pub(crate) fn open(
        &self,
        location: &Location,
        settings: &CompareSettings,
        cancel: &CancelToken,
    ) -> Result<Arc<dyn ByteSource + Send + Sync>, Reason> {
        self.opens.fetch_add(1, Ordering::Relaxed);
        if location.is_local() {
            return open_local(location);
        }
        if location.provider().as_str() != "git" {
            return Err(Reason::Unsupported);
        }
        let (side, entry) = side_of(location, settings)?;
        self.open_git(side.as_ref(), &entry, cancel)
    }

    /// Read a Git blob without trusting its listed size, then keep it in memory or spool it.
    pub(super) fn open_git(
        &self,
        source: &dyn SideSource,
        entry: &SideEntry,
        cancel: &CancelToken,
    ) -> Result<Arc<dyn ByteSource + Send + Sync>, Reason> {
        self.open_git_with(GitBlob { source, entry }, cancel, Spill::default())
    }

    /// `open_git` with the memory limit and spool writer of `spill`.
    pub(super) fn open_git_with(
        &self,
        blob: GitBlob<'_>,
        cancel: &CancelToken,
        spill: Spill,
    ) -> Result<Arc<dyn ByteSource + Send + Sync>, Reason> {
        let mut reader = blob.source.open(&blob.entry.rel, ReadHint::Buffered)?;
        let head = read_head(reader.as_mut(), cancel, spill.limit)?;
        if u64::try_from(head.len()).unwrap_or(u64::MAX) <= spill.limit {
            return Ok(Arc::new(MemorySource(head)));
        }
        let Some(owner) = &self.owner else {
            return Err(Reason::ReadError);
        };
        // A failure below drops `spool`, which deletes the partial file.
        let (spool, file) = SpoolFile::create(owner).map_err(write_reason)?;
        let mut out = (spill.writer)(file);
        spool_copy(reader.as_mut(), out.as_mut(), &head, cancel)?;
        drop(out);
        let file = PagedFile::open(&spool.path).map_err(|_| Reason::ReadError)?;
        Ok(Arc::new(SpoolSource {
            file,
            _spool: Arc::new(spool),
        }))
    }
}

/// One Git blob to read: the side it lives on and its listing entry.
pub(super) struct GitBlob<'a> {
    pub source: &'a dyn SideSource,
    pub entry: &'a SideEntry,
}

/// How much of a Git blob stays in memory and how the spool file is written.
pub(super) struct Spill {
    pub limit: u64,
    pub writer: fn(File) -> Box<dyn Write>,
}

impl Default for Spill {
    /// The 64 MiB whole-text limit and a plain file writer.
    fn default() -> Self {
        Self {
            limit: WHOLE_LIMIT,
            writer: |file| Box::new(file),
        }
    }
}

/// A spooled blob read positionally, owning its file.
struct SpoolSource {
    file: PagedFile,
    _spool: Arc<SpoolFile>,
}

impl ByteSource for SpoolSource {
    /// The spooled blob length in bytes.
    fn len(&self) -> u64 {
        self.file.len()
    }

    /// Copy from `offset`; a short result means the end of the blob.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.file.read_at(offset, buf)
    }
}

/// A Git blob small enough to keep in memory.
struct MemorySource(Vec<u8>);

impl ByteSource for MemorySource {
    /// The blob length in bytes.
    fn len(&self) -> u64 {
        u64::try_from(self.0.len()).unwrap_or(u64::MAX)
    }

    /// Copy from `offset`; a short result means the end of the blob.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.0.as_slice().read_at(offset, buf)
    }
}

/// An empty source for a one-sided large pair.
pub(super) fn empty_source() -> Arc<dyn ByteSource + Send + Sync> {
    Arc::new(MemorySource(Vec::new()))
}

/// One local file, or the `Unsupported` reason when it has no OS path.
fn open_local(location: &Location) -> Result<Arc<dyn ByteSource + Send + Sync>, Reason> {
    let path = os_path(location).map_err(|_| Reason::Unsupported)?;
    PagedFile::open(&path)
        .map(|file| Arc::new(file) as Arc<dyn ByteSource + Send + Sync>)
        .map_err(|_| Reason::ReadError)
}

/// Read up to `limit + 1` bytes, or to the end if shorter, checking cancel between 1 MiB
/// chunks. The reader keeps the rest for the spool copy.
fn read_head(reader: &mut dyn Read, cancel: &CancelToken, limit: u64) -> Result<Vec<u8>, Reason> {
    let cap = usize::try_from(limit.saturating_add(1)).unwrap_or(usize::MAX);
    let mut bytes = Vec::with_capacity(cap.min(COPY_CHUNK));
    let mut buffer = vec![0u8; COPY_CHUNK];
    while bytes.len() < cap {
        if cancel.is_cancelled() {
            return Err(Reason::Stopped);
        }
        let want = cap.saturating_sub(bytes.len()).min(COPY_CHUNK);
        match reader.read(&mut buffer[..want]) {
            Ok(0) => break,
            Ok(count) => bytes.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Err(Reason::ReadError),
        }
    }
    Ok(bytes)
}
