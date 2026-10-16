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

    pub(super) fn full_of(&self, tab: TabId, rows: &[FsEntry]) -> Full {
        sorted_rows(rows, self.sort_key(tab))
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

fn sorted(full: Full, key: SortKey) -> Full {
    sorted_rows(&full.details, key)
}

fn sorted_rows(rows: &[FsEntry], key: SortKey) -> Full {
    let mut rows: Vec<Keyed> = rows.iter().cloned().map(keyed).collect();
    rows.sort_by(|a, b| compare(a, b, key));
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

fn compare(a: &Keyed, b: &Keyed, key: SortKey) -> Ordering {
    a.is_file.cmp(&b.is_file).then_with(|| {
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
