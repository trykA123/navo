use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusty_cache::{CacheKey, CacheKeys, CachedListing, ListingCache};
use rusty_core::{Event, FsError, Location, TabId};
use rusty_fs::{CancelToken, FsEntry, ProviderOps, load_folder};

use super::Browser;

const FRESH_FOR: Duration = Duration::from_secs(2);

pub(super) struct Fetch {
    pub(super) provider: Arc<dyn ProviderOps>,
    pub(super) cache: ListingCache,
    pub(super) key: CacheKey,
    pub(super) location: Location,
    pub(super) shown: Option<Arc<CachedListing>>,
}

impl Fetch {
    pub(super) fn run(self, cancel: &CancelToken) -> Result<Arc<[FsEntry]>, FsError> {
        let read_at = SystemTime::now();
        let folder_changed = self
            .provider
            .metadata(&self.location)
            .ok()
            .and_then(|folder| folder.modified_ms)
            .and_then(|ms| u64::try_from(ms).ok())
            .map(|ms| UNIX_EPOCH + Duration::from_millis(ms));
        if let Some(shown) = self
            .shown
            .filter(|shown| is_unchanged(shown, folder_changed, read_at))
        {
            return Ok(Arc::clone(&shown.rows));
        }
        let snapshot = load_folder(self.provider.as_ref(), &self.location, cancel)?;
        let rows: Arc<[FsEntry]> = snapshot.details.into();
        let listing = CachedListing {
            rows: Arc::clone(&rows),
            read_at,
            folder_changed,
            complete: true,
        };
        self.cache.put(self.key, listing);
        Ok(rows)
    }
}

fn is_unchanged(shown: &CachedListing, changed: Option<SystemTime>, now: SystemTime) -> bool {
    shown.complete
        && changed.is_some()
        && shown.folder_changed == changed
        && now
            .duration_since(shown.read_at)
            .is_ok_and(|age| age < FRESH_FOR)
}

#[derive(Default)]
pub(super) struct Pins {
    keys: HashSet<CacheKey>,
    shown: Vec<Location>,
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
        let key = self.keys.key(location);
        let is_arriving = self.state.tab(tab).is_some_and(|t| t.listing().is_none());
        let shown = is_arriving
            .then(|| self.cache.get(&key))
            .flatten()
            .filter(|cached| self.show_cached(tab, request, &cached.rows));
        Fetch {
            provider: Arc::clone(&self.provider),
            cache: self.cache.clone(),
            key,
            location: location.clone(),
            shown,
        }
    }

    fn show_cached(&mut self, tab: TabId, request: u64, rows: &[FsEntry]) -> bool {
        let Some(location) = self.state.tab(tab).map(|t| t.location().clone()) else {
            return false;
        };
        let full = self.full_of(tab, rows);
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
        let shown: Vec<Location> = self
            .state
            .window_ids()
            .into_iter()
            .filter_map(|window| self.state.window(window))
            .flat_map(|window| window.panes())
            .map(|pane| pane.active().location().clone())
            .collect();
        if shown == self.pins.shown {
            return;
        }
        let wanted: HashSet<CacheKey> = shown
            .iter()
            .flat_map(|location| std::iter::once(location.clone()).chain(location.parent()))
            .map(|location| self.keys.key(&location))
            .collect();
        wanted
            .difference(&self.pins.keys)
            .for_each(|key| self.cache.pin(key));
        self.pins
            .keys
            .difference(&wanted)
            .for_each(|key| self.cache.unpin(key));
        self.pins = Pins {
            keys: wanted,
            shown,
        };
    }

    pub(super) fn release_pins(&mut self) {
        let pins = std::mem::take(&mut self.pins);
        pins.keys.iter().for_each(|key| self.cache.unpin(key));
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.release_pins();
    }
}

#[cfg(test)]
mod tests;
