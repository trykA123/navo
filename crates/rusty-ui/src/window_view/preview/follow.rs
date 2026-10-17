use rusty_core::{Location, PaneId, TabId};
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
        let row = tab.view().cursor;
        let listed = tab.listing()?.get(row)?;
        let details = self.browser.details(tab.id());
        let entry = details.get(usize::try_from(row).ok()?)?;
        if entry.name.to_text() != listed.name {
            return None;
        }
        Some(Target {
            pane,
            tab: tab.id(),
            folder: tab.location().clone(),
            location: tab.location().join(&entry.name).ok()?,
            entry: entry.clone(),
        })
    }
}
