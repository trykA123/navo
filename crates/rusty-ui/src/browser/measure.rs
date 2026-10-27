use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use rusty_core::{Location, ProviderPath, TabId};
use rusty_fs::{JobKind, JobSlot, OwnerId, Priority, folder_size, volume_of};

use super::{Browser, Message};

pub(crate) struct Measured {
    pub(crate) tab: TabId,
    pub(crate) folder: Location,
    pub(crate) name: String,
    pub(crate) bytes: Option<u64>,
}

pub(super) struct SizeWork {
    location: Location,
    pending: usize,
}

impl Browser {
    pub fn set_space_measures_folders(&mut self, measures: bool) {
        self.measures_folders = measures;
    }

    pub(super) fn retain_size_owners(&mut self) {
        self.size_owners
            .retain(|tab, _| self.state.tab(*tab).is_some());
        self.size_work
            .retain(|tab, _| self.size_owners.contains_key(tab));
    }

    pub(super) fn start_size_visit(&mut self, tab: TabId, location: &Location) {
        if self
            .size_work
            .get(&tab)
            .is_some_and(|w| &w.location == location)
        {
            return;
        }
        self.size_owners
            .insert(tab, self.pools.queue(JobKind::Size).owner());
        self.size_work.insert(
            tab,
            SizeWork {
                location: location.clone(),
                pending: 0,
            },
        );
    }

    pub(super) fn measuring(&self) -> bool {
        self.size_work.values().any(|work| work.pending > 0)
    }

    pub(super) fn measure(&mut self, tab: TabId, folder: Location, name: String) {
        let Ok(target) = folder.join(&ProviderPath::from_text(&name)) else {
            return;
        };
        let (Some(owner), Some(work)) = (self.size_owners.get(&tab), self.size_work.get_mut(&tab))
        else {
            return;
        };
        let owner = owner.id();
        let (provider, sender, wake) = (
            Arc::clone(&self.provider),
            self.sender.clone(),
            Arc::clone(&self.wake),
        );
        let slot = JobSlot {
            volume: volume_of(&target),
            priority: Priority::Idle,
            owner,
        };
        work.pending += 1;
        self.pools.queue(JobKind::Size).submit_then(
            slot,
            move |cancel| {
                let bytes = catch_unwind(AssertUnwindSafe(|| {
                    folder_size(provider.as_ref(), &target, cancel).map_or(0, |s| s.bytes)
                }))
                .ok();
                Measured {
                    tab,
                    folder,
                    name,
                    bytes,
                }
            },
            move |measured| {
                let _ = sender.send(Message::Measured(owner, measured));
                wake();
            },
        );
    }

    pub(super) fn accept_size(&mut self, owner: OwnerId, measured: &Measured) -> bool {
        if self.size_owners.get(&measured.tab).map(|o| o.id()) != Some(owner) {
            return false;
        }
        if let Some(work) = self.size_work.get_mut(&measured.tab) {
            work.pending = work.pending.saturating_sub(1);
        }
        self.show_folder_size(measured)
    }

    pub(crate) fn show_folder_size(&mut self, measured: &Measured) -> bool {
        let here = self
            .state
            .tab(measured.tab)
            .is_some_and(|tab| tab.location() == &measured.folder);
        let (true, Some(bytes)) = (here, measured.bytes) else {
            return false;
        };
        self.remember_size(measured.tab, &measured.name, bytes);
        let Some(tab) = self.state.tab(measured.tab) else {
            return false;
        };
        let Some(row) = tab
            .listing()
            .and_then(|l| l.entries().iter().position(|e| e.name == measured.name))
        else {
            return false;
        };
        let Some(details) = self.details.get(&measured.tab) else {
            return false;
        };
        let mut rows = details.to_vec();
        let Some(entry) = rows.get_mut(row) else {
            return false;
        };
        entry.size = bytes;
        self.details.insert(measured.tab, rows.into());
        true
    }
}
