//! The compare screen's local file reads and the shared hash cache.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use navo_compare_profile::book::profile_text;
use navo_compare_profile::paths::open_profile;
use sidewise_engine::model::{ContentKey, HashCache};
use sidewise_host::compare::hash_cache::file::{ClearError, FileHashes};

use sidewise_core::Location;
use sidewise_core::paging::ByteSource;
use sidewise_fs::PagedFile;
use sidewise_ui::window_view::CompareFiles;

/// A hash cache that keeps nothing: every lookup misses, so content checks hash afresh.
#[cfg(test)]
pub struct NoHashes;

#[cfg(test)]
impl HashCache for NoHashes {
    /// Always misses, so a content check hashes the file again.
    fn get(&self, _key: &ContentKey) -> Option<[u8; 32]> {
        None
    }

    /// Drops the hash: this cache keeps nothing.
    fn put(&self, _key: &ContentKey, _hash: [u8; 32]) {}
}

/// The cache file name inside the profile's cache folder.
const CACHE_FILE: &str = "hashes.v1";

/// The profile folder choice made at start (`None`: the platform folders); unset in tests.
static HASH_PROFILE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// The one hash cache of this process, opened by the first worker that needs it.
static HASHES: OnceLock<Arc<FileHashes>> = OnceLock::new();

/// Remembers which profile folder the cache opens in. Call once at start; it opens nothing,
/// so the UI thread never waits on the disk.
pub(crate) fn install_hashes(explicit: Option<&Path>) {
    let _ = HASH_PROFILE.set(explicit.map(Path::to_path_buf));
}

/// The profile folder choice made at start: `None` before `install_hashes` ran (tests), else
/// `Some(None)` for the platform folders. Other caches of the profile use the same choice.
pub(crate) fn profile_choice() -> Option<&'static Option<PathBuf>> {
    HASH_PROFILE.get()
}

/// Opens `cache/hashes.v1` of the profile. A profile error is logged and the cache lives in
/// memory for the run, so the compare still works.
pub(crate) fn open_hashes(explicit: Option<&Path>) -> Arc<FileHashes> {
    match open_profile(explicit) {
        Ok(profile) => Arc::new(FileHashes::open(&profile.cache_dir().join(CACHE_FILE))),
        Err(error) => {
            tracing::warn!(error = %profile_text(&error), "compare hashes will not be saved this run");
            Arc::new(FileHashes::memory())
        }
    }
}

/// The shared cache. The first call opens it, so call it from a worker thread. With no
/// installed profile (tests) it lives in memory.
pub(crate) fn shared_hashes() -> Arc<FileHashes> {
    HASHES
        .get_or_init(|| match HASH_PROFILE.get() {
            Some(explicit) => open_hashes(explicit.as_deref()),
            None => Arc::new(FileHashes::memory()),
        })
        .clone()
}

/// Forgets every stored hash; fails with `Busy` when another window holds the cache.
pub(crate) fn clear_hashes() -> Result<usize, ClearError> {
    shared_hashes().clear_all()
}

/// The hash cache handed to the engine: it forwards to the shared store, so the file opens
/// on the first content check, on a worker thread.
pub struct SharedHashes;

impl HashCache for SharedHashes {
    /// The stored hash for `key`, if any.
    fn get(&self, key: &ContentKey) -> Option<[u8; 32]> {
        shared_hashes().get(key)
    }

    /// Stores `hash` for `key`.
    fn put(&self, key: &ContentKey, hash: [u8; 32]) {
        shared_hashes().put(key, hash);
    }

    /// Writes buffered rows; the static store never drops, so each compare must call this.
    fn flush(&self) {
        shared_hashes().flush();
    }
}

/// The compare screen's file reads over the local disk: whole bytes up to a limit, and paged sources.
/// Read-only by construction (CompareFiles has no write).
pub fn compare_files() -> CompareFiles {
    CompareFiles {
        bytes: Arc::new(read_bytes),
        source: Arc::new(open_source),
    }
}

/// Reads at most `limit` bytes of a local file; fails on a non-local location, an open/read error,
/// or a file over `limit` bytes ("Too large to open here (over N MB).").
fn read_bytes(location: &Location, limit: u64) -> Result<Vec<u8>, String> {
    let path = path_of(location)?;
    let file = File::open(&path).map_err(|error| format!("Cannot open it: {error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("Cannot open it: {error}"))?
        .len();
    if size > limit {
        return Err(too_large(limit));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read it: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(too_large(limit));
    }
    Ok(bytes)
}

/// Opens a local file as a paged byte source (sidewise_fs::PagedFile).
fn open_source(location: &Location) -> Result<Arc<dyn ByteSource + Send + Sync>, String> {
    let path = path_of(location)?;
    let paged = PagedFile::open(&path)?;
    Ok(Arc::new(paged))
}

/// The local path of a location; fails with "Only local files open here." when it is not local.
fn path_of(location: &Location) -> Result<PathBuf, String> {
    if !location.is_local() {
        return Err("Only local files open here.".to_owned());
    }
    sidewise_fs::os_path(location).map_err(|error| error.message())
}

/// The "too large" message for `limit` bytes, in whole megabytes.
fn too_large(limit: u64) -> String {
    format!(
        "Too large to open here (over {} MB).",
        limit / (1024 * 1024)
    )
}

#[cfg(test)]
#[path = "host_parts_tests.rs"]
mod tests;
