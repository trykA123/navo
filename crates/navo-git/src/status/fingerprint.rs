use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::operation::MARKERS;

/// A file written more recently than this cannot be confirmed stable.
const SETTLE: Duration = Duration::from_secs(2);
/// A `HEAD` holds one short reference line or one object id.
const HEAD_MAX: u64 = 256;
const STAMPED: [&str; 7] = [
    "HEAD",
    "index",
    "FETCH_HEAD",
    "config",
    "packed-refs",
    "info/exclude",
    "reftable/tables.list",
];

/// FNV-1a 64 over the files a status result depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint(u64);

/// Fingerprints the stamps a status reads, or `None` when no proof is safe.
pub fn fingerprint(root: &Path, upstream: Option<&str>, now: SystemTime) -> Option<Fingerprint> {
    let git_dir = root.join(".git");
    let metadata = fs::symlink_metadata(&git_dir).ok()?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return None;
    }
    if present(&git_dir.join("commondir"))?
        || present(&git_dir.join("modules"))?
        || present(&root.join(".gitmodules"))?
    {
        return None;
    }
    let cutoff = now.checked_sub(SETTLE)?;
    let mut hasher = Hasher::new();
    for name in STAMPED {
        hasher.stamp(&git_dir.join(name), cutoff)?;
    }
    for (name, _) in MARKERS {
        hasher.stamp(&git_dir.join(name), cutoff)?;
    }
    if let HeadRef::Named(reference) = read_head(&git_dir)? {
        hasher.stamp(&ref_path(&git_dir, &reference)?, cutoff)?;
    }
    if let Some(upstream) = upstream {
        for prefix in ["refs/remotes", "refs/heads"] {
            hasher.stamp(
                &ref_path(&git_dir, &format!("{prefix}/{upstream}"))?,
                cutoff,
            )?;
        }
    }
    Some(Fingerprint(hasher.finish()))
}

enum HeadRef {
    Named(String),
    Detached,
}

fn read_head(git_dir: &Path) -> Option<HeadRef> {
    let mut bytes = Vec::new();
    fs::File::open(git_dir.join("HEAD"))
        .ok()?
        .take(HEAD_MAX)
        .read_to_end(&mut bytes)
        .ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let line = text.trim_end_matches(['\r', '\n']);
    match line.strip_prefix("ref: ") {
        Some(reference) => Some(HeadRef::Named(reference.to_owned())),
        None => Some(HeadRef::Detached),
    }
}

fn present(path: &Path) -> Option<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Some(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

fn ref_path(git_dir: &Path, reference: &str) -> Option<PathBuf> {
    if !safe_ref(reference) {
        return None;
    }
    let mut path = git_dir.to_path_buf();
    for part in reference.split('/') {
        path.push(part);
    }
    Some(path)
}

fn safe_ref(reference: &str) -> bool {
    reference.starts_with("refs/")
        && !reference.contains("..")
        && !reference.contains(['\\', ':'])
        && reference.split('/').all(|part| !part.is_empty())
}

struct Hasher(u64);

impl Hasher {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn stamp(&mut self, path: &Path, cutoff: SystemTime) -> Option<()> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.write(&[0]);
                return Some(());
            }
            Err(_) => return None,
        };
        let modified = metadata.modified().ok()?;
        if modified > cutoff {
            return None;
        }
        let nanos = modified.duration_since(UNIX_EPOCH).ok()?.as_nanos();
        self.write(&[1, u8::from(metadata.is_dir())]);
        self.write(&metadata.len().to_le_bytes());
        self.write(&nanos.to_le_bytes());
        Some(())
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
#[path = "fingerprint/tests.rs"]
mod tests;
