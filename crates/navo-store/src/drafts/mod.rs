//! Durable editor draft store under `<data>/drafts/` (E7a).
//!
//! Writes go exclusively through `navo_fs::native_replace_user_durable` after syncing a sibling temp
//! file, capping payloads at 32 MiB and checksum-verifying every record with BLAKE3.

mod codec;

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::StoreDirs;

/// Magic prefix identifying a Navo editor draft file.
pub const DRAFT_MAGIC: &[u8; 16] = b"NAVO-DRAFT-V001\n";

/// Current binary format version for editor draft files.
pub const DRAFT_VERSION: u16 = 1;

/// Maximum draft content size (32 MiB); larger buffers are skipped without writing.
pub const MAX_DRAFT_BYTES: usize = 32 * 1024 * 1024;

/// Default debounce delay before flushing an editor draft to disk (1 second).
pub const DRAFT_DEBOUNCE_DELAY: Duration = Duration::from_secs(1);

const DRAFT_EXT: &str = ".draft";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Typed errors returned by [`DraftStore`] operations.
#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    /// Underlying filesystem error.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// File does not begin with [`DRAFT_MAGIC`].
    #[error("not a Navo draft file")]
    BadMagic,
    /// Draft file format version is newer than [`DRAFT_VERSION`].
    #[error("draft format version {found} is newer than supported {supported}")]
    UnsupportedVersion { found: u16, supported: u16 },
    /// Draft file exceeds the bounded read limit.
    #[error("draft file size {found} exceeds maximum {limit} bytes")]
    TooLarge { found: u64, limit: u64 },
    /// Draft file is truncated or has trailing bytes.
    #[error("draft file length mismatch: {0}")]
    Truncated(String),
    /// BLAKE3 integrity checksum does not match file contents.
    #[error("draft checksum mismatch")]
    BadChecksum,
    /// Metadata payload failed to decode or encode.
    #[error("draft metadata error: {0}")]
    CorruptMeta(String),
}

/// Snapshot of the original target file's size and modification time for conflict detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DraftFileStamp {
    /// Target file length in bytes.
    pub size: u64,
    /// Modification time seconds relative to the Unix epoch (negative if before 1970).
    pub modified_unix_secs: i64,
    /// Sub-second nanoseconds component (`0..1_000_000_000`).
    pub modified_subsec_nanos: u32,
}

impl DraftFileStamp {
    /// Builds a stamp from already-fetched filesystem metadata.
    #[must_use]
    pub fn from_metadata(meta: &fs::Metadata) -> Self {
        let (modified_unix_secs, modified_subsec_nanos) = meta
            .modified()
            .ok()
            .map(system_time_to_unix)
            .unwrap_or((0, 0));
        Self {
            size: meta.len(),
            modified_unix_secs,
            modified_subsec_nanos,
        }
    }

    /// Reads the stamp of `path` on disk, returning `Ok(None)` if the file does not exist.
    pub fn from_path(path: &Path) -> io::Result<Option<Self>> {
        match fs::metadata(path) {
            Ok(meta) => Ok(Some(Self::from_metadata(&meta))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// Full editor draft record loaded from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftRecord {
    /// Canonical BLAKE3 hex key for the target path.
    pub key: String,
    /// Original target file path recorded when the draft was saved.
    pub target_path: PathBuf,
    /// Optional stamp of the on-disk target file when the draft was created.
    pub file_stamp: Option<DraftFileStamp>,
    /// Unix timestamp (seconds) when the draft was saved.
    pub saved_at_unix_secs: i64,
    /// BLAKE3 digest of `content`.
    pub content_blake3: [u8; 32],
    /// Raw draft content bytes (UTF-8 or arbitrary bytes).
    pub content: Vec<u8>,
}

/// Lightweight metadata summary of a stored draft, returned by [`DraftStore::list_drafts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftSummary {
    /// Canonical BLAKE3 hex key for the target path.
    pub key: String,
    /// Original target file path recorded when the draft was saved.
    pub target_path: PathBuf,
    /// Optional stamp of the on-disk target file when the draft was created.
    pub file_stamp: Option<DraftFileStamp>,
    /// Unix timestamp (seconds) when the draft was saved.
    pub saved_at_unix_secs: i64,
    /// Length of the draft content in bytes.
    pub content_len: usize,
    /// BLAKE3 digest of the draft content.
    pub content_blake3: [u8; 32],
}

/// Outcome of attempting to persist a draft via [`DraftStore::save`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftSaveOutcome {
    /// Draft was atomically persisted with `durable_replace`.
    Saved {
        /// Canonical key of the stored draft.
        key: String,
        /// Total record bytes written on disk.
        bytes_written: usize,
    },
    /// Content exceeded [`MAX_DRAFT_BYTES`]; no draft file was written and any stale draft was removed.
    SkippedTooLarge {
        /// Rejected content length in bytes.
        size: usize,
        /// Configured size cap ([`MAX_DRAFT_BYTES`]).
        limit: usize,
    },
}

/// Fault-injection hook stages used by crash-safety tests.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteStage {
    AfterPartialTempWrite,
    AfterTempSync,
    BeforeReplace,
}

