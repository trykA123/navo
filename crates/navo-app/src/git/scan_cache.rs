use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use navo_ui::git::{BadgeKind, BadgeReason, RepoBadge, RepoRoot};

use super::{CACHE_BYTES, CACHE_CAP};

pub(super) type State = (BadgeKind, Option<BadgeReason>);

#[derive(Clone)]
pub(super) struct Cached {
    pub root: RepoBadge,
    /// The repository folder's name.
    pub root_name: String,
    pub dirs: BTreeMap<String, State>,
    pub stale: bool,
}

impl Cached {
    pub fn row(&self, relative: &str) -> Option<RepoBadge> {
        let mut badge = self.root.clone();
        badge.stale = self.stale;
        if !relative.is_empty() {
            let (kind, reason) = self.dirs.get(relative).copied()?;
            badge.kind = kind;
            badge.reason = reason;
            badge.is_root = false;
            badge.ahead = 0;
            badge.behind = 0;
            badge.changed = 0;
            badge.untracked = 0;
            badge.conflicted = 0;
        }
        Some(badge)
    }

    /// The repository itself, for the menu of a folder inside it.
    pub fn repo(&self) -> RepoRoot {
        let mut badge = self.root.clone();
        badge.stale = self.stale;
        RepoRoot {
            name: self.root_name.clone(),
            badge,
        }
    }

    pub fn bytes(&self) -> usize {
        let text = self.root_name.capacity()
            + self.root.branch.as_ref().map_or(0, String::capacity)
            + self.root.upstream.as_ref().map_or(0, String::capacity);
        text + self
            .dirs
            .keys()
            .map(|dir| dir.capacity() + 256)
            .sum::<usize>()
            + 256
    }
}

#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<(PathBuf, Cached)>,
    bytes: usize,
}

impl Cache {
    pub fn get(&mut self, root: &Path) -> Option<Cached> {
        let index = self.entries.iter().position(|(path, _)| path == root)?;
        let entry = self.entries.remove(index)?;
        let value = entry.1.clone();
        self.entries.push_back(entry);
        Some(value)
    }

    pub fn insert(&mut self, root: PathBuf, mut value: Cached) {
        if let Some(index) = self.entries.iter().position(|(path, _)| path == &root) {
            self.remove(index);
        }
        let path_bytes = root.as_os_str().len() * 2;
        while value.bytes() + path_bytes > CACHE_BYTES && !value.dirs.is_empty() {
            value.dirs.pop_last();
        }
        let bytes = value.bytes() + path_bytes;
        if bytes > CACHE_BYTES {
            return;
        }
        while self.entries.len() >= CACHE_CAP || self.bytes + bytes > CACHE_BYTES {
            self.remove(0);
        }
        self.bytes += bytes;
        self.entries.push_back((root, value));
    }

    fn remove(&mut self, index: usize) {
        if let Some((root, value)) = self.entries.remove(index) {
            self.bytes -= value.bytes() + root.as_os_str().len() * 2;
        }
    }
}

#[cfg(test)]
#[path = "scan_tests/cache.rs"]
mod tests;
