use rusty_core::ProviderPath;
use rusty_core::{Command, CommandError, Direction, Location};
use rusty_fs::EntryKind;

use super::{Browser, Launcher};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    Folder(Location),
    File(Location),
}

impl Browser {
    pub fn take_seen(&mut self) -> Vec<Seen> {
        std::mem::take(&mut self.seen)
    }

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
        self.open_location(location, entry.is_folder || is_link)
    }

    pub fn open_location(&mut self, location: Location, folder: bool) -> bool {
        if folder {
            return self.command(&Command::PaneNavigate { location }).is_ok();
        }
        (self.launcher)(&location);
        self.seen.push(Seen::File(location));
        true
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

    pub(crate) fn clone_tab_to_other_pane(&mut self) -> Result<(), CommandError> {
        let state = &self.state;
        let source = state.focused_pane();
        let direction = state.window(state.focused_window()).and_then(|window| {
            Direction::ALL
                .into_iter()
                .find(|direction| window.layout().neighbour(source, *direction).is_some())
        });
        let Some(direction) = direction else {
            return Ok(());
        };
        let tab = state.active_tab();
        let location = Some(tab.location().clone());
        let sort = self.sort_key(tab.id());
        self.command(&Command::LayoutFocus { direction })?;
        let opened = self.command(&Command::PaneOpenTab { location });
        if opened.is_ok() {
            let tab = self.state.active_tab().id();
            self.sorts.insert(tab, sort);
            self.resort(tab);
        }
        self.command(&Command::LayoutFocus {
            direction: direction.opposite(),
        })?;
        opened
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::tests::{browser_at, fake, here, settle};
    use super::Seen;

    #[test]
    fn cloning_preserves_a_provider_location() {
        let fs = fake();
        let mut browser = browser_at(&fs, "home");
        let location = rusty_core::Location::new("archive://bundle.zip/docs").unwrap();
        browser
            .command(&rusty_core::Command::PaneNavigate {
                location: location.clone(),
            })
            .unwrap();
        browser
            .command(&rusty_core::Command::LayoutSplit {
                direction: rusty_core::Direction::Right,
            })
            .unwrap();
        let source = browser.state().focused_pane();
        browser.clone_tab_to_other_pane().unwrap();
        assert_eq!(browser.state().focused_pane(), source);
        let window = browser
            .state()
            .window(browser.state().focused_window())
            .unwrap();
        let other = window
            .panes()
            .into_iter()
            .find(|pane| pane.id() != source)
            .unwrap();
        assert_eq!(other.tabs().len(), 2);
        assert_eq!(other.active_index(), 1);
        assert_eq!(other.active().location(), &location);
    }

    #[test]
    fn a_folder_opens_in_the_active_pane() {
        let fs = fake();
        let mut browser = browser_at(&fs, "home");
        settle(&mut browser);
        assert!(browser.open_location(fs.location("home/docs"), true));
        settle(&mut browser);
        assert_eq!(here(&browser), fs.location("home/docs").as_str());
    }

    #[test]
    fn a_file_launches_with_the_launcher_and_is_recorded() {
        let fs = fake();
        let mut browser = browser_at(&fs, "home");
        settle(&mut browser);
        let launched = Arc::new(Mutex::new(Vec::new()));
        let sink = launched.clone();
        browser.set_launcher(Arc::new(move |location| {
            sink.lock().unwrap().push(location.as_str().to_owned());
        }));
        browser.take_seen();
        let file = fs.location("home/docs/report.txt");
        assert!(browser.open_location(file.clone(), false));
        assert_eq!(*launched.lock().unwrap(), [file.as_str().to_owned()]);
        assert_eq!(browser.take_seen(), [Seen::File(file)]);
        assert_eq!(here(&browser), fs.location("home").as_str());
    }
}
