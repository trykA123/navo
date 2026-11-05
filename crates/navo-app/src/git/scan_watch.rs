//! Repository watches: one subtree watch per unchanged root, with every raw change counted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use navo_core::FileIdentity;
use navo_fs::DriveClass;

use super::skip::Mark;
use super::{Probe, RepoWatch, WATCH_CAP};

/// One armed watch, its root's parent key, its folder identity, and the epoch it was armed at.
struct Entry {
    watch: Box<dyn RepoWatch>,
    parent: String,
    identity: FileIdentity,
    epoch: u64,
}

/// Watches by root key. The oldest entry goes when a new one brings the count past the cap.
pub(crate) struct Watches {
    probe: Arc<dyn Probe>,
    entries: HashMap<String, Entry>,
    next_epoch: u64,
}

impl Watches {
    pub(crate) fn new(probe: Arc<dyn Probe>) -> Self {
        Self {
            probe,
            entries: HashMap::new(),
            next_epoch: 0,
        }
    }

    /// Arms a subtree watch on a fixed local drive, unless `root` is a volume root or the watch
    /// would block renaming a folder the listing watch leaves free (R3).
    pub(crate) fn arm(&mut self, root: &Path, listing: &Path, class: DriveClass) {
        let Some(parent) = root.parent() else {
            return;
        };
        if !matches!(class, DriveClass::Ssd | DriveClass::Hdd) {
            return;
        }
        let (key, parent) = (root_key(root), root_key(parent));
        if !allowed(&key, &parent, &root_key(listing)) {
            return;
        }
        let Some(identity) = self.probe.identity(root) else {
            self.remove(&key);
            return;
        };
        let Some(watch) = self.probe.watch_tree(root) else {
            self.remove(&key);
            return;
        };
        self.next_epoch += 1;
        let entry = Entry {
            watch,
            parent,
            identity,
            epoch: self.next_epoch,
        };
        if let Some(old) = self.entries.insert(key, entry) {
            release(old.watch);
        }
        if self.entries.len() > WATCH_CAP {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.epoch)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.remove(&oldest);
            }
        }
    }

    /// The epoch and raw change count of the live watch, while the folder identity is unchanged.
    pub(crate) fn mark(&mut self, root: &Path) -> Option<Mark> {
        let key = root_key(root);
        let entry = self.entries.get(&key)?;
        if !entry.watch.is_live() {
            return None;
        }
        let epoch = entry.epoch;
        let changes = entry.watch.changes();
        let stored = entry.identity;
        let current = self.probe.identity(root)?;
        if current != stored {
            self.remove(&key);
            return None;
        }
        Some(Mark { epoch, changes })
    }

    /// Releases every watch that none of the held `listings` would arm (R3).
    pub(crate) fn keep_for(&mut self, listings: &[PathBuf]) {
        let held: Vec<String> = listings.iter().map(|listing| root_key(listing)).collect();
        let gone: Vec<String> = self
            .entries
            .iter()
            .filter(|(key, entry)| !held.iter().any(|l| allowed(key, &entry.parent, l)))
            .map(|(key, _)| key.clone())
            .collect();
        for key in gone {
            self.remove(&key);
        }
    }

    fn remove(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            release(entry.watch);
        }
    }
}

/// Closing a watch can sleep; never hold up the scanner for it.
fn release(watch: Box<dyn RepoWatch>) {
    let _ = thread::Builder::new()
        .name("navo-git-watch-release".to_owned())
        .spawn(move || drop(watch));
}

/// The key for a root: forward slashes, no trailing one; on Windows also without the verbatim
/// `\\?\` prefix and ASCII-lowercased.
pub(crate) fn root_key(path: &Path) -> String {
    key_of(&path.to_string_lossy(), cfg!(windows))
}

fn key_of(text: &str, windows: bool) -> String {
    let unc = text.strip_prefix(r"\\?\UNC\");
    let text = match (windows, unc) {
        (false, _) => text.to_owned(),
        (true, Some(share)) => format!(r"\\{share}"),
        (true, None) => text.strip_prefix(r"\\?\").unwrap_or(text).to_owned(),
    };
    let mut key = text.replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') && !key.ends_with(":/") {
        key.pop();
    }
    if windows {
        key.make_ascii_lowercase();
    }
    key
}

/// Whether a watch on `key` blocks no folder the listing watch leaves free: the root is the
/// listing, an ancestor of it, or a direct child of it.
fn allowed(key: &str, parent: &str, listing: &str) -> bool {
    parent == listing || contains(key, listing)
}

/// Whether the folder keyed `deep` is the folder keyed `top` or lies inside it.
pub(crate) fn contains(top: &str, deep: &str) -> bool {
    deep.strip_prefix(top)
        .is_some_and(|rest| rest.is_empty() || top.ends_with('/') || rest.starts_with('/'))
}

#[cfg(test)]
#[path = "scan_tests/watch.rs"]
mod tests;
