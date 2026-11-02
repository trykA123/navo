use std::collections::HashMap;

use navo_core::TabId;
use navo_fs::{OwnerGuard, OwnerId};

use super::Browser;
use super::measure::{Measured, Remembered, SizeMode};

impl Browser {
    /// Shows the sizes of one drain, copying each tab's rows once.
    pub(super) fn accept_sizes(&mut self, arrived: Vec<(OwnerId, Measured)>) -> bool {
        let mut shown: HashMap<TabId, Vec<(String, u64)>> = HashMap::new();
        for (owner, measured) in arrived {
            if !self.settle_size_job(owner, &measured) {
                continue;
            }
            if let Some(bytes) = self.remember_measured(&measured) {
                shown
                    .entry(measured.tab)
                    .or_default()
                    .push((measured.name, bytes));
            }
        }
        let mut changed = false;
        for (tab, sizes) in shown {
            changed |= self.show_sizes(tab, &sizes);
        }
        changed
    }

    fn settle_size_job(&mut self, owner: OwnerId, measured: &Measured) -> bool {
        let is_space = self.size_owners.get(&measured.tab).map(OwnerGuard::id) == Some(owner);
        let Some(work) = self.sizes.work.get_mut(&measured.tab) else {
            return false;
        };
        if is_space {
            work.pending = work.pending.saturating_sub(1);
        } else if work.owns(owner) {
            work.shown_pending = work.shown_pending.saturating_sub(1);
        } else {
            return false;
        }
        work.asked.remove(&measured.name);
        true
    }

    pub(crate) fn show_folder_size(&mut self, measured: &Measured) -> bool {
        let Some(bytes) = self.remember_measured(measured) else {
            return false;
        };
        self.show_sizes(measured.tab, &[(measured.name.clone(), bytes)])
    }

    /// The number to show, or `None` when the row is elsewhere or keeps a known size on disk.
    fn remember_measured(&mut self, measured: &Measured) -> Option<u64> {
        let here = self
            .state
            .tab(measured.tab)
            .is_some_and(|tab| tab.location() == &measured.folder);
        let (true, Some(size)) = (here, measured.size) else {
            return None;
        };
        let mode = self.sizes.mode;
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
        let keeps_on_disk = mode == SizeMode::OnDisk
            && !size.on_disk_known
            && remembered
                .sizes
                .get(&measured.name)
                .is_some_and(|old| old.on_disk_known);
        if keeps_on_disk {
            return None;
        }
        remembered.sizes.insert(measured.name.clone(), size);
        Some(mode.shown(&size))
    }

    pub(super) fn reshow_sizes(&mut self, tab: TabId) {
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
        self.show_sizes(tab, &shown);
    }

    #[cfg(test)]
    pub(super) fn show_size_of(&mut self, tab: TabId, name: &str, bytes: u64) -> bool {
        self.show_sizes(tab, &[(name.to_owned(), bytes)])
    }

    pub(super) fn show_sizes(&mut self, tab: TabId, sizes: &[(String, u64)]) -> bool {
        if sizes.is_empty() {
            return false;
        }
        self.remember_sizes(tab, sizes);
        let Some(listing) = self.state.tab(tab).and_then(|t| t.listing()) else {
            return false;
        };
        let Some(details) = self.details.get(&tab) else {
            return false;
        };
        let row_of: HashMap<&str, usize> = listing
            .entries()
            .iter()
            .enumerate()
            .map(|(row, entry)| (entry.name.as_str(), row))
            .collect();
        let mut rows = details.to_vec();
        count_copy();
        let mut changed = false;
        for (name, bytes) in sizes {
            if let Some(entry) = row_of.get(name.as_str()).and_then(|&row| rows.get_mut(row)) {
                entry.size = *bytes;
                changed = true;
            }
        }
        if changed {
            self.details.insert(tab, rows.into());
        }
        changed
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static COPIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn count_copy() {
    #[cfg(test)]
    COPIES.with(|copies| copies.set(copies.get() + 1));
}