/// Durable editor draft store rooted at `<data>/drafts/`.
#[derive(Debug, Clone)]
pub struct DraftStore {
    root: PathBuf,
}

impl DraftStore {
    /// Creates a draft store rooted at `root`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Creates a draft store rooted at `dirs.drafts_dir()`.
    #[must_use]
    pub fn from_dirs(dirs: &StoreDirs) -> Self {
        Self::new(dirs.drafts_dir())
    }

    /// Returns the root directory where draft files are stored.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Computes the safe fixed-length 64-char ASCII hex key for `target`.
    #[must_use]
    pub fn key_for(target: &Path) -> String {
        codec::canonical_key(target)
    }

    /// Returns the draft file path inside `self.root` for `target`.
    #[must_use]
    pub fn draft_path(&self, target: &Path) -> PathBuf {
        self.draft_path_for_key(&Self::key_for(target))
    }

    /// Atomically saves a draft for `target` via `navo_fs::native_replace_user_durable`.
    ///
    /// Payloads larger than [`MAX_DRAFT_BYTES`] are skipped without writing a file, and any
    /// stale previous draft for `target` is removed.
    pub fn save(
        &self,
        target: &Path,
        content: &[u8],
        file_stamp: Option<DraftFileStamp>,
    ) -> Result<DraftSaveOutcome, DraftError> {
        let now = system_time_to_unix(SystemTime::now()).0;
        self.save_inner(target, content, file_stamp, now, &mut |_| Ok(()))
    }

    /// Loads and validates the stored draft for `target`, returning `Ok(None)` if absent.
    pub fn load(&self, target: &Path) -> Result<Option<DraftRecord>, DraftError> {
        let key = Self::key_for(target);
        let path = self.draft_path_for_key(&key);
        codec::read_record_bounded(key, &path)
    }

    /// Returns `true` if a draft file exists on disk for `target`.
    pub fn has_draft(&self, target: &Path) -> Result<bool, DraftError> {
        Ok(self.draft_path(target).try_exists()?)
    }

    /// Removes the draft for `target` if present; idempotent when already absent.
    pub fn remove(&self, target: &Path) -> Result<bool, DraftError> {
        remove_if_exists(&self.draft_path(target))
    }

