//! Sparse hex edit patch overlay, undo/redo history, and streaming durable file save.
//! Never reads the whole target file into memory: reads are windowed and saves stream in fixed chunks.

use std::collections::{BTreeMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::SystemTime;

use navo_core::paging::{ByteSource, TextLayout, read_full};
use thiserror::Error;

use crate::FileIdentity;

/// Streaming buffer size used when applying sparse hex patches to disk (64 KiB).
pub const HEX_SAVE_CHUNK: usize = 64 * 1024;

/// Maximum undo history entries retained per hex edit session.
pub const MAX_HEX_UNDO: usize = 10_000;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// File identity and modification stamp captured before editing a hex file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexFileStamp {
    pub len: u64,
    pub modified: Option<SystemTime>,
    pub identity: Option<FileIdentity>,
}

impl HexFileStamp {
    /// Captures the stamp from an already-open regular file handle.
    pub fn from_file(file: &File) -> io::Result<Self> {
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "target is not a regular file",
            ));
        }
        let identity = crate::local::identity::from_file(file)?;
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            identity,
        })
    }

    /// Refuses symlinks, opens `path`, and captures its stamp from the open handle.
    pub fn from_path(path: &Path) -> io::Result<Self> {
        ensure_regular_non_symlink(path)?;
        let file = File::open(path)?;
        Self::from_file(&file)
    }

    /// True when `current` matches the size, mtime, and platform identity of `self`.
    #[must_use]
    pub fn matches(&self, current: &Self) -> bool {
        if self.modified.is_none() && self.identity.is_none() {
            return false;
        }
        self.len == current.len
            && self.modified == current.modified
            && self.identity == current.identity
    }
}

/// Errors produced when durably saving sparse hex edits.
#[derive(Debug, Error)]
pub enum HexSaveError {
    /// Target file size, mtime, or identity changed on disk since edit began.
    #[error("File changed on disk since it was opened")]
    FileChangedOnDisk,
    /// A patch targets an offset at or past the end of the file.
    #[error("Patch offset {offset} is outside file length {len}")]
    OffsetOutOfRange { offset: u64, len: u64 },
    /// Underlying filesystem error during read, sync, or durable replace.
    #[error("Cannot save hex edits: {0}")]
    Io(#[from] io::Error),
    /// Non-IO refusal (for example read-only preview or non-local file).
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Clone, Copy)]
struct ByteDelta {
    offset: u64,
    before: Option<u8>,
    after: Option<u8>,
}

#[derive(Debug, Clone)]
enum PatchEntry {
    Single(ByteDelta),
    Range(Vec<ByteDelta>),
}

impl PatchEntry {
    fn first_offset(&self) -> Option<u64> {
        match self {
            Self::Single(delta) => Some(delta.offset),
            Self::Range(deltas) => deltas.first().map(|delta| delta.offset),
        }
    }
}

/// Sparse in-memory byte patch overlay with bounded undo/redo history.
#[derive(Debug, Default)]
pub struct HexPatchOverlay {
    patches: BTreeMap<u64, u8>,
    undo: VecDeque<PatchEntry>,
    redo: Vec<PatchEntry>,
    revision: u64,
}

impl HexPatchOverlay {
    /// Creates an empty overlay with no pending byte patches.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Overwrites a single byte at `offset`, recording the exact prior state for undo.
    pub fn overwrite_byte(&mut self, offset: u64, original: u8, value: u8) -> bool {
        let before = self.patches.get(&offset).copied();
        let after = (value != original).then_some(value);
        if before == after {
            return false;
        }
        apply_patch_slot(&mut self.patches, offset, after);
        self.push_undo(PatchEntry::Single(ByteDelta {
            offset,
            before,
            after,
        }));
        true
    }

    /// Updates an in-progress single-byte edit (second hex nibble) within one undo step.
    pub fn coalesce_byte(&mut self, offset: u64, original: u8, value: u8) -> bool {
        let after = (value != original).then_some(value);
        if let Some(PatchEntry::Single(last)) = self.undo.back_mut()
            && last.offset == offset
        {
            apply_patch_slot(&mut self.patches, offset, after);
            if last.before == after {
                self.undo.pop_back();
            } else {
                last.after = after;
            }
            self.redo.clear();
            self.revision = self.revision.wrapping_add(1);
            return true;
        }
        self.overwrite_byte(offset, original, value)
    }

