//! Cache eviction (NC223 rule 6), run before each fetch: keeps the fetching key plus the 15 most
//! recently used others within 2 GiB − `max_bytes`, touching only NC223 names.

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use super::names::{self, Names};

/// The whole cache stays within this many bytes after a fetch of at most `max_bytes`.
const CACHE_BYTES: u64 = 2 << 30;
/// Keys kept besides the fetching one.
const OTHERS: usize = 15;

/// What one eviction did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EvictReport {
    /// Keys whose repository went to trash.
    pub evicted: usize,
    /// Keys skipped because another handle holds their lock or their size walk failed; they
    /// count against the 15.
    pub busy: usize,
    /// Bytes of the repositories still in the cache (links count 0).
    pub bytes: u64,
}

/// Evicts least recently used keys under `root` so `keep_key` (whose lock the caller holds)
/// plus at most 15 others fit in 2 GiB − `max_bytes`. Fails when the root or the fetching key's
/// size walk fails; another key that cannot be measured is kept and counted as busy.
pub fn evict(root: &Path, keep_key: &str, max_bytes: u64) -> io::Result<EvictReport> {
    let keys = scan(root)?;
    let mut report = EvictReport {
        bytes: size(&Names::new(root, keep_key.to_owned()).live())?,
        ..EvictReport::default()
    };
    let mut held = Vec::new();
    for key in keys.into_iter().filter(|key| key != keep_key) {
        let names = Names::new(root, key);
        let Some(lock) = try_lock(&names.lock()) else {
            report.busy += 1;
            report.bytes += size(&names.live()).unwrap_or(0);
            continue;
        };
        names.remove_staging();
        // An unmeasured key is kept (busy), so one unreadable entry never stops the eviction.
        let Ok(bytes) = size(&names.live()) else {
            report.busy += 1;
            continue;
        };
        let used = std::fs::symlink_metadata(names.used()).and_then(|meta| meta.modified());
        held.push((used.ok(), bytes, names.key().to_owned(), lock));
    }
    // Newest `.used` first; a missing one (`None`) sorts last, as the oldest.
    held.sort_by_key(|one| std::cmp::Reverse(one.0));
    let budget = CACHE_BYTES.saturating_sub(max_bytes);
    let mut room = OTHERS.saturating_sub(report.busy);
    let mut full = false;
    // Each key's lock (`_lock`) is held until its turn here ends.
    for (_, bytes, key, _lock) in held {
        full = full || room == 0 || report.bytes + bytes > budget;
        if !full {
            room -= 1;
            report.bytes += bytes;
        } else if drop_key(&Names::new(root, key)) {
            report.evicted += 1;
        } else {
            report.bytes += bytes;
        }
    }
    Ok(report)
}

/// Removes every `<key>.trash-<16 hex>` under `root` (no lock: nothing opens a trash name) and
/// returns the keys that have a repository, a `.used` mark or a staging folder.
fn scan(root: &Path) -> io::Result<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    // An entry that cannot be read is skipped, never fatal (fail closed: its key is not touched).
    for entry in std::fs::read_dir(root)?.filter_map(Result::ok) {
        let (entry, name) = (entry.path(), entry.file_name());
        let split = name.to_str().and_then(|name| name.split_once('.'));
        let Some((key, suffix)) = split.filter(|(key, _)| is_hex(key, 32)) else {
            continue;
        };
        let tail = |p| suffix.strip_prefix(p).is_some_and(|hex| is_hex(hex, 16));
        if tail("trash-") {
            names::remove_any(&entry).ok();
        } else if matches!(suffix, "git" | "used") || tail("tmp-") {
            keys.insert(key.to_owned());
        }
    }
    Ok(keys)
}

/// True for exactly `len` lower-case hex digits.
fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Opens `lock` (create, never truncate) and takes it without waiting; `None` when it is busy
/// or any step fails, so an unsure key is treated as busy and kept.
fn try_lock(lock: &Path) -> Option<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock)
        .ok()?;
    file.try_lock().ok()?;
    Some(file)
}

/// Bytes of the regular files under `path`, walked with `symlink_metadata` so a link (or a
/// Windows junction) counts 0 and is never followed. Entries that vanish mid-walk count 0.
fn size(path: &Path) -> io::Result<u64> {
    let mut total = 0;
    let mut stack = vec![path.to_owned()];
    while let Some(next) = stack.pop() {
        let meta = match std::fs::symlink_metadata(&next) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            other => other?,
        };
        if meta.is_dir() {
            let entries = match std::fs::read_dir(&next) {
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                other => other?,
            };
            for entry in entries {
                stack.push(entry?.path());
            }
        } else if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}

/// Trashes `<key>.git` (rule 4), then removes `<key>.used`; the `.lock` stays (rule 3). False
/// when the repository is still there (a failed rename, as on Windows with mapped packs).
fn drop_key(names: &Names<'_>) -> bool {
    let live = names.live();
    match std::fs::symlink_metadata(&live) {
        Ok(_) if names.trash(&live).is_err() => return false,
        Err(err) if err.kind() != io::ErrorKind::NotFound => return false,
        _ => {}
    }
    std::fs::remove_file(names.used()).ok();
    true
}
