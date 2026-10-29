use std::sync::Arc;

use rusty_core::{Entry, Listing, TabId};
use rusty_fs::view::{materialize, sort_order};
use rusty_fs::{CancelToken, EntryKind, FsEntry};

use super::Browser;
use super::filter::Full;
pub use crate::file_list::{SortBy, SortKey};

impl Browser {
    #[must_use]
    pub fn sort_key(&self, tab: TabId) -> SortKey {
        self.sorts.get(&tab).copied().unwrap_or(self.default_sort)
    }

    pub fn set_default_sort(&mut self, key: SortKey, folders_first: bool) {
        let is_new_order = self.folders_first != folders_first;
        if !is_new_order && self.default_sort == key {
            return;
        }
        self.default_sort = key;
        self.folders_first = folders_first;
        if is_new_order {
            self.sorted.clear();
        }
        let tabs: Vec<TabId> = self
            .full
            .keys()
            .copied()
            .filter(|tab| is_new_order || !self.sorts.contains_key(tab))
            .collect();
        for tab in tabs {
            self.resort(tab);
        }
    }

    pub(super) fn resort(&mut self, tab: TabId) -> bool {
        let key = self.sort_key(tab);
        let Some(full) = self.full.remove(&tab) else {
            return false;
        };
        self.full.insert(tab, sorted(full, key, self.folders_first));
        let text = self.filter_text(tab).unwrap_or_default().to_owned();
        self.set_filter(tab, text)
    }

    pub fn sort_by(&mut self, by: SortBy) -> bool {
        let tab = self.state.active_tab().id();
        let old = self.sort_key(tab);
        let key = SortKey {
            by,
            descending: old.by == by && !old.descending,
        };
        self.sorts.insert(tab, key);
        self.resort(tab)
    }

    pub(super) fn full_of(&self, tab: TabId, rows: &[FsEntry]) -> Full {
        #[cfg(test)]
        ui_sorts::bump();
        sorted_rows(rows, self.sort_key(tab), self.folders_first)
    }
}

fn sorted(full: Full, key: SortKey, folders_first: bool) -> Full {
    sorted_rows(&full.details, key, folders_first)
}

pub(super) fn sorted_full(rows: &Arc<[FsEntry]>, key: SortKey, folders_first: bool) -> Full {
    sorted_rows(rows, key, folders_first)
}

fn sorted_rows(rows: &[FsEntry], key: SortKey, folders_first: bool) -> Full {
    let order = sort_order(rows, key, folders_first, &CancelToken::new()).unwrap_or_default();
    let rows = materialize(rows, &order);
    Full {
        listing: listing_of(&rows),
        details: rows.into(),
    }
}

pub(super) fn listing_of(rows: &[FsEntry]) -> Listing {
    Listing::new(
        rows.iter()
            .map(|entry| Entry {
                name: entry.name.to_text(),
                is_folder: entry.kind == EntryKind::Folder,
            })
            .collect(),
    )
}

#[cfg(test)]
pub(super) mod ui_sorts {
    use std::cell::Cell;

    thread_local!(static COUNT: Cell<usize> = const { Cell::new(0) });

    pub(in crate::browser) fn bump() {
        COUNT.with(|count| count.set(count.get() + 1));
    }

    pub(in crate::browser) fn take() -> usize {
        COUNT.with(Cell::take)
    }
}

#[cfg(test)]
mod tests;
