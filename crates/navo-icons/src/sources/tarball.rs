//! Bounded in-memory gzip and tar reader for npm icon tarballs.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use tar::Archive;

use super::SourceError;

/// The most tar entries one archive may hold.
const MAX_ENTRIES: usize = 40_000;
/// The most bytes one kept entry may hold (accommodates whole-set JSON indexes such as `@tabler/icons`).
const MAX_ENTRY: u64 = 8 * 1024 * 1024;
/// The most bytes all kept entries may hold together.
const MAX_TOTAL: usize = 256 * 1024 * 1024;

/// Reads the wanted regular files from a gzipped tar, enforcing the entry and size caps.
pub(super) fn read(
    tgz: &[u8],
    wanted: &dyn Fn(&str) -> bool,
) -> Result<BTreeMap<String, Vec<u8>>, SourceError> {
    let mut archive = Archive::new(GzDecoder::new(tgz));
    let entries = archive.entries().map_err(archive_error)?;
    let mut files = BTreeMap::new();
    let mut total = 0usize;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_ENTRIES {
            return Err(SourceError::TooLarge);
        }
        let mut entry = entry.map_err(archive_error)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(archive_error)?;
        let Some(name) = path_name(&path) else {
            continue;
        };
        if !wanted(&name) {
            continue;
        }
        if entry.header().size().map_err(archive_error)? > MAX_ENTRY {
            return Err(SourceError::TooLarge);
        }
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(MAX_ENTRY + 1)
            .read_to_end(&mut bytes)
            .map_err(archive_error)?;
        if bytes.len() as u64 > MAX_ENTRY {
            return Err(SourceError::TooLarge);
        }
        total = total
            .checked_add(bytes.len())
            .ok_or(SourceError::TooLarge)?;
        if total > MAX_TOTAL {
            return Err(SourceError::TooLarge);
        }
        files.insert(name, bytes);
    }
    Ok(files)
}

/// Returns the archive key for a path, dropping any leading `./` and refusing non-UTF-8.
fn path_name(path: &Path) -> Option<String> {
    let mut name = path.to_str()?;
    while let Some(rest) = name.strip_prefix("./") {
        name = rest;
    }
    (!name.is_empty()).then(|| name.to_owned())
}

/// Maps a gzip or tar I/O failure into the source error.
fn archive_error(error: std::io::Error) -> SourceError {
    SourceError::Archive(error.to_string())
}
