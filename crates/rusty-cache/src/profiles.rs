use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rusty_core::Location;
use rusty_fs::{FolderProfile, ProfileState};

pub const PROFILE_CAPACITY: usize = 256;

struct Slot {
    changed_ms: i64,
    profile: Arc<FolderProfile>,
    used: u64,
}

#[derive(Default)]
struct Profiles {
    slots: HashMap<Location, Slot>,
    clock: u64,
}

#[derive(Clone)]
pub struct ProfileCache {
    capacity: usize,
    profiles: Arc<Mutex<Profiles>>,
}

impl Default for ProfileCache {
    fn default() -> Self {
        Self::new(PROFILE_CAPACITY)
    }
}

impl ProfileCache {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            profiles: Arc::default(),
        }
    }

    #[must_use]
    pub fn get(&self, folder: &Location, changed_ms: i64) -> Option<Arc<FolderProfile>> {
        let mut profiles = self.lock();
        profiles.clock += 1;
        let now = profiles.clock;
        let slot = profiles.slots.get_mut(folder)?;
        if slot.changed_ms == changed_ms {
            slot.used = now;
            return Some(Arc::clone(&slot.profile));
        }
        profiles.slots.remove(folder);
        None
    }

    pub fn put(&self, folder: Location, changed_ms: i64, profile: FolderProfile) {
        if profile.state != ProfileState::Done {
            return;
        }
        let mut profiles = self.lock();
        profiles.clock += 1;
        let used = profiles.clock;
        let slot = Slot {
            changed_ms,
            profile: Arc::new(profile),
            used,
        };
        profiles.slots.insert(folder, slot);
        if profiles.slots.len() > self.capacity {
            profiles.evict_oldest();
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().slots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    fn lock(&self) -> MutexGuard<'_, Profiles> {
        self.profiles.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Profiles {
    fn evict_oldest(&mut self) {
        let oldest = self
            .slots
            .iter()
            .min_by_key(|(_, slot)| slot.used)
            .map(|(folder, _)| folder.clone());
        if let Some(folder) = oldest {
            self.slots.remove(&folder);
        }
    }
}
