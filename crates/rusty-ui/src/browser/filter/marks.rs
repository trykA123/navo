use std::collections::HashSet;

use rusty_core::{Command, CommandError, Effect, Event, Listing, Location, TabId, apply};

use super::Browser;

pub(in crate::browser) struct HiddenMarks {
    location: Location,
    names: HashSet<String>,
}

impl Browser {
    #[must_use]
    pub fn hidden_marks(&self, tab: TabId) -> u64 {
        self.hidden_marks
            .get(&tab)
            .map_or(0, |hidden| hidden.names.len() as u64)
    }

    pub(in crate::browser) fn apply_keeping_hidden_marks(
        &mut self,
        command: &Command,
    ) -> Result<Vec<Effect>, CommandError> {
        match command {
            Command::SelectionClear => {
                let tab = self.state.active_tab();
                let mut names = tab.marked_names();
                let hidden = self.hidden_marks.remove(&tab.id());
                let effects = apply(&mut self.state, command)?;
                if let Some(hidden) = hidden {
                    names.extend(hidden.names);
                    self.state.remember_marks(names);
                }
                Ok(effects)
            }
            Command::SelectionRestore => {
                let is_hidden = self.restore_hidden();
                match apply(&mut self.state, command) {
                    Err(CommandError::NothingToRestore) if is_hidden => Ok(Vec::new()),
                    applied => applied,
                }
            }
            _ => apply(&mut self.state, command),
        }
    }

    fn restore_hidden(&mut self) -> bool {
        let tab = self.state.active_tab();
        let (Some(saved), Some(shown), Some(full)) = (
            self.state.last_marks(),
            tab.listing(),
            self.full.get(&tab.id()),
        ) else {
            return false;
        };
        let saved: HashSet<&str> = saved.iter().map(String::as_str).collect();
        let visible: HashSet<&str> = shown.entries().iter().map(|e| e.name.as_str()).collect();
        let newly: Vec<String> = full
            .listing
            .entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .filter(|name| saved.contains(name) && !visible.contains(name))
            .map(str::to_owned)
            .collect();
        if newly.is_empty() {
            return false;
        }
        let (id, location) = (tab.id(), tab.location().clone());
        let hidden = self.hidden_marks.entry(id).or_insert_with(|| HiddenMarks {
            location,
            names: HashSet::new(),
        });
        hidden.names.extend(newly);
        true
    }

    pub(in crate::browser) fn hide_marks(&mut self, tab: TabId, shown: &Listing) {
        let Some(found) = self.state.tab(tab) else {
            return;
        };
        let Some(old) = found.listing() else {
            return;
        };
        let visible: HashSet<&str> = shown.entries().iter().map(|e| e.name.as_str()).collect();
        let newly: Vec<String> = found
            .selection()
            .marked()
            .into_iter()
            .filter_map(|row| old.get(row))
            .filter(|entry| !visible.contains(entry.name.as_str()))
            .map(|entry| entry.name.clone())
            .collect();
        if newly.is_empty() {
            return;
        }
        let location = found.location().clone();
        let hidden = self.hidden_marks.entry(tab).or_insert_with(|| HiddenMarks {
            location,
            names: HashSet::new(),
        });
        hidden.names.extend(newly);
    }

    pub(in crate::browser) fn remark_hidden(&mut self, tab: TabId) {
        let Some(listing) = self.state.tab(tab).and_then(|t| t.listing()) else {
            return;
        };
        let Some(hidden) = self.hidden_marks.get_mut(&tab) else {
            return;
        };
        let mut back: Vec<String> = listing
            .entries()
            .iter()
            .filter(|entry| hidden.names.remove(&entry.name))
            .map(|entry| entry.name.clone())
            .collect();
        if hidden.names.is_empty() {
            self.hidden_marks.remove(&tab);
        }
        if !back.is_empty() {
            back.sort_unstable();
            self.event(Event::MarksRestored { tab, names: back });
        }
    }

    pub(in crate::browser) fn prune_hidden(&mut self, tab: TabId, full: &Listing) {
        let Some(hidden) = self.hidden_marks.get_mut(&tab) else {
            return;
        };
        let present: HashSet<&str> = full.entries().iter().map(|e| e.name.as_str()).collect();
        hidden.names.retain(|name| present.contains(name.as_str()));
        if hidden.names.is_empty() {
            self.hidden_marks.remove(&tab);
        }
    }

    pub(in crate::browser) fn drop_hidden_marks(&mut self) {
        let state = &self.state;
        self.hidden_marks.retain(|tab, hidden| {
            state
                .tab(*tab)
                .is_some_and(|tab| tab.location() == &hidden.location)
        });
    }
}
