use rusty_core::ProviderPath;
use rusty_core::{Command, Location};
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
