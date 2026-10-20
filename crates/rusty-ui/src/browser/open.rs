use rusty_core::ProviderPath;
use rusty_core::{Command, Location};
use rusty_fs::EntryKind;

use super::{Browser, Launcher};

impl Browser {
    pub fn open_row(&mut self, row: u64) -> bool {
        let tab = self.state.active_tab();
        let Some(entry) = tab.listing().and_then(|l| l.get(row)) else {
            return false;
        };
        let is_link = usize::try_from(row)
            .ok()
            .and_then(|index| self.details.get(&tab.id())?.get(index).cloned())
            .is_some_and(|detail| {
                detail.kind == EntryKind::Link && detail.attributes.link_to_folder
            });
        let Ok(location) = tab.location().join(&ProviderPath::from_text(&entry.name)) else {
            return false;
        };
        if !entry.is_folder && !is_link {
            (self.launcher)(&location);
            return true;
        }
        self.command(&Command::PaneNavigate { location }).is_ok()
    }

    pub fn set_launcher(&mut self, launcher: Launcher) {
        self.launcher = launcher;
    }

    pub fn go_up(&mut self) -> bool {
        let tab = self.state.active_tab();
        let Some(parent) = tab.location().parent() else {
            return false;
        };
        let child = tab
            .location()
            .path()
            .to_text()
            .rsplit(['/', '\\'])
            .find(|segment| !segment.is_empty())
            .map(str::to_owned);
        let id = tab.id();
        if self
            .command(&Command::PaneNavigate { location: parent })
            .is_err()
        {
            return false;
        }
        if let Some(child) = child {
            self.focus_after_load.insert(id, child);
        }
        true
    }

    pub fn open_at(&mut self, folder: Location, name: String) -> bool {
        let id = self.state.active_tab().id();
        if self
            .command(&Command::PaneNavigate { location: folder })
            .is_err()
        {
            return false;
        }
        self.focus_after_load.insert(id, name);
        true
    }

    pub fn open_tab_at(&mut self, folder: Location, name: Option<String>) -> bool {
        let location = Some(folder);
        if self.command(&Command::PaneOpenTab { location }).is_err() {
            return false;
        }
        if let Some(name) = name {
            self.focus_after_load
                .insert(self.state.active_tab().id(), name);
        }
        true
    }
}
