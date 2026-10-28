use rusty_core::{Command, CommandError, Effect, Location, StateError, TabMove, list_unloaded};
use rusty_fs::EntryKind;

use super::{Browser, Launcher};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    Folder(Location),
    File(Location),
}

pub(super) fn save_request(command: &Command) -> Option<Effect> {
    matches!(
        command,
        Command::PaneOpenTab { .. }
            | Command::PaneCloseTab { .. }
            | Command::PaneNavigate { .. }
            | Command::PaneGoDrive { .. }
            | Command::PaneBack
            | Command::PaneForward
            | Command::PaneActivateTab { .. }
            | Command::LayoutSplit { .. }
            | Command::LayoutClose
            | Command::LayoutPreset { .. }
    )
    .then_some(Effect::SaveSession)
}

impl Browser {
    pub fn move_tab(&mut self, change: TabMove) -> Result<(), StateError> {
        let focused = self.state.focused_pane();
        self.state.move_tab(change)?;
        let mut effects = list_unloaded(&mut self.state);
        effects.push(Effect::SaveSession);
        self.run(effects);
        if self.state.focused_pane() != focused {
            self.revalidate_focused();
        }
        Ok(())
    }

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
        let Ok(location) = tab.entry_location(&entry.name) else {
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

    pub(crate) fn focus_target_pane(&mut self) -> Result<(), CommandError> {
        let Some(target) = self.target_pane() else {
            return Ok(());
        };
        let window = self
            .state
            .window(self.state.focused_window())
            .ok_or(StateError::UnknownWindow(self.state.focused_window()))?;
        let number = window
            .layout()
            .numbered()
            .iter()
            .position(|&pane| pane == target)
            .ok_or(StateError::UnknownPane(target))?
            + 1;
        self.state.focus_number(number)?;
        self.revalidate_focused();
        Ok(())
    }

    pub(crate) fn focus_next_pane(&mut self, back: bool) -> Result<(), CommandError> {
        let window = self
            .state
            .window(self.state.focused_window())
            .ok_or(StateError::UnknownWindow(self.state.focused_window()))?;
        let numbered = window.layout().numbered();
        let focused = self.state.focused_pane();
        let at = numbered
            .iter()
            .position(|&pane| pane == focused)
            .ok_or(StateError::UnknownPane(focused))?;
        let count = numbered.len();
        let next = if back { at + count - 1 } else { at + 1 } % count;
        self.state.focus_number(next + 1)?;
        self.revalidate_focused();
        Ok(())
    }

    pub(crate) fn clone_tab_to_other_pane(&mut self) -> Result<(), CommandError> {
        let state = &self.state;
        let Some(target) = self.target_pane() else {
            return Ok(());
        };
        let window = state
            .window(state.focused_window())
            .ok_or(StateError::UnknownWindow(state.focused_window()))?;
        let panes = window.layout().numbered();
        let number = |pane| {
            panes
                .iter()
                .position(|&id| id == pane)
                .map(|index| index + 1)
                .ok_or(StateError::UnknownPane(pane))
        };
        let source = number(state.focused_pane())?;
        let target = number(target)?;
        let tab = state.active_tab();
        let location = Some(tab.location().clone());
        let sort = self.sort_key(tab.id());
        self.state.focus_number(target)?;
        self.revalidate_focused();
        let opened = self.command(&Command::PaneOpenTab { location });
        if opened.is_ok() {
            let tab = self.state.active_tab().id();
            self.sorts.insert(tab, sort);
            self.resort(tab);
        }
        self.state.focus_number(source)?;
        self.revalidate_focused();
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
    fn cloning_opens_in_the_last_focused_other_pane_with_three_panes() {
        let fs = fake();
        let mut browser = browser_at(&fs, "home");
        for _ in 0..2 {
            browser
                .command(&rusty_core::Command::LayoutSplit {
                    direction: rusty_core::Direction::Right,
                })
                .unwrap();
        }
        let target = browser.state().focused_pane();
        browser.state.focus_number(1).unwrap();
        let source = browser.state().focused_pane();
        browser
            .command(&rusty_core::Command::PaneNavigate {
                location: fs.location("home/docs"),
            })
            .unwrap();
        browser.clone_tab_to_other_pane().unwrap();
        assert_eq!(browser.state().focused_pane(), source);
        let window = browser
            .state()
            .window(browser.state().focused_window())
            .unwrap();
        let pane = window.pane(target).unwrap();
        assert_eq!(pane.tabs().len(), 2);
        assert_eq!(pane.active().location(), &fs.location("home/docs"));
        assert_eq!(window.panes()[1].tabs().len(), 1);
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