    /// Lists valid drafts currently stored in `self.root`, skipping temp and corrupt files.
    pub fn list_drafts(&self) -> Result<Vec<DraftSummary>, DraftError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(DraftError::Io(error)),
        };
        let mut drafts = Vec::new();
        for entry in entries.flatten() {
            if let Some(summary) = read_entry_summary(&entry.path()) {
                drafts.push(summary);
            }
        }
        drafts.sort_by(|a, b| {
            b.saved_at_unix_secs
                .cmp(&a.saved_at_unix_secs)
                .then_with(|| a.key.cmp(&b.key))
        });
        Ok(drafts)
    }

    /// Deletes valid drafts older than `max_age` and returns how many were removed.
    pub fn prune_older_than(&self, max_age: Duration) -> Result<usize, DraftError> {
        let now = system_time_to_unix(SystemTime::now()).0;
        let age_secs = i64::try_from(max_age.as_secs()).unwrap_or(i64::MAX);
        let cutoff = now.saturating_sub(age_secs);
        let mut removed = 0;
        for summary in self.list_drafts()? {
            if summary.saved_at_unix_secs < cutoff
                && remove_if_exists(&self.draft_path_for_key(&summary.key))?
            {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Removes orphan temp files left by interrupted draft writes.
    pub fn remove_orphan_temps(&self) -> io::Result<usize> {
        crate::remove_orphan_temps(&self.root)
    }

    #[cfg(test)]
    pub(crate) fn save_with_hook(
        &self,
        target: &Path,
        content: &[u8],
        file_stamp: Option<DraftFileStamp>,
        saved_at_unix_secs: i64,
        hook: &mut dyn FnMut(WriteStage) -> io::Result<()>,
    ) -> Result<DraftSaveOutcome, DraftError> {
        self.save_inner(target, content, file_stamp, saved_at_unix_secs, hook)
    }

    fn draft_path_for_key(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}{DRAFT_EXT}"))
    }

    fn save_inner(
        &self,
        target: &Path,
        content: &[u8],
        file_stamp: Option<DraftFileStamp>,
        saved_at_unix_secs: i64,
        hook: &mut dyn FnMut(HookStage) -> io::Result<()>,
    ) -> Result<DraftSaveOutcome, DraftError> {
        if content.len() > MAX_DRAFT_BYTES {
            let _ = self.remove(target)?;
            return Ok(DraftSaveOutcome::SkippedTooLarge {
                size: content.len(),
                limit: MAX_DRAFT_BYTES,
            });
        }
        let key = Self::key_for(target);
        let final_path = self.draft_path_for_key(&key);
        let encoded = codec::encode_record(target, content, file_stamp, saved_at_unix_secs)?;
        fs::create_dir_all(&self.root)?;
        write_durable(&final_path, &encoded, hook)?;
        Ok(DraftSaveOutcome::Saved {
            key,
            bytes_written: encoded.len(),
        })
    }
}

#[cfg(test)]
type HookStage = WriteStage;

#[cfg(not(test))]
type HookStage = ();

/// Writes `bytes` to a synced create-new sibling temp file and commits via `native_replace_user_durable`.
fn write_durable(
    final_path: &Path,
    bytes: &[u8],
    hook: &mut dyn FnMut(HookStage) -> io::Result<()>,
) -> io::Result<()> {
    let mut temp = TempGuard::create(final_path)?;
    write_all_with_hook(&mut temp.file, bytes, hook)?;
    if let Some(handle) = temp.file.as_ref() {
        handle.sync_all()?;
    }
    let temp_path = temp.close_file();
    fire_post_sync(hook)?;
    fire_pre_replace(hook)?;
    navo_fs::native_replace_user_durable(&temp_path, final_path)?;
    temp.disarm();
    Ok(())
}

#[cfg(test)]
fn write_all_with_hook(
    file: &mut Option<fs::File>,
    bytes: &[u8],
    hook: &mut dyn FnMut(WriteStage) -> io::Result<()>,
) -> io::Result<()> {
    let Some(handle) = file.as_mut() else {
        return Err(io::Error::other("temp file closed"));
    };
    let mid = bytes.len() / 2;
    handle.write_all(&bytes[..mid])?;
    hook(WriteStage::AfterPartialTempWrite)?;
    handle.write_all(&bytes[mid..])
}

#[cfg(not(test))]
fn write_all_with_hook(
    file: &mut Option<fs::File>,
    bytes: &[u8],
    _hook: &mut dyn FnMut(()) -> io::Result<()>,
) -> io::Result<()> {
    let Some(handle) = file.as_mut() else {
        return Err(io::Error::other("temp file closed"));
    };
    handle.write_all(bytes)
}

#[cfg(test)]
fn fire_post_sync(hook: &mut dyn FnMut(WriteStage) -> io::Result<()>) -> io::Result<()> {
    hook(WriteStage::AfterTempSync)
}

#[cfg(not(test))]
fn fire_post_sync(_hook: &mut dyn FnMut(()) -> io::Result<()>) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn fire_pre_replace(hook: &mut dyn FnMut(WriteStage) -> io::Result<()>) -> io::Result<()> {
    hook(WriteStage::BeforeReplace)
}

#[cfg(not(test))]
fn fire_pre_replace(_hook: &mut dyn FnMut(()) -> io::Result<()>) -> io::Result<()> {
    Ok(())
}

/// RAII guard for a sibling temp file; cleans up the file on error or panic unless disarmed.
struct TempGuard {
    path: PathBuf,
    file: Option<fs::File>,
    active: bool,
}

impl TempGuard {
    fn create(target: &Path) -> io::Result<Self> {
        let name = target.file_name().unwrap_or_default().to_string_lossy();
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = target.with_file_name(format!(".{name}.tmp-{}-{seq}", std::process::id()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            path,
            file: Some(file),
            active: true,
        })
    }

    fn close_file(&mut self) -> PathBuf {
        self.file.take();
        self.path.clone()
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for TempGuard {
    fn drop(&mut self) {
        self.file.take();
        if self.active {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn read_entry_summary(path: &Path) -> Option<DraftSummary> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(DRAFT_EXT)?;
    if !codec::is_valid_key(stem) {
        return None;
    }
    match codec::read_record_bounded(stem.to_owned(), path) {
        Ok(Some(record)) => Some(DraftSummary {
            key: record.key,
            target_path: record.target_path,
            file_stamp: record.file_stamp,
            saved_at_unix_secs: record.saved_at_unix_secs,
            content_len: record.content.len(),
            content_blake3: record.content_blake3,
        }),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, key = stem, "skipping unreadable draft file");
            None
        }
    }
}

fn remove_if_exists(path: &Path) -> Result<bool, DraftError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(DraftError::Io(error)),
    }
}

fn system_time_to_unix(time: SystemTime) -> (i64, u32) {
    match time.duration_since(UNIX_EPOCH) {
        Ok(delta) => (
            i64::try_from(delta.as_secs()).unwrap_or(i64::MAX),
            delta.subsec_nanos(),
        ),
        Err(err) => {
            let delta = err.duration();
            let secs = i64::try_from(delta.as_secs()).unwrap_or(i64::MAX);
            (-secs, delta.subsec_nanos())
        }
    }
}

/// Pure debounce policy tracking when an edited buffer is due for a draft flush.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DraftDebounce {
    delay: Duration,
    last_edit: Option<Instant>,
}

impl DraftDebounce {
    /// Creates a debounce tracker with the given flush delay.
    #[must_use]
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            last_edit: None,
        }
    }

    /// Returns the configured debounce delay.
    #[must_use]
    pub fn delay(&self) -> Duration {
        self.delay
    }

    /// Records an edit at `now`, pushing the flush deadline forward.
    pub fn mark_dirty(&mut self, now: Instant) {
        self.last_edit = Some(now);
    }

    /// Returns the instant when the pending draft should be flushed, if dirty.
    #[must_use]
    pub fn due_at(&self) -> Option<Instant> {
        self.last_edit.map(|last| last + self.delay)
    }

    /// Returns `true` when a dirty edit has waited at least `delay` as of `now`.
    #[must_use]
    pub fn is_due(&self, now: Instant) -> bool {
        self.due_at().is_some_and(|due| now >= due)
    }

    /// Clears the pending dirty state after a flush, save, or discard.
    pub fn clear(&mut self) {
        self.last_edit = None;
    }
}

impl Default for DraftDebounce {
    fn default() -> Self {
        Self::new(DRAFT_DEBOUNCE_DELAY)
    }
}

#[cfg(test)]
mod tests;
