use rusty_core::{Location, PaneId, Tab, TabId};
use rusty_fs::{EntryKind, FsEntry};

use crate::window_view::FerriteWindow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view) struct Key {
    pane: PaneId,
    location: Location,
    kind: EntryKind,
    modified_ms: Option<i64>,
    file_size: Option<u64>,
}

#[derive(Debug, Clone)]
pub(in crate::window_view) struct Target {
    pub pane: PaneId,
    pub tab: TabId,
    pub folder: Location,
    pub location: Location,
    pub entry: FsEntry,
}

impl Key {
    pub(super) fn same_place(&self, other: &Self) -> bool {
        (self.pane, &self.location, self.kind) == (other.pane, &other.location, other.kind)
    }
}

impl Target {
    pub(super) fn key(&self) -> Key {
        Key {
            pane: self.pane,
            location: self.location.clone(),
            kind: self.entry.kind,
            modified_ms: self.entry.modified_ms,
            file_size: (!self.is_folder()).then_some(self.entry.size),
        }
    }

    pub(super) fn is_folder(&self) -> bool {
        match self.entry.kind {
            EntryKind::Folder => true,
            EntryKind::Link => self.entry.attributes.link_to_folder,
            EntryKind::File => false,
        }
    }

    pub(super) fn name(&self) -> String {
        self.entry.name.to_text()
    }

    pub(super) fn changed_ms(&self) -> i64 {
        self.entry.modified_ms.unwrap_or_default()
    }
}

impl FerriteWindow {
    pub(super) fn cursor_target(&self) -> Option<Target> {
        let state = self.browser.state();
        let pane = state.focused_pane();
        let tab = state.pane(pane)?.active();
        cursor_target(pane, tab, &self.browser.details(tab.id()))
    }
}

fn cursor_target(pane: PaneId, tab: &Tab, details: &[FsEntry]) -> Option<Target> {
    let row = tab.view().cursor;
    let listed = tab.listing()?.get(row)?;
    let entry = details.get(usize::try_from(row).ok()?)?;
    if entry.name.to_text() != listed.name {
        return None;
    }
    Some(Target {
        pane,
        tab: tab.id(),
        folder: tab.location().clone(),
        location: tab.entry_location(&entry.name.to_text()).ok()?,
        entry: entry.clone(),
    })
}

#[cfg(test)]
mod tests {
    use rusty_core::{
        AppState, Entry, Event, Listing, Location, ProviderPath, apply_event, list_missing,
    };
    use rusty_fs::{Attributes, EntryKind, FsEntry};

    #[test]
    fn preview_resolves_a_results_entry_under_its_base() {
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
        let entry = FsEntry {
            name: ProviderPath::from_text(r"sub\a.txt"),
            kind: EntryKind::File,
            size: 1,
            modified_ms: None,
            attributes: Attributes::default(),
        };
        let target = super::cursor_target(state.focused_pane(), &tab, &[entry]).unwrap();
        assert_eq!(
            target.location,
            Location::new(r"C:\base\sub\a.txt").unwrap()
        );
    }
}
