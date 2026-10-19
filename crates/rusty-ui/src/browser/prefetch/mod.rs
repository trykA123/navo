use std::sync::Arc;
use std::sync::mpsc::Sender;

use rusty_core::{Location, ProviderPath};
use rusty_fs::{JobSlot, OwnerGuard, Priority, volume_of};

use super::cached::Fetch;
use super::offline::Progress;
use super::{Browser, Message, Waker};

const IN_FLIGHT: usize = 2;

struct Running {
    id: u64,
    location: Location,
    owner: Option<OwnerGuard>,
}

#[derive(Default)]
pub(super) struct Prefetches {
    running: Vec<Running>,
    waiting: Option<Location>,
    next_id: u64,
}

struct Finished {
    id: u64,
    sender: Sender<Message>,
    wake: Waker,
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
        let tab = self.state.active_tab();
        let entry = tab.listing()?.get(tab.view().cursor)?;
        if !entry.is_folder || entry.name == ".." {
            return None;
        }
        tab.location()
            .join(&ProviderPath::from_text(&entry.name))
            .ok()
    }

    pub fn leave_prefetches(&mut self, kept: Option<&Location>) {
        let prefetches = &mut self.prefetches;
        prefetches.waiting = prefetches
            .waiting
            .take()
            .filter(|waiting| Some(waiting) == kept);
        prefetches
            .running
            .iter_mut()
            .filter(|running| Some(&running.location) != kept)
            .for_each(|running| running.owner = None);
    }

    pub fn prefetch(&mut self, location: Location) {
        if !self.wants_prefetch(&location) {
            return;
        }
        if self.prefetches.running.len() >= IN_FLIGHT {
            self.prefetches.waiting = Some(location);
            return;
        }
        self.start_prefetch(location);
    }

    #[must_use]
    pub fn prefetching(&self) -> usize {
        self.prefetches.running.len()
    }

    pub(super) fn prefetched(&mut self, id: u64) -> bool {
        self.prefetches.running.retain(|running| running.id != id);
        if let Some(location) = self.prefetches.waiting.take() {
            self.prefetch(location);
        }
        false
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
        let owner = self.queue.owner();
        let fetch = Fetch {
            provider: Arc::clone(&self.provider),
            cache: self.cache.clone(),
            keys: self.keys.clone(),
            location: location.clone(),
            shown: None,
            touched: None,
            is_watched: false,
            progress: Progress::default(),
        };
        let finished = Finished {
            id,
            sender: self.sender.clone(),
            wake: Arc::clone(&self.wake),
        };
        let slot = JobSlot {
            volume: volume_of(&location),
            priority: Priority::Prefetch,
            owner: owner.id(),
        };
        self.prefetches.running.push(Running {
            id,
            location,
            owner: Some(owner),
        });
        self.queue.submit(
            slot,
            Box::new(move |cancel| {
                let _finished = finished;
                let _ = fetch.run(cancel);
            }),
        );
    }
}

#[cfg(test)]
mod tests;
