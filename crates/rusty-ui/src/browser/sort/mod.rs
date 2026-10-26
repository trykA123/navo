use std::cmp::Ordering;

use rusty_core::{Entry, Listing, TabId};
use rusty_fs::{EntryKind, FsEntry};

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
        sorted_rows(rows, self.sort_key(tab), self.folders_first)
    }
}

struct Keyed {
    is_file: bool,
    lower: String,
    extension: String,
    entry: FsEntry,
}

fn keyed(entry: FsEntry) -> Keyed {
    let lower = entry.name.display().to_lowercase();
    let is_file = entry.kind != EntryKind::Folder;
    let extension = match lower.rsplit_once('.') {
        Some((stem, ext)) if is_file && !stem.is_empty() => ext.to_owned(),
        _ => String::new(),
    };
    Keyed {
        is_file,
        lower,
        extension,
        entry,
    }
}

fn sorted(full: Full, key: SortKey, folders_first: bool) -> Full {
    sorted_rows(&full.details, key, folders_first)
}

fn sorted_rows(rows: &[FsEntry], key: SortKey, folders_first: bool) -> Full {
    let mut rows: Vec<Keyed> = rows.iter().cloned().map(keyed).collect();
    rows.sort_by(|a, b| compare(a, b, key, folders_first));
    let rows: Vec<FsEntry> = rows.into_iter().map(|row| row.entry).collect();
    let listing = Listing::new(
        rows.iter()
            .map(|entry| Entry {
                name: entry.name.to_text(),
                is_folder: entry.kind == EntryKind::Folder,
            })
            .collect(),
    );
    Full {
        listing,
        details: rows.into(),
    }
}

fn compare(a: &Keyed, b: &Keyed, key: SortKey, folders_first: bool) -> Ordering {
    let groups = if folders_first {
        a.is_file.cmp(&b.is_file)
    } else {
        Ordering::Equal
    };
    groups.then_with(|| {
        let by_key = by(a, b, key.by);
        let by_key = if key.descending {
            by_key.reverse()
        } else {
            by_key
        };
        by_key.then_with(|| a.lower.cmp(&b.lower))
    })
}

fn by(a: &Keyed, b: &Keyed, sort: SortBy) -> Ordering {
    match sort {
        SortBy::Name => a.lower.cmp(&b.lower),
        SortBy::Ext => a
            .extension
            .cmp(&b.extension)
            .then_with(|| a.lower.cmp(&b.lower)),
        SortBy::Size => a.entry.size.cmp(&b.entry.size),
        SortBy::Date => a.entry.modified_ms.cmp(&b.entry.modified_ms),
    }
}

#[cfg(test)]
mod tests;
