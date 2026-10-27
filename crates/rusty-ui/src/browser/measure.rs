use std::sync::Arc;

use rusty_core::{Location, ProviderPath, TabId};
use rusty_fs::{JobKind, JobSlot, Priority, folder_size, volume_of};

use super::{Browser, Message};

pub(crate) struct Measured {
    pub(crate) tab: TabId,
    pub(crate) folder: Location,
    pub(crate) name: String,
    pub(crate) bytes: u64,
}

impl Browser {
    pub fn set_space_measures_folders(&mut self, measures: bool) {
        self.measures_folders = measures;
    }

    pub(super) fn measure(&mut self, tab: TabId, folder: Location, name: String) {
        let Ok(target) = folder.join(&ProviderPath::from_text(&name)) else {
            return;
        };
        let (provider, sender, wake) = (
            Arc::clone(&self.provider),
            self.sender.clone(),
            Arc::clone(&self.wake),
        );
        let slot = JobSlot {
            volume: volume_of(&target),
            priority: Priority::Idle,
            owner: self.sizes_owner.id(),
        };
        self.in_flight += 1;
        self.pools.queue(JobKind::Size).submit(
            slot,
            Box::new(move |cancel| {
                let bytes = folder_size(provider.as_ref(), &target, cancel).map_or(0, |s| s.bytes);
                let _ = sender.send(Message::Measured(Measured {
                    tab,
                    folder,
                    name,
                    bytes,
                }));
                wake();
            }),
        );
    }

    pub(crate) fn show_folder_size(&mut self, measured: &Measured) -> bool {
        let here = self
            .state
            .tab(measured.tab)
            .is_some_and(|tab| tab.location() == &measured.folder);
        if !here {
            return false;
        }
        self.remember_size(measured.tab, &measured.name, measured.bytes);
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
        entry.size = measured.bytes;
        self.details.insert(measured.tab, rows.into());
        true
    }
}
