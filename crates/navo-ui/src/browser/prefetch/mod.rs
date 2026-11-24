use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};

use navo_cache::{CacheKeys, ListingCache};
use navo_core::{FsError, FsErrorKind, Location, Tab, TabId};
use navo_fs::{CancelToken, JobKind, JobSlot, OwnerGuard, Priority, volume_of};

use super::cached::{Fetch, Fresh};
use super::offline::Progress;
use super::{Browser, Loaded, Message, Waker};

const IN_FLIGHT: usize = 2;

#[derive(Default)]
enum Hand {
    #[default]
    Open,
    Adopted {
        tab: TabId,
        request: u64,
    },
    Closed,
}

#[derive(Default)]
struct Handoff(Mutex<Hand>);

impl Handoff {
    fn adopt(&self, tab: TabId, request: u64) -> bool {
        let mut hand = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let is_open = matches!(*hand, Hand::Open);
        if is_open {
            *hand = Hand::Adopted { tab, request };
        }
        is_open
    }

    fn close(&self) -> Option<(TabId, u64)> {
        let mut hand = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match std::mem::replace(&mut *hand, Hand::Closed) {
            Hand::Adopted { tab, request } => Some((tab, request)),
            Hand::Open | Hand::Closed => None,
        }
    }
}

struct Running {
    id: u64,
    location: Location,
    owner: Option<OwnerGuard>,
    is_adopted: bool,
    handoff: Arc<Handoff>,
    progress: Progress,
}

impl Running {
    fn is_live(&self) -> bool {
        self.owner.is_some() && !self.is_adopted
    }
}

#[derive(Default)]
pub(super) struct Prefetches {
    running: Vec<Running>,
    next_id: u64,
}

struct Finished {
    id: u64,
    handoff: Arc<Handoff>,
    location: Location,
    cache: ListingCache,
    keys: CacheKeys,
    sender: Sender<Message>,
    wake: Waker,
}

impl Finished {
    /// Gives the result to the tab that adopted this prefetch, if any; the cache keeps it on disk.
    fn hand_over(self, result: Result<Fresh, FsError>) {
        if let Some((tab, request)) = self.handoff.close() {
            if let Some(key) = self.keys.key(&self.location) {
                self.cache.persist(&key);
            }
            self.send_loaded(tab, request, result);
        }
    }

    /// Sends the adopted request's `Loaded`, which also lowers the browser's in-flight count.
    fn send_loaded(&self, tab: TabId, request: u64, result: Result<Fresh, FsError>) {
        let location = self.location.clone();
        let loaded = Loaded {
            tab,
            location,
            request,
            result,
        };
        let _ = self.sender.send(Message::Loaded(loaded));
    }
}

impl Drop for Finished {
    fn drop(&mut self) {
        // An adopted job dropped unrun (cancelled while queued) still answers its listing.
        if let Some((tab, request)) = self.handoff.close() {
            self.send_loaded(tab, request, Err(FsError::new(FsErrorKind::Cancelled)));
        }
        let _ = self.sender.send(Message::Prefetched(self.id));
        (self.wake)();
    }
}

impl Browser {
    #[must_use]
    pub fn cursor_folder(&self) -> Option<Location> {
        cursor_folder(self.state.active_tab())
    }

    pub fn leave_prefetches(&mut self, kept: Option<&Location>) {
        self.prefetches
            .running
            .iter_mut()
            .filter(|running| !running.is_adopted && Some(&running.location) != kept)
            .for_each(|running| running.owner = None);
    }

    pub fn prefetch(&mut self, location: Location) -> bool {
        let running = &self.prefetches.running;
        let is_wanted = running.iter().filter(|r| r.is_live()).count() < IN_FLIGHT
            && self.wants_prefetch(&location);
        if is_wanted {
            self.start_prefetch(location);
        }
        is_wanted
    }

    #[must_use]
    pub fn prefetching(&self) -> usize {
        self.prefetches.running.len()
    }

    pub(super) fn prefetched(&mut self, id: u64) -> bool {
        self.prefetches.running.retain(|running| running.id != id);
        false
    }

    pub(super) fn adopt_prefetch(
        &mut self,
        tab: TabId,
        location: &Location,
        request: u64,
    ) -> Option<Progress> {
        let running = self
            .prefetches
            .running
            .iter_mut()
            .find(|running| running.is_live() && &running.location == location)?;
        running.is_adopted = running.handoff.adopt(tab, request);
        if !running.is_adopted {
            return None;
        }
        if let Some(owner) = running.owner.take() {
            self.listing_owners.hold(tab, request, owner);
        }
        Some(running.progress.clone())
    }

    fn wants_prefetch(&self, location: &Location) -> bool {
        let is_running = self
            .prefetches
            .running
            .iter()
            .any(|r| (r.owner.is_some() || r.is_adopted) && &r.location == location);
        let Some(key) = self.keys.key(location) else {
            return false;
        };
        let is_cached = self.cache.get(&key).is_some_and(|cached| cached.complete);
        !is_running && !is_cached && !self.is_offline_volume(location)
    }

    fn is_offline_volume(&self, location: &Location) -> bool {
        let volume = volume_of(location);
        self.open_tabs()
            .into_iter()
            .any(|(tab, at)| self.offline(tab).is_some() && volume_of(&at) == volume)
    }

    fn start_prefetch(&mut self, location: Location) {
        self.prefetches.next_id += 1;
        let id = self.prefetches.next_id;
        let queue = self.pools.queue(JobKind::Prefetch);
        let owner = queue.owner();
        let (handoff, progress) = (Arc::new(Handoff::default()), Progress::default());
        let fetch = Fetch {
            provider: Arc::clone(&self.provider),
            cache: self.cache.clone(),
            keys: self.keys.clone(),
            location: location.clone(),
            shown: None,
            touched: None,
            is_watched: false,
            is_kept_on_disk: false,
            progress: progress.clone(),
            sort: None,
            sort_count: self.sort_count.clone(),
            early: None,
        };
        let finished = Finished {
            id,
            handoff: Arc::clone(&handoff),
            location: location.clone(),
            cache: self.cache.clone(),
            keys: self.keys.clone(),
            sender: self.sender.clone(),
            wake: Arc::clone(&self.wake),
        };
        let slot = JobSlot {
            volume: volume_of(&location),
            priority: Priority::Prefetch,
            owner: owner.id(),
        };
        let job = Box::new(move |cancel: &CancelToken| finished.hand_over(fetch.run(cancel)));
        self.prefetches.running.push(Running {
            id,
            location,
            owner: Some(owner),
            is_adopted: false,
            handoff,
            progress,
        });
        queue.submit(slot, job);
    }
}

fn cursor_folder(tab: &Tab) -> Option<Location> {
    let entry = tab.listing()?.get(tab.view().cursor)?;
    if !entry.is_folder || entry.name == ".." {
        return None;
    }
    tab.entry_location(&entry.name).ok()
}

#[cfg(test)]
mod tests;