    /// Overwrites a contiguous byte range starting at `offset` as one atomic undo step.
    pub fn overwrite_range(&mut self, offset: u64, originals: &[u8], values: &[u8]) -> bool {
        let count = originals.len().min(values.len());
        let mut deltas = Vec::new();
        for index in 0..count {
            let at = offset.saturating_add(index as u64);
            let before = self.patches.get(&at).copied();
            let after = (values[index] != originals[index]).then_some(values[index]);
            if before != after {
                apply_patch_slot(&mut self.patches, at, after);
                deltas.push(ByteDelta {
                    offset: at,
                    before,
                    after,
                });
            }
        }
        if deltas.is_empty() {
            return false;
        }
        self.push_undo(PatchEntry::Range(deltas));
        true
    }

    /// Undoes the most recent edit, returning the first affected offset.
    pub fn undo(&mut self) -> Option<u64> {
        let entry = self.undo.pop_back()?;
        let at = entry.first_offset();
        apply_entry(&mut self.patches, &entry, false);
        self.redo.push(entry);
        self.revision = self.revision.wrapping_add(1);
        at
    }

    /// Reapplies the most recently undone edit, returning the first affected offset.
    pub fn redo(&mut self) -> Option<u64> {
        let entry = self.redo.pop()?;
        let at = entry.first_offset();
        apply_entry(&mut self.patches, &entry, true);
        if self.undo.len() >= MAX_HEX_UNDO {
            self.undo.pop_front();
        }
        self.undo.push_back(entry);
        self.revision = self.revision.wrapping_add(1);
        at
    }

    /// True when at least one offset differs from the underlying file.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        !self.patches.is_empty()
    }

    /// Number of currently modified byte offsets.
    #[must_use]
    pub fn patched_count(&self) -> usize {
        self.patches.len()
    }

    /// True when `offset` currently carries a patched byte.
    #[must_use]
    pub fn is_offset_patched(&self, offset: u64) -> bool {
        self.patches.contains_key(&offset)
    }

    /// Returns the patched byte at `offset`, if modified.
    #[must_use]
    pub fn patched_byte(&self, offset: u64) -> Option<u8> {
        self.patches.get(&offset).copied()
    }

    /// Returns sorted patched offsets within `range`.
    #[must_use]
    pub fn patched_in_range(&self, range: Range<u64>) -> Vec<u64> {
        self.patches.range(range).map(|(&at, _)| at).collect()
    }

    /// Monotonic revision bumped on every mutation, undo, redo, or clear.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// True when at least one undo step is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// True when at least one redo step is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Overlays any patches intersecting `[offset, offset + buf.len())` onto `buf`.
    pub fn apply_to_slice(&self, offset: u64, buf: &mut [u8]) {
        apply_patches_to_chunk(&self.patches, offset, buf);
    }

    /// Clones only the sparse patch map for background streaming save.
    #[must_use]
    pub fn snapshot_patches(&self) -> BTreeMap<u64, u8> {
        self.patches.clone()
    }

    /// Clears all patches and undo/redo history after a committed save.
    pub fn clear(&mut self) {
        self.patches.clear();
        self.undo.clear();
        self.redo.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    fn push_undo(&mut self, entry: PatchEntry) {
        if self.undo.len() >= MAX_HEX_UNDO {
            self.undo.pop_front();
        }
        self.undo.push_back(entry);
        self.redo.clear();
        self.revision = self.revision.wrapping_add(1);
    }
}

fn apply_patch_slot(patches: &mut BTreeMap<u64, u8>, offset: u64, value: Option<u8>) {
    match value {
        Some(byte) => {
            patches.insert(offset, byte);
        }
        None => {
            patches.remove(&offset);
        }
    }
}

fn apply_entry(patches: &mut BTreeMap<u64, u8>, entry: &PatchEntry, forward: bool) {
    let pick = |delta: &ByteDelta| if forward { delta.after } else { delta.before };
    match entry {
        PatchEntry::Single(delta) => apply_patch_slot(patches, delta.offset, pick(delta)),
        PatchEntry::Range(deltas) => {
            if forward {
                for delta in deltas {
                    apply_patch_slot(patches, delta.offset, pick(delta));
                }
            } else {
                for delta in deltas.iter().rev() {
                    apply_patch_slot(patches, delta.offset, pick(delta));
                }
            }
        }
    }
}

