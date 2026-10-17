use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusty_cache::{CacheKey, CacheKeys, CachedListing, Change, ListingCache};
use rusty_core::{Event, FsError, FsErrorKind, Listing, Location, ProviderPath, TabId};
use rusty_fs::{CancelToken, FsEntry, ProviderOps, WatchChange, load_folder_reporting};

use super::Browser;
use super::filter::Full;
use super::offline::Progress;
use crate::file_list::SortKey;

const KEEP_SORTED: usize = 2;

pub(super) enum Fresh {
    Rows(Arc<[FsEntry]>),
    Unchanged,
}

pub(super) struct Fetch {
    pub(super) provider: Arc<dyn ProviderOps>,
    pub(super) cache: ListingCache,
    pub(super) keys: CacheKeys,
    pub(super) location: Location,
    pub(super) shown: Option<Arc<CachedListing>>,
    pub(super) touched: Option<Vec<WatchChange>>,
    pub(super) is_watched: bool,
    pub(super) progress: Progress,
}

impl Fetch {
    pub(super) fn run(self, cancel: &CancelToken) -> Result<Fresh, FsError> {
        let read_at = SystemTime::now();
        let folder = self.provider.metadata(&self.location).ok();
        if folder.is_some() {
            self.progress.answered();
        }
        let folder_changed = folder
            .and_then(|folder| folder.modified_ms)
            .and_then(|ms| u64::try_from(ms).ok())
            .map(|ms| UNIX_EPOCH + Duration::from_millis(ms));
        if self.is_watched
            && folder_changed.is_some()
            && self
                .shown
                .as_ref()
                .is_some_and(|shown| shown.folder_changed == folder_changed)
        {
            return Ok(Fresh::Unchanged);
        }
        if let Some(rows) = self
            .touched
            .as_deref()
            .and_then(|changes| self.patched(changes))
        {
            return Ok(Fresh::Rows(rows));
        }
        let progress = &self.progress;
        let snapshot =
            load_folder_reporting(self.provider.as_ref(), &self.location, cancel, &mut || {
                progress.answered()
            })?;
        let rows: Arc<[FsEntry]> = snapshot.details.into();
        let unchanged = self
            .shown
            .filter(|shown| shown.rows[..] == rows[..])
            .map(|shown| Arc::clone(&shown.rows));
        if let Some(key) = self.keys.key(&self.location) {
            let listing = CachedListing {
                rows: unchanged.clone().unwrap_or_else(|| Arc::clone(&rows)),
                read_at,
                folder_changed,
                complete: true,
            };
            self.cache.put(key.clone(), listing);
            self.cache.persist(&key);
        }
        Ok(match unchanged {
            Some(_) => Fresh::Unchanged,
            None => Fresh::Rows(rows),
        })
    }

    fn patched(&self, changes: &[WatchChange]) -> Option<Arc<[FsEntry]>> {
        let key = self.keys.key(&self.location)?;
        self.cache.get(&key)?;
        let changed_at = SystemTime::now();
        let mut patch = Vec::with_capacity(changes.len());
        for change in changes {
            match change {
                WatchChange::Removed(name) => patch.push(Change::Removed(name.clone())),
                WatchChange::Renamed { from, to } => {
                    patch.push(Change::Removed(from.clone()));
                    patch.push(self.stat(to)?);
                }
                WatchChange::Created(name) | WatchChange::Modified(name) => {
                    patch.push(self.stat(name)?);
                }
            }
        }
        self.cache.patch(&key, changed_at, patch);
        self.cache
            .get(&key)
            .map(|listing| Arc::clone(&listing.rows))
    }

    fn stat(&self, name: &ProviderPath) -> Option<Change> {
        let location = self.location.join(name).ok()?;
        match self.provider.metadata(&location) {
            Ok(entry) => Some(Change::Modified(entry)),
            Err(error) if error.kind == FsErrorKind::NotFound => {
                Some(Change::Removed(name.clone()))
            }
            Err(_) => None,
        }
    }
}

pub(super) struct Sorted {
    source: Arc<[FsEntry]>,
    key: SortKey,
    full: Full,
}

impl Browser {
    pub fn set_listing_cache(&mut self, cache: ListingCache, keys: CacheKeys) {
        self.release_pins();
        self.cache = cache;
        self.keys = keys;
        self.sync_pins();
    }

