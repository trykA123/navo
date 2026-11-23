//! BLAKE3 content hashes of local files: a plain read through the system cache, and an uncached
//! read-back for verify-after-copy (Windows: unbuffered handle; Linux: a best-effort cache-drop
//! hint, dirty pages stay cached) that says how it read.

use std::collections::HashSet;
use std::fmt::Display;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::{CancelToken, ReadSource, UncachedHash};

const CHUNK: usize = 256 * 1024;

/// Buffer address alignment for unbuffered reads; covers 512- and 4096-byte sectors.
#[cfg(any(windows, test))]
pub(super) const SECTOR_ALIGN: usize = 4096;
/// `ERROR_INVALID_PARAMETER` and `ERROR_NOT_SUPPORTED`: the volume refuses unbuffered reads.
#[cfg(any(windows, test))]
pub(super) const UNBUFFERED_REFUSED: [i32; 2] = [87, 50];

/// BLAKE3 of the file at `path`, read through the system cache; `None` when `cancel` fires
/// between reads. Fails with the open or read error.
pub(super) fn blake3_of(path: &Path, cancel: &CancelToken) -> io::Result<Option<[u8; 32]>> {
    hash_reader(File::open(path)?, cancel)
}

/// Hashes `file` from its current position to the end in `CHUNK` reads, checking `cancel`
/// before each read.
fn hash_reader(mut file: File, cancel: &CancelToken) -> io::Result<Option<[u8; 32]>> {
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; CHUNK];
    loop {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let read = match file.read(&mut buffer) {
            Ok(0) => return Ok(Some(*hasher.finalize().as_bytes())),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hasher.update(buffer.get(..read).unwrap_or_default());
    }
}

/// BLAKE3 of the file at `path` after a cache-drop hint where the system has one (Linux: a
/// best-effort hint; dirty pages stay cached). `None` when cancelled; fails with the open or read error.
#[cfg(not(windows))]
pub(super) fn blake3_uncached(
    path: &Path,
    cancel: &CancelToken,
) -> io::Result<Option<UncachedHash>> {
    let file = File::open(path)?;
    let source = drop_cached_pages(path, &file);
    let hash = hash_reader(file, cancel)?;
    Ok(hash.map(|hash| UncachedHash { hash, source }))
}

/// Asks the kernel to drop the file's clean cached pages, so the next reads come from the device.
/// Returns `CacheFallback` (and warns once per volume) when the hint is refused.
#[cfg(target_os = "linux")]
fn drop_cached_pages(path: &Path, file: &File) -> ReadSource {
    use rustix::fs::{Advice, fadvise};
    match fadvise(file, 0, None, Advice::DontNeed) {
        Ok(()) => ReadSource::Device,
        Err(error) => {
            warn_fallback(path, &error);
            ReadSource::CacheFallback
        }
    }
}

/// No cache-drop hint is wired up here: the read goes through the cache.
#[cfg(not(any(windows, target_os = "linux")))]
fn drop_cached_pages(path: &Path, _file: &File) -> ReadSource {
    warn_fallback(path, &"no cache-drop hint on this system");
    ReadSource::CacheFallback
}

/// Opens `path` read-only with `FILE_FLAG_NO_BUFFERING`, the handle the uncached hash reads.
#[cfg(windows)]
pub(super) fn unbuffered_open(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_NO_BUFFERING, FILE_FLAG_SEQUENTIAL_SCAN,
    };

    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_SEQUENTIAL_SCAN)
        .open(path)
}

/// BLAKE3 of the file at `path` through an unbuffered handle. A volume that refuses the open or the
/// first read (OS error 87 or 50) falls back to a cached read; any later error fails.
#[cfg(windows)]
pub(super) fn blake3_uncached(
    path: &Path,
    cancel: &CancelToken,
) -> io::Result<Option<UncachedHash>> {
    let file = match unbuffered_open(path) {
        Ok(file) => file,
        Err(error) if is_refusal(&error) => return cached_fallback(path, cancel, &error),
        Err(error) => return Err(error),
    };
    let expected = file.metadata()?.len();
    let mut storage = vec![0_u8; CHUNK + SECTOR_ALIGN];
    let start = storage.as_ptr().align_offset(SECTOR_ALIGN);
    let window = storage
        .get_mut(start..)
        .and_then(|rest| rest.get_mut(..CHUNK))
        .ok_or_else(|| io::Error::other("no sector-aligned buffer for an unbuffered read"))?;
    match hash_aligned(file, window, expected, cancel)? {
        Aligned::Hashed(hash) => Ok(Some(UncachedHash {
            hash,
            source: ReadSource::Device,
        })),
        Aligned::Cancelled => Ok(None),
        Aligned::RefusedAtStart(error) => cached_fallback(path, cancel, &error),
    }
}