/// Applies all sparse patches intersecting `[offset, offset + buf.len())` onto `buf`.
fn apply_patches_to_chunk(patches: &BTreeMap<u64, u8>, offset: u64, buf: &mut [u8]) {
    if patches.is_empty() || buf.is_empty() {
        return;
    }
    let end = offset.saturating_add(buf.len() as u64);
    for (&at, &byte) in patches.range(offset..end) {
        if let Ok(index) = usize::try_from(at - offset)
            && let Some(slot) = buf.get_mut(index)
        {
            *slot = byte;
        }
    }
}

/// A `ByteSource` adapter layering a sparse `HexPatchOverlay` over any paged byte source.
pub struct PatchedByteSource<S: ?Sized = dyn ByteSource + Send + Sync> {
    overlay: Arc<RwLock<HexPatchOverlay>>,
    base: Arc<S>,
}

impl<S: ByteSource + ?Sized> PatchedByteSource<S> {
    /// Wraps `base` with a fresh empty patch overlay.
    #[must_use]
    pub fn new(base: Arc<S>) -> Self {
        Self {
            overlay: Arc::new(RwLock::new(HexPatchOverlay::new())),
            base,
        }
    }

    /// Wraps `base` with a shared `overlay` handle.
    #[must_use]
    pub fn with_overlay(base: Arc<S>, overlay: Arc<RwLock<HexPatchOverlay>>) -> Self {
        Self { overlay, base }
    }

    /// Returns the shared overlay handle.
    #[must_use]
    pub fn overlay(&self) -> &Arc<RwLock<HexPatchOverlay>> {
        &self.overlay
    }

    /// Reads the unpatched original byte at `offset` directly from the underlying source.
    pub fn read_original_byte(&self, offset: u64) -> Result<Option<u8>, String> {
        let mut byte = [0u8; 1];
        let read = self.base.read_at(offset, &mut byte)?;
        Ok((read == 1).then_some(byte[0]))
    }

    /// Reads `[offset, offset + buf.len())` from the base source and overlays patched bytes.
    pub fn read_range(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        read_full(self, offset, buf)
    }
}

impl<S: ByteSource + ?Sized> ByteSource for PatchedByteSource<S> {
    fn len(&self) -> u64 {
        self.base.len()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let read = self.base.read_at(offset, buf)?;
        let overlay = self.overlay.read().unwrap_or_else(PoisonError::into_inner);
        overlay.apply_to_slice(offset, &mut buf[..read]);
        Ok(read)
    }

    fn probe_end(&self) -> Result<(), String> {
        self.base.probe_end()
    }

    fn take_changed(&self) -> bool {
        self.base.take_changed()
    }

    fn take_rewritten(&self) -> bool {
        self.base.take_rewritten()
    }

    fn revision(&self) -> u64 {
        self.base.revision()
    }

    fn revalidate(&self) -> io::Result<bool> {
        self.base.revalidate()
    }