    #[must_use]
    pub fn is_cached(&self, tab: TabId) -> bool {
        self.cached.get(&tab).is_some_and(|request| {
            self.state
                .tab(tab)
                .is_some_and(|t| t.pending_request() == Some(*request))
        })
    }

    pub(super) fn fetch(&mut self, tab: TabId, location: &Location, request: u64) -> Fetch {
        let is_arriving = self.state.tab(tab).is_some_and(|t| t.listing().is_none());
        let shown = is_arriving
            .then(|| self.keys.key(location))
            .flatten()
            .and_then(|key| self.cache.get(&key))
            .filter(|cached| self.show_cached(tab, request, &cached.rows));
        Fetch {
            provider: Arc::clone(&self.provider),
            cache: self.cache.clone(),
            keys: self.keys.clone(),
            location: location.clone(),
            shown,
            touched: self.take_patch(tab),
            is_watched: self.is_settled(location),
            progress: self.stalls.progress(tab),
        }
    }

    pub(super) fn fresh_listing(
        &mut self,
        tab: TabId,
        location: &Location,
        fresh: Fresh,
    ) -> Listing {
        match fresh {
            Fresh::Rows(rows) => {
                let full = self.sorted_full(tab, &rows);
                let (listing, details) = self.narrow(tab, location, full);
                self.details.insert(tab, details);
                listing
            }
            Fresh::Unchanged => self
                .state
                .tab(tab)
                .and_then(|t| t.listing().cloned())
                .unwrap_or_else(|| Listing::new(Vec::new())),
        }
    }

    fn sorted_full(&mut self, tab: TabId, rows: &Arc<[FsEntry]>) -> Full {
        let key = self.sort_key(tab);
        let memo = self.sorted.entry(tab).or_default();
        if let Some(at) = memo
            .iter()
            .position(|sorted| sorted.key == key && Arc::ptr_eq(&sorted.source, rows))
        {
            let hit = memo.remove(at);
            let full = hit.full.clone();
            memo.insert(0, hit);
            return full;
        }
        let full = self.full_of(tab, rows);
        let memo = self.sorted.entry(tab).or_default();
        memo.insert(
            0,
            Sorted {
                source: Arc::clone(rows),
                key,
                full: full.clone(),
            },
        );
        memo.truncate(KEEP_SORTED);
        full
    }

    fn show_cached(&mut self, tab: TabId, request: u64, rows: &Arc<[FsEntry]>) -> bool {
        let Some(location) = self.state.tab(tab).map(|t| t.location().clone()) else {
            return false;
        };
        let full = self.sorted_full(tab, rows);
        let (listing, details) = self.narrow(tab, &location, full);
        self.details.insert(tab, details);
        self.errors.remove(&tab);
        self.event(Event::ListingCached {
            tab,
            location,
            request,
            listing,
        });
        let is_shown = self.state.tab(tab).is_some_and(|t| t.listing().is_some());
        if is_shown {
            self.cached.insert(tab, request);
            self.restore_focus(tab);
        }
        is_shown
    }

    pub(super) fn sync_pins(&mut self) {
        let wanted: HashSet<CacheKey> = self
            .state
            .window_ids()
            .into_iter()
            .filter_map(|window| self.state.window(window))
            .flat_map(|window| window.panes())
            .map(|pane| pane.active().location())
            .flat_map(|location| std::iter::once(location.clone()).chain(location.parent()))
            .filter_map(|location| self.keys.key(&location))
            .collect();
        if wanted == self.pins {
            return;
        }
        wanted
            .difference(&self.pins)
            .for_each(|key| self.cache.pin(key));
        self.pins
            .difference(&wanted)
            .for_each(|key| self.cache.unpin(key));
        self.pins = wanted;
    }

    pub(super) fn sync_disk_folders(&self) {
        let state = &self.state;
        let tabs = state
            .window_ids()
            .into_iter()
            .filter_map(|window| state.window(window))
            .flat_map(|window| window.panes())
            .flat_map(|pane| pane.tabs());
        self.cache.keep_on_disk(tabs.map(|tab| tab.location()));
    }

    pub(super) fn release_pins(&mut self) {
        let pins = std::mem::take(&mut self.pins);
        pins.iter().for_each(|key| self.cache.unpin(key));
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.release_pins();
        self.release_watches();
    }
}

#[cfg(test)]
mod tests;
