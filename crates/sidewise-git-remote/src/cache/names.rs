//! The NC223 names of one cache key under the root, the random staging and trash suffix, and
//! the delete-by-rename step (rules 2, 4 and 7).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The paths of one key: `<key>.git`, `.used`, `.lock`, `.tmp-<hex>` and `.trash-<hex>`.
pub(super) struct Names<'a> {
    root: &'a Path,
    key: String,
}

impl<'a> Names<'a> {
    /// The names of `key` (32 lower-case hex digits) under `root`.
    pub(super) fn new(root: &'a Path, key: String) -> Self {
        Self { root, key }
    }

    /// The key itself.
    pub(super) fn key(&self) -> &str {
        &self.key
    }

    /// `<root>/<key>.<suffix>`.
    fn at(&self, suffix: &str) -> PathBuf {
        self.root.join(format!("{}.{suffix}", self.key))
    }

    /// The live repository, `<key>.git`.
    pub(super) fn live(&self) -> PathBuf {
        self.at("git")
    }

    /// The last-use marker, `<key>.used` (only its mtime matters).
    pub(super) fn used(&self) -> PathBuf {
        self.at("used")
    }

    /// The lock file, `<key>.lock`; never deleted (rule 3).
    pub(super) fn lock(&self) -> PathBuf {
        self.at("lock")
    }

    /// A fresh staging name, `<key>.tmp-<16 hex>`.
    pub(super) fn staging(&self) -> PathBuf {
        self.at(&format!("tmp-{}", random_hex()))
    }

    /// Removes this key's leftover staging folders (a crashed fetch); call only with the key lock
    /// held. A folder that will not go stays for the next holder.
    pub(super) fn remove_staging(&self) {
        let Ok(entries) = std::fs::read_dir(self.root) else {
            return;
        };
        let prefix = format!("{}.tmp-", self.key);
        for entry in entries.flatten() {
            let name = entry.file_name();
            let hex = name.to_str().and_then(|n| n.strip_prefix(&prefix));
            if hex.is_some_and(is_hex16) {
                remove_any(&entry.path()).ok();
            }
        }
    }

    /// Moves `path` to `<key>.trash-<16 hex>`, then deletes it (rule 4). Fails only when the
    /// rename fails; a failed delete leaves the trash folder for a later eviction.
    pub(super) fn trash(&self, path: &Path) -> io::Result<()> {
        let trash = self.at(&format!("trash-{}", random_hex()));
        std::fs::rename(path, &trash)?;
        remove_any(&trash).ok();
        Ok(())
    }
}

/// True for exactly 16 lower-case hex digits.
fn is_hex16(text: &str) -> bool {
    text.len() == 16 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Deletes a folder tree, a file or a link (never its target); fails as the OS call fails.
pub(super) fn remove_any(path: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(path)?.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// 16 hex digits: the start of blake3(nanos since epoch, process id, counter); unique, not secret.
fn random_hex() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut hasher = blake3::Hasher::new();
    hasher.update(&nanos.to_le_bytes());
    hasher.update(&std::process::id().to_le_bytes());
    hasher.update(&COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    hasher.finalize().to_hex()[..16].to_owned()
}
