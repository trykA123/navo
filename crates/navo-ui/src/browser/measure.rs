use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use navo_core::{Location, Tab, TabId};
use navo_fs::{
    CancelToken, FolderSize, JobId, JobKind, JobSlot, OwnerGuard, OwnerId, Priority, Provider,
    folder_size, volume_of,
};
pub use navo_index::pipe::FolderTotals;

use super::{Browser, Message};

pub type FolderTotalsQuery = Arc<dyn Fn(&Location) -> Option<FolderTotals> + Send + Sync>;

#[must_use]
pub fn no_folder_totals() -> FolderTotalsQuery {
    Arc::new(|_: &navo_core::Location| None)
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
        .map(FolderSize::apparent_only)
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
    pub(super) shown: super::shown_sizes::ShownSizes,
}

pub(super) struct Remembered {
    pub(super) folder: Location,
    pub(super) sizes: HashMap<String, FolderSize>,
}

/// Who asked for a size: a Space press, the shown-folder sizes (D7), or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Asker {
    Space,
    Shown,
    ShownThenSpace,
}

pub(super) struct Asked {
    job: JobId,
    pub(super) asker: Asker,
}

pub(super) struct SizeWork {
    pub(super) location: Location,
    pub(super) pending: usize,
    pub(super) shown_planned: bool,
    pub(super) shown_owner: Option<OwnerGuard>,
    pub(super) shown_pending: usize,
    pub(super) asked: HashMap<String, Asked>,
}

impl Asker {
    fn priority(self) -> Priority {
        if self == Self::Space {
            Priority::Visible
        } else {
            Priority::Idle
        }
    }
}

impl SizeWork {
    fn count_asked(&mut self, asker: Asker) {
        if asker == Asker::Space {
            self.pending += 1;
        } else {
            self.shown_pending += 1;
        }
    }

    fn owner_for(&self, asker: Asker, space: &OwnerGuard) -> Option<OwnerId> {
        match asker {
            Asker::Space => Some(space.id()),
            Asker::Shown | Asker::ShownThenSpace => self.shown_owner.as_ref().map(OwnerGuard::id),
        }
    }

    pub(super) fn owns(&self, owner: OwnerId) -> bool {
        self.shown_owner.as_ref().map(OwnerGuard::id) == Some(owner)
    }
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
                shown_planned: false,
                shown_owner: None,
                shown_pending: 0,
                asked: HashMap::new(),
            },
        );
    }

    pub(super) fn measuring(&self) -> bool {
        self.sizes
            .work
            .values()
            .any(|work| work.pending + work.shown_pending > 0)
    }

    pub(super) fn measure(&mut self, tab: TabId, name: String) {
        let Some(tab) = self.state.tab(tab).cloned() else {
            return;
        };
        if !self.promote(tab.id(), &name) {
            self.measure_entry(&tab, name, Asker::Space);
        }
    }

    /// A Space on a folder the shown sizes already asked for moves that job ahead instead of walking twice.
    fn promote(&mut self, tab: TabId, name: &str) -> bool {
        let Some(asked) = self
            .sizes
            .work
            .get_mut(&tab)
            .and_then(|work| work.asked.get_mut(name))
        else {
            return false;
        };
        if asked.asker == Asker::Shown {
            asked.asker = Asker::ShownThenSpace;
        }
        let _ = self
            .pools
            .queue(JobKind::Size)
            .reprioritise(asked.job, Priority::Visible);
        true
    }

    pub(super) fn measure_entry(&mut self, tab: &Tab, name: String, asker: Asker) {
        let Ok(target) = tab.entry_location(&name) else {
            return;
        };
        let folder = tab.location().clone();
        let tab = tab.id();
        let (Some(space), Some(work)) = (self.size_owners.get(&tab), self.sizes.work.get_mut(&tab))
        else {
            return;
        };
        let Some(owner) = work.owner_for(asker, space) else {
            return;
        };
        if asker == Asker::Shown && work.asked.contains_key(&name) {
            return;
        }
        let (provider, sender, wake) = (
            Arc::clone(&self.provider),
            self.sender.clone(),
            Arc::clone(&self.wake),
        );
        let totals = Arc::clone(&self.folder_totals);
        let key = name.clone();
        let slot = JobSlot {
            volume: volume_of(&target),
            priority: asker.priority(),
            owner,
        };
        work.count_asked(asker);
        let job = self.pools.queue(JobKind::Size).submit_then(
            slot,
            move |cancel| Measured {
                tab,
                folder,
                size: catch_unwind(AssertUnwindSafe(|| {
                    folder_measure(&totals, provider.as_ref(), &target, cancel).unwrap_or_default()
                }))
                .ok(),
                name,
            },
            move |measured| {
                let _ = sender.send(Message::Measured(owner, measured));
                wake();
            },
        );
        work.asked.insert(key, Asked { job, asker });
    }
}
