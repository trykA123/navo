use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use rusty_core::{Location, Tab, TabId};
use rusty_fs::{
    CancelToken, FolderSize, JobKind, JobSlot, OwnerId, Priority, Provider, folder_size, volume_of,
};
pub use rusty_index::pipe::FolderTotals;

use super::{Browser, Message};

pub type FolderTotalsQuery = Arc<dyn Fn(&Location) -> Option<FolderTotals> + Send + Sync>;

#[must_use]
pub fn no_folder_totals() -> FolderTotalsQuery {
    Arc::new(|_| None)
}

/// Which number a folder row shows: the file sizes or the space they take on disk.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SizeMode {
    #[default]
    Apparent,
    OnDisk,
}

impl SizeMode {
    /// On disk falls back to the apparent size where the provider cannot tell.
    #[must_use]
    pub fn shown(self, size: &FolderSize) -> u64 {
        if self == Self::OnDisk && size.on_disk_known {
            size.on_disk
        } else {
            size.bytes
        }
    }
}

pub(crate) fn complete_bytes(query: &FolderTotalsQuery, folder: &Location) -> Option<u64> {
    query(folder)
        .filter(|totals| totals.complete)
        .map(|totals| totals.bytes)
}

pub(crate) fn folder_bytes(
    query: &FolderTotalsQuery,
    provider: &dyn Provider,
    folder: &Location,
    cancel: &CancelToken,
) -> Option<u64> {
    folder_measure(query, provider, folder, cancel).map(|size| size.bytes)
}

pub(crate) fn folder_measure(
    query: &FolderTotalsQuery,
    provider: &dyn Provider,
    folder: &Location,
    cancel: &CancelToken,
) -> Option<FolderSize> {
    complete_bytes(query, folder)
        .map(|bytes| FolderSize {
            bytes,
            ..FolderSize::default()
        })
        .or_else(|| folder_size(provider, folder, cancel).ok())
}

pub(crate) struct Measured {
    pub(crate) tab: TabId,
    pub(crate) folder: Location,
    pub(crate) name: String,
    pub(crate) size: Option<FolderSize>,
}

/// Folder size work per tab, the size mode and the last sizes, to reshow them when the mode changes.
#[derive(Default)]
pub(super) struct SizeView {
    pub(super) mode: SizeMode,
    pub(super) work: HashMap<TabId, SizeWork>,
    pub(super) remembered: HashMap<TabId, Remembered>,
}

pub(super) struct Remembered {
    folder: Location,
    sizes: HashMap<String, FolderSize>,
}

pub(super) struct SizeWork {
    location: Location,
    pending: usize,
}

impl Browser {
    pub fn set_space_measures_folders(&mut self, measures: bool) {
        self.measures_folders = measures;
    }

    pub fn set_size_mode(&mut self, mode: SizeMode) {
        if self.sizes.mode == mode {
            return;
        }
        self.sizes.mode = mode;
        let tabs: Vec<TabId> = self.sizes.remembered.keys().copied().collect();
        for tab in tabs {
            self.reshow_sizes(tab);
        }
    }

    pub fn set_folder_totals(&mut self, query: FolderTotalsQuery) {
        self.folder_totals = query;
    }

    pub(super) fn retain_size_owners(&mut self) {
        self.size_owners
            .retain(|tab, _| self.state.tab(*tab).is_some());
        self.sizes
            .work
            .retain(|tab, _| self.size_owners.contains_key(tab));
        self.sizes
            .remembered
            .retain(|tab, _| self.state.tab(*tab).is_some());
    }

    pub(super) fn start_size_visit(&mut self, tab: TabId, location: &Location) {
        if self
            .sizes
            .work
            .get(&tab)
            .is_some_and(|w| &w.location == location)
        {
            return;
        }
        self.sizes.remembered.remove(&tab);
        self.size_owners
            .insert(tab, self.pools.queue(JobKind::Size).owner());
        self.sizes.work.insert(
            tab,
            SizeWork {
                location: location.clone(),
                pending: 0,
            },
        );
    }

    pub(super) fn measuring(&self) -> bool {
        self.sizes.work.values().any(|work| work.pending > 0)
    }

    pub(super) fn measure(&mut self, tab: TabId, name: String) {
        let Some(tab) = self.state.tab(tab).cloned() else {
            return;
        };
        self.measure_entry(&tab, name);
    }

    pub(super) fn measure_entry(&mut self, tab: &Tab, name: String) {
        let Ok(target) = tab.entry_location(&name) else {
            return;
        };
        let folder = tab.location().clone();
        let tab = tab.id();
        let (Some(owner), Some(work)) = (self.size_owners.get(&tab), self.sizes.work.get_mut(&tab))
        else {
            return;
        };
        let owner = owner.id();
        let (provider, sender, wake) = (
            Arc::clone(&self.provider),
            self.sender.clone(),
            Arc::clone(&self.wake),
        );
        let totals = Arc::clone(&self.folder_totals);
        let slot = JobSlot {
            volume: volume_of(&target),
            priority: Priority::Idle,
            owner,
        };
        work.pending += 1;
        self.pools.queue(JobKind::Size).submit_then(
            slot,
            move |cancel| {
                let size = catch_unwind(AssertUnwindSafe(|| {
                    folder_measure(&totals, provider.as_ref(), &target, cancel).unwrap_or_default()
                }))
                .ok();
                Measured {
                    tab,
                    folder,
                    name,
                    size,
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
        if let Some(work) = self.sizes.work.get_mut(&measured.tab) {
            work.pending = work.pending.saturating_sub(1);
        }
        self.show_folder_size(measured)
    }

    pub(crate) fn show_folder_size(&mut self, measured: &Measured) -> bool {
        let here = self
            .state
            .tab(measured.tab)
            .is_some_and(|tab| tab.location() == &measured.folder);
        let (true, Some(size)) = (here, measured.size) else {
            return false;
        };
        let remembered = self
            .sizes
            .remembered
            .entry(measured.tab)
            .or_insert_with(|| Remembered {
                folder: measured.folder.clone(),
                sizes: HashMap::new(),
            });
        if remembered.folder != measured.folder {
            remembered.folder = measured.folder.clone();
            remembered.sizes.clear();
        }
        remembered.sizes.insert(measured.name.clone(), size);
        self.show_size_of(measured.tab, &measured.name, self.sizes.mode.shown(&size))
    }

    fn reshow_sizes(&mut self, tab: TabId) {
        let Some(remembered) = self.sizes.remembered.get(&tab) else {
            return;
        };
        let here = self
            .state
            .tab(tab)
            .is_some_and(|t| t.location() == &remembered.folder);
        if !here {
            return;
        }
        let shown: Vec<(String, u64)> = remembered
            .sizes
            .iter()
            .map(|(name, size)| (name.clone(), self.sizes.mode.shown(size)))
            .collect();
        for (name, bytes) in shown {
            self.show_size_of(tab, &name, bytes);
        }
    }

    fn show_size_of(&mut self, tab: TabId, name: &str, bytes: u64) -> bool {
        self.remember_size(tab, name, bytes);
        let Some(tab_state) = self.state.tab(tab) else {
            return false;
        };
        let Some(row) = tab_state
            .listing()
            .and_then(|l| l.entries().iter().position(|e| e.name == name))
        else {
            return false;
        };
        let Some(details) = self.details.get(&tab) else {
            return false;
        };
        let mut rows = details.to_vec();
        let Some(entry) = rows.get_mut(row) else {
            return false;
        };
        entry.size = bytes;
        self.details.insert(tab, rows.into());
        true
    }
}