    fn layout(&self) -> TextLayout {
        self.base.layout()
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveFaultStage {
    AfterFirstChunk,
    BeforeReplace,
}

#[cfg(test)]
type FaultHook<'a> = &'a mut dyn FnMut(SaveFaultStage) -> io::Result<()>;

#[cfg(not(test))]
type FaultHook<'a> = &'a mut dyn FnMut(()) -> io::Result<()>;

/// Streams `target` in fixed chunks while applying `patches` into a synced sibling temp file,
/// checks `expected` file stamp before writing and replacing, and commits via `native_replace_user_durable`.
pub fn save_patched_file(
    target: &Path,
    expected: &HexFileStamp,
    patches: &BTreeMap<u64, u8>,
) -> Result<HexFileStamp, HexSaveError> {
    save_patched_inner(target, expected, patches, &mut |_| Ok(()))
}

#[cfg(test)]
pub(crate) fn save_patched_file_with_hook(
    target: &Path,
    expected: &HexFileStamp,
    patches: &BTreeMap<u64, u8>,
    hook: FaultHook<'_>,
) -> Result<HexFileStamp, HexSaveError> {
    save_patched_inner(target, expected, patches, hook)
}

fn save_patched_inner(
    target: &Path,
    expected: &HexFileStamp,
    patches: &BTreeMap<u64, u8>,
    hook: FaultHook<'_>,
) -> Result<HexFileStamp, HexSaveError> {
    ensure_regular_non_symlink(target)?;
    let mut source = File::open(target)?;
    let current = HexFileStamp::from_file(&source)?;
    if !expected.matches(&current) {
        return Err(HexSaveError::FileChangedOnDisk);
    }
    validate_patch_offsets(patches, current.len)?;

    let mut temp = TempFileGuard::create(target)?;
    copy_unix_permissions(&source, &temp)?;
    let copied = stream_patched_chunks(&mut source, &mut temp, patches, hook)?;
    drop(source);
    if copied != current.len {
        return Err(HexSaveError::FileChangedOnDisk);
    }
    temp.sync_and_close()?;

    // Re-check right before replace so an external edit during streaming cannot be clobbered.
    let rechecked = HexFileStamp::from_path(target)?;
    if !expected.matches(&rechecked) {
        return Err(HexSaveError::FileChangedOnDisk);
    }
    fire_before_replace(hook)?;
    crate::native_replace_user_durable(&temp.path, target)?;
    temp.disarm();
    Ok(HexFileStamp::from_path(target)?)
}

fn validate_patch_offsets(patches: &BTreeMap<u64, u8>, len: u64) -> Result<(), HexSaveError> {
    if let Some(&offset) = patches.keys().next_back()
        && offset >= len
    {
        return Err(HexSaveError::OffsetOutOfRange { offset, len });
    }
    Ok(())
}

fn ensure_regular_non_symlink(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hex save requires a regular non-symlink file",
        ));
    }
    Ok(())
}

fn copy_unix_permissions(source: &File, temp: &TempFileGuard) -> io::Result<()> {
    #[cfg(unix)]
    if let Some(handle) = temp.file.as_ref() {
        handle.set_permissions(source.metadata()?.permissions())?;
    }
    #[cfg(not(unix))]
    let _ = (source, temp);
    Ok(())
}

fn stream_patched_chunks(
    source: &mut File,
    temp: &mut TempFileGuard,
    patches: &BTreeMap<u64, u8>,
    hook: FaultHook<'_>,
) -> Result<u64, HexSaveError> {
    let mut buf = vec![0u8; HEX_SAVE_CHUNK];
    let mut offset = 0u64;
    let mut chunk_index = 0u64;
    loop {
        let read = read_chunk(source, &mut buf)?;
        if read == 0 {
            break;
        }
        apply_patches_to_chunk(patches, offset, &mut buf[..read]);
        temp.write_all(&buf[..read])?;
        offset = offset.saturating_add(read as u64);
        chunk_index += 1;
        if chunk_index == 1 {
            fire_after_first_chunk(hook)?;
        }
    }
    Ok(offset)
}

fn read_chunk(source: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match source.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

#[cfg(test)]
fn fire_after_first_chunk(hook: FaultHook<'_>) -> io::Result<()> {
    hook(SaveFaultStage::AfterFirstChunk)
}

#[cfg(not(test))]
fn fire_after_first_chunk(_hook: FaultHook<'_>) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn fire_before_replace(hook: FaultHook<'_>) -> io::Result<()> {
    hook(SaveFaultStage::BeforeReplace)
}

#[cfg(not(test))]
fn fire_before_replace(_hook: FaultHook<'_>) -> io::Result<()> {
    Ok(())
}

/// RAII guard for a create-new sibling temp file; unlinks it on error or panic unless disarmed.
struct TempFileGuard {
    path: PathBuf,
    file: Option<File>,
    active: bool,
}

impl TempFileGuard {
    fn create(target: &Path) -> io::Result<Self> {
        let pid = std::process::id();
        for _ in 0..16 {
            let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
            // Fixed-length sibling name stays well under 255-byte component limits.
            let path = target.with_file_name(format!(".navo-hex-{pid}-{seq}.tmp"));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                        active: true,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate unique hex save temp file",
        ))
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some(file) = self.file.as_mut() else {
            return Err(io::Error::other("temp file already closed"));
        };
        file.write_all(bytes)
    }

    fn sync_and_close(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.take() {
            file.sync_all()?;
        }
        Ok(())
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        self.file.take();
        if self.active {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
#[path = "hex_patch_tests.rs"]
mod tests;
