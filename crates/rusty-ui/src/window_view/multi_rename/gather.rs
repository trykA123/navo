use rusty_core::rename::check::{Existing, ExistingName, Fold};
use rusty_core::{Location, ProviderPath, Tab};
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
        let shown: HashSet<_> = items
            .iter()
            .map(|item| (&item.parent, &item.name))
            .collect();
        self.items.len() == shown.len()
            && self
                .items
                .iter()
                .all(|item| shown.contains(&(&item.parent, &item.name)))
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
    gather_tab(browser, browser.state().active_tab())
}

fn gather_tab(browser: &Browser, tab: &Tab) -> Gathered {
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
            let resolved = entry_name(tab, &entry.name)?;
            let index = usize::try_from(row).ok();
            let mut detail = index
                .and_then(|index| details.get(index))
                .filter(|detail| detail.name.to_text() == entry.name)
                .cloned()
                .unwrap_or_else(|| bare_entry(&entry.name, entry.is_folder));
            if resolved.name != entry.name {
                detail.name = ProviderPath::from_text(&resolved.name);
            }
            Some(Item::from_entry(&resolved.parent, &detail))
        })
        .collect();
    if items.is_empty() {
        return Gathered::Empty;
    }
    let names = browser.full_names(tab.id()).into_iter();
    let existing = Existing::new(names.filter_map(|name| entry_name(tab, &name)).collect());
    Gathered::Ready(Batch {
        folder,
        items,
        existing,
    })
}

fn entry_name(tab: &Tab, name: &str) -> Option<ExistingName> {
    if tab.base_or_location() == tab.location() {
        return Some(ExistingName {
            parent: tab.location().clone(),
            name: name.to_owned(),
        });
    }
    let location = tab.entry_location(name).ok()?;
    Some(ExistingName {
        parent: location.parent()?,
        name: location.name()?.to_text(),
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
use std::collections::HashSet;

#[cfg(test)]
mod tests {
    use crate::browser::Browser;
    use rusty_core::rename::check::{Existing, ExistingName};
    use rusty_core::{AppState, Entry, Event, Listing, Location, apply_event, list_missing};
    use rusty_fs::FakeFs;
    use std::sync::Arc;

    #[test]
    fn multi_rename_gathers_a_results_entry_with_its_real_parent_and_name() {
        let location = Location::new(r"C:\shown").unwrap();
        let mut state = AppState::new(location.clone());
        list_missing(&mut state);
        let id = state.active_tab().id();
        let request = state.active_tab().pending_request().unwrap();
        apply_event(
            &mut state,
            Event::ListingLoaded {
                tab: id,
                request,
                location,
                listing: Listing::new(vec![Entry {
                    name: r"sub\a.txt".to_owned(),
                    is_folder: false,
                }]),
            },
        );
        let mut tab = state.active_tab().clone();
        tab.set_base(Some(Location::new(r"C:\base").unwrap()));
        let browser = Browser::new(state, Arc::new(FakeFs::new()), Arc::new(|| {}));
        let super::Gathered::Ready(batch) = super::gather_tab(&browser, &tab) else {
            panic!("expected gathered entry");
        };
        let parent = Location::new(r"C:\base\sub").unwrap();
        assert_eq!(
            (&batch.items[0].parent, batch.items[0].name.as_str()),
            (&parent, "a.txt")
        );
        assert_eq!(
            batch.existing,
            Existing::new(vec![ExistingName {
                parent,
                name: "a.txt".to_owned()
            }])
        );
    }
}
