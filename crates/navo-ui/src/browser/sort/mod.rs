use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use navo_core::{Entry, Listing, TabId};
use navo_fs::view::{Rows, sort_order};
use navo_fs::{CancelToken, EntryKind, FsEntry};

use super::Browser;
use super::filter::Full;
use super::view_jobs::ViewChange;
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
        if self.views_off_thread(tab) {
            return self.request_view(tab, ViewChange::Sort);
        }
        let key = self.sort_key(tab);
        let Some(full) = self.full.remove(&tab) else {
            return false;
        };
        let source = Arc::clone(full.details.source());
        let sorted = if self.is_cached_source(tab, &source) {
            self.sorted_full(tab, &source)
        } else {
            sorted_full(&source, key, self.folders_first, &self.sort_count)
        };
        self.full.insert(tab, sorted);
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

    pub(super) fn full_of(&self, tab: TabId, rows: &Arc<[FsEntry]>) -> Full {
        #[cfg(test)]
        ui_sorts::bump();
        sorted_full(
            rows,
            self.sort_key(tab),
            self.folders_first,
            &self.sort_count,
        )
    }

    #[cfg(test)]
    pub(super) fn sorts(&self) -> usize {
        self.sort_count.0.load(Ordering::Relaxed)
    }
}

/// Sorts started for one `Browser`, on the UI thread and on workers.
#[derive(Clone, Default)]
pub(in crate::browser) struct SortCount(Arc<AtomicUsize>);

pub(super) fn sorted_full(
    rows: &Arc<[FsEntry]>,
    key: SortKey,
    folders_first: bool,
    count: &SortCount,
) -> Full {
    let cancel = CancelToken::new();
    sorted_rows(rows, (key, folders_first), &cancel, count).unwrap_or_else(|| {
        let details = Rows::all(Arc::clone(rows));
        Full {
            listing: listing_of(&details),
            details,
        }
    })
}

/// Sorts `rows` into an index view over them; `None` when cancelled.
pub(super) fn sorted_rows(
    rows: &Arc<[FsEntry]>,
    (key, folders_first): (SortKey, bool),
    cancel: &CancelToken,
    count: &SortCount,
) -> Option<Full> {
    count.0.fetch_add(1, Ordering::Relaxed);
    let order = sort_order(rows, key, folders_first, cancel)?;
    let details = Rows::ordered(Arc::clone(rows), order.into());
    Some(Full {
        listing: listing_of(&details),
        details,
    })
}

pub(super) fn listing_of(rows: &Rows) -> Listing {
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
#[cfg(test)]
mod tests_jobs;
#[cfg(test)]
mod tests_views;
