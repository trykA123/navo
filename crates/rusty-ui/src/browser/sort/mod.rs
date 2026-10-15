use std::cmp::Ordering;

use rusty_core::{Entry, Listing, TabId};
use rusty_fs::{EntryKind, FsEntry};

use super::Browser;
use super::filter::Full;
pub use crate::file_list::{SortBy, SortKey};

impl Browser {
    #[must_use]
    pub fn sort_key(&self, tab: TabId) -> SortKey {
        self.sorts.get(&tab).copied().unwrap_or_default()
    }

    pub fn sort_by(&mut self, by: SortBy) -> bool {
        let tab = self.state.active_tab().id();
        let old = self.sort_key(tab);
        let key = SortKey {
            by,
            descending: old.by == by && !old.descending,
        };
        self.sorts.insert(tab, key);
        let Some(full) = self.full.remove(&tab) else {
            return false;
        };
        self.full.insert(tab, sorted(full, key));
        let text = self.filter_text(tab).unwrap_or_default().to_owned();
        self.set_filter(tab, text)
    }

    pub(super) fn sort_full(&self, tab: TabId, full: Full) -> Full {
        sorted(full, self.sort_key(tab))
    }
}

fn sorted(full: Full, key: SortKey) -> Full {
    let mut rows: Vec<FsEntry> = full.details.to_vec();
    rows.sort_by(|a, b| compare(a, b, key));
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

fn compare(a: &FsEntry, b: &FsEntry, key: SortKey) -> Ordering {
    let files_last = |entry: &FsEntry| entry.kind != EntryKind::Folder;
    let by_key = by(a, b, key.by);
    let by_key = if key.descending {
        by_key.reverse()
    } else {
        by_key
    };
    files_last(a)
        .cmp(&files_last(b))
        .then(by_key)
        .then_with(|| lower(a).cmp(&lower(b)))
}

fn by(a: &FsEntry, b: &FsEntry, sort: SortBy) -> Ordering {
    match sort {
        SortBy::Name => lower(a).cmp(&lower(b)),
        SortBy::Ext => extension(a)
            .cmp(&extension(b))
            .then_with(|| lower(a).cmp(&lower(b))),
        SortBy::Size => a.size.cmp(&b.size),
        SortBy::Date => a.modified_ms.cmp(&b.modified_ms),
    }
}

fn lower(entry: &FsEntry) -> String {
    entry.name.display().to_lowercase()
}

fn extension(entry: &FsEntry) -> String {
    if entry.kind == EntryKind::Folder {
        return String::new();
    }
    match lower(entry).rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_owned(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
