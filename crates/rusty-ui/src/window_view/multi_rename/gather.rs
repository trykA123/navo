use rusty_core::rename::check::{Existing, ExistingName, Fold};
use rusty_core::{Location, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use crate::browser::Browser;
use crate::multi_rename::Item;

pub(super) struct Batch {
    pub folder: Location,
    pub items: Vec<Item>,
    pub existing: Existing,
}

impl Batch {
    pub(super) fn has_items(&self, items: &[Item]) -> bool {
        self.items.len() == items.len()
            && self
                .items
                .iter()
                .zip(items)
                .all(|(fresh, shown)| fresh.parent == shown.parent && fresh.name == shown.name)
    }
}

pub(super) enum Gathered {
    Ready(Batch),
    Loading,
    Empty,
}

pub(super) const fn platform_fold() -> Fold {
    if cfg!(windows) {
        Fold::Insensitive
    } else {
        Fold::Exact
    }
}

pub(super) fn gather(browser: &Browser) -> Gathered {
    let tab = browser.state().active_tab();
    let Some(listing) = tab.listing().filter(|_| tab.pending_request().is_none()) else {
        return Gathered::Loading;
    };
    let folder = tab.location().clone();
    let marked = tab.selection().marked();
    let rows = if marked.is_empty() {
        vec![tab.view().cursor]
    } else {
        marked
    };
    let details = browser.details(tab.id());
    let items: Vec<Item> = rows
        .into_iter()
        .filter_map(|row| {
            let entry = listing.get(row).filter(|entry| entry.name != "..")?;
            let index = usize::try_from(row).ok();
            let detail = index
                .and_then(|index| details.get(index))
                .filter(|detail| detail.name.to_text() == entry.name)
                .cloned()
                .unwrap_or_else(|| bare_entry(&entry.name, entry.is_folder));
            Some(Item::from_entry(&folder, &detail))
        })
        .collect();
    if items.is_empty() {
        return Gathered::Empty;
    }
    let names = browser.full_names(tab.id()).into_iter();
    let existing = Existing::new(
        names
            .map(|name| ExistingName {
                parent: folder.clone(),
                name,
            })
            .collect(),
    );
    Gathered::Ready(Batch {
        folder,
        items,
        existing,
    })
}

fn bare_entry(name: &str, is_folder: bool) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: if is_folder {
            EntryKind::Folder
        } else {
            EntryKind::File
        },
        size: 0,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}
