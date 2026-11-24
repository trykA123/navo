//! Orders a full build against the snapshot on disk: the old snapshot is removed only after the
//! replacement index was enumerated, so a stop or a failed build never forces a cold start.

use std::io;
use std::path::Path;

use navo_index::ntfs::NtfsError;

/// Runs `enumerate`, then `discard`; a failed or cancelled enumeration returns its error and
/// leaves the snapshot on disk for the next start to restore. A failed discard is returned too.
pub(super) fn enumerate_then_discard<T>(
    enumerate: impl FnOnce() -> Result<T, NtfsError>,
    discard: impl FnOnce() -> Result<bool, NtfsError>,
) -> Result<(T, bool), NtfsError> {
    let candidate = enumerate()?;
    let discarded = discard()?;
    Ok((candidate, discarded))
}

/// Removes the snapshot at `path`; returns whether a file was removed (false when none existed).
/// Fails with an I/O error for any other cause, so the build stops instead of keeping it.
pub(super) fn discard_snapshot(path: &Path) -> Result<bool, NtfsError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(NtfsError::Io {
            operation: "discard snapshot",
            source,
        }),
    }
}

#[cfg(test)]
#[path = "snapshot_discard_tests.rs"]
mod tests;