/// Hashes `path` through the cache after the volume refused an unbuffered read, warning once per
/// volume. `None` when cancelled.
#[cfg(windows)]
fn cached_fallback(
    path: &Path,
    cancel: &CancelToken,
    refusal: &io::Error,
) -> io::Result<Option<UncachedHash>> {
    warn_fallback(path, refusal);
    let hash = blake3_of(path, cancel)?;
    Ok(hash.map(|hash| UncachedHash {
        hash,
        source: ReadSource::CacheFallback,
    }))
}

/// True when `error` is the volume refusing unbuffered access (OS error 87 or 50).
#[cfg(any(windows, test))]
fn is_refusal(error: &io::Error) -> bool {
    error
        .raw_os_error()
        .is_some_and(|code| UNBUFFERED_REFUSED.contains(&code))
}

/// How an aligned read loop ended.
#[cfg(any(windows, test))]
#[derive(Debug)]
pub(super) enum Aligned {
    Hashed([u8; 32]),
    Cancelled,
    /// The very first read was refused with OS error 87 or 50; the caller may read cached instead.
    RefusedAtStart(io::Error),
}

/// Hashes `reader` with whole reads of `window` (a multiple of `SECTOR_ALIGN` bytes). An aligned
/// short read goes on; a zero or unaligned read ends the file. Fails when a later read fails or the
/// total differs from `expected` bytes (short: "unbuffered read ended early").
#[cfg(any(windows, test))]
pub(super) fn hash_aligned(
    mut reader: impl Read,
    window: &mut [u8],
    expected: u64,
    cancel: &CancelToken,
) -> io::Result<Aligned> {
    let mut hasher = blake3::Hasher::new();
    let mut total: u64 = 0;
    let mut first = true;
    loop {
        if cancel.is_cancelled() {
            return Ok(Aligned::Cancelled);
        }
        let read = match reader.read(window) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if first && is_refusal(&error) => return Ok(Aligned::RefusedAtStart(error)),
            Err(error) => return Err(error),
        };
        first = false;
        hasher.update(window.get(..read).unwrap_or_default());
        total = total.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        // An aligned short read keeps the file position aligned, so the next read is still valid.
        if read == 0 || read % SECTOR_ALIGN != 0 {
            break;
        }
    }
    if total < expected {
        let message = format!("unbuffered read ended early: {total} of {expected} bytes");
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, message));
    }
    if total > expected {
        let message =
            format!("unbuffered read ran past the file size: {total} of {expected} bytes");
        return Err(io::Error::other(message));
    }
    Ok(Aligned::Hashed(*hasher.finalize().as_bytes()))
}

/// Volumes that already logged a cache fallback in this process.
static FALLBACK_VOLUMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Warns that `path` was hashed through the cache, once per volume root per process; later files on
/// the same volume log at debug level.
fn warn_fallback(path: &Path, reason: &dyn Display) {
    let volume = volume_key(path);
    if first_fallback_on(&volume) {
        tracing::warn!(%reason, %volume, "uncached hash read through the cache on this volume");
    } else {
        tracing::debug!(%reason, %volume, "uncached hash read through the cache");
    }
}

/// Records a fallback on `volume`; true only the first time this process sees it.
pub(super) fn first_fallback_on(volume: &str) -> bool {
    let seen = FALLBACK_VOLUMES.get_or_init(|| Mutex::new(HashSet::new()));
    let mut seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
    seen.insert(volume.to_owned())
}

/// The volume root of `path` (drive or share prefix plus root), lowercased so `C:` and `c:` match.
pub(super) fn volume_key(path: &Path) -> String {
    let root: PathBuf = path
        .components()
        .take_while(|part| matches!(part, Component::Prefix(_) | Component::RootDir))
        .collect();
    root.to_string_lossy().to_lowercase()
}
