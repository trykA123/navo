use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};

use rusty_cache::{CacheKeys, ListingCache};
use rusty_core::{FsError, Location, Tab, TabId};
use rusty_fs::{CancelToken, JobKind, JobSlot, OwnerGuard, Priority, volume_of};

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
    cache: ListingCache,
    keys: CacheKeys,
    sender: Sender<Message>,
    wake: Waker,
}

impl Finished {
    fn hand_over(self, handoff: &Handoff, location: Location, result: Result<Fresh, FsError>) {
        if let Some((tab, request)) = handoff.close() {
            if let Some(key) = self.keys.key(&location) {
                self.cache.persist(&key);
            }
            let loaded = Loaded {
                tab,
                location,
                request,
                result,
            };
            let _ = self.sender.send(Message::Loaded(loaded));
        }
    }
}

impl Drop for Finished {
    fn drop(&mut self) {
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
        running.is_adopted.then(|| running.progress.clone())
    }

    fn wants_prefetch(&self, location: &Location) -> bool {
        let is_running = self
            .prefetches
            .running
            .iter()
            .any(|running| running.owner.is_some() && &running.location == location);
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
        };
        let finished = Finished {
            id,
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
        let job = Box::new({
            let (handoff, location) = (Arc::clone(&handoff), location.clone());
            move |cancel: &CancelToken| {
                let result = fetch.run(cancel);
                finished.hand_over(&handoff, location, result);
            }
        });
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
