//! Helpers that put a root's cached status into a folder's badges and publish it.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use navo_ui::git::FolderBadges;

use super::cache::{Cache, Cached};
use super::listing::Discovered;
use super::queue::Shared;

/// Publishes one root's kept status as fresh, unless the folder already shows it.
pub(super) fn publish_fresh(
    shared: &Mutex<Shared>,
    found: &Discovered,
    root: &Path,
    cached: &Cached,
) -> Option<u64> {
    let mut shared = shared.lock().unwrap_or_else(PoisonError::into_inner);
    if !shared.is_current(&found.listing) {
        return None;
    }
    let mut badges = shared
        .active
        .get(&found.listing.folder)
        .and_then(|entry| entry.badges.as_deref())
        .cloned()
        .unwrap_or_default();
    let before = badges.clone();
    fill(&mut badges, found, root, cached);
    if badges == before {
        return None;
    }
    shared.publish(&found.listing, badges)
}

/// A cached projection of a scanned root, hidden while it is scanned.
pub(super) fn hidden(cache: &mut Cache, shows: bool, root: &Path) -> Option<(PathBuf, Cached)> {
    if shows {
        return None;
    }
    stale(cache, root)
}

/// A cached projection of a root, marked stale.
pub(super) fn stale(cache: &mut Cache, root: &Path) -> Option<(PathBuf, Cached)> {
    let mut cached = cache.get(root)?;
    cached.stale = true;
    Some((root.to_path_buf(), cached))
}

pub(super) fn fill(badges: &mut FolderBadges, found: &Discovered, root: &Path, cached: &Cached) {
    if found.parent.as_deref() == Some(root) {
        badges.repository = Some(cached.repo());
    }
    for binding in found.bindings.iter().filter(|binding| binding.root == root) {
        if let Some(badge) = cached.row(&binding.relative) {
            if !badge.is_root {
                badges.roots.insert(binding.name.clone(), cached.repo());
            }
            badges.by_name.insert(binding.name.clone(), badge);
        }
    }
}

/// Applies each update to `badges`. A fresh update that changes nothing stays fresh; any other is
/// shown stale.
pub(super) fn apply_updates(
    badges: &mut FolderBadges,
    found: &Discovered,
    updates: &mut [(PathBuf, Cached)],
    confirm: &[PathBuf],
) {
    for (root, cached) in updates {
        if confirm.contains(root) {
            let mut same = badges.clone();
            fill(&mut same, found, root, cached);
            if same == *badges {
                continue;
            }
        }
        cached.stale = true;
        fill(badges, found, root, cached);
    }
}
