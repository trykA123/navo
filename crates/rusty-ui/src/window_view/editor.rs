use std::rc::Rc;

use rusty_core::{Location, Tab};

use super::FerriteWindow;
use super::prompt::PromptKind;
use crate::prelude::*;
use crate::settings::{Change, Field};

type Resolve = Rc<dyn Fn(&str) -> Result<String, String>>;
type OpenWith = Rc<dyn Fn(&str, &Location) -> Result<(), String>>;

pub struct EditorHooks {
    pub suggestion: Option<String>,
    pub resolve: Resolve,
    pub open: OpenWith,
}

impl EditorHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            suggestion: None,
            resolve: Rc::new(|_| Err("Editors are not set up.".to_owned())),
            open: Rc::new(|_, _| Err("Editors are not set up.".to_owned())),
        }
    }
}

impl FerriteWindow {
    pub(super) fn edit_row(&mut self, row: u64, choose: bool) {
        let tab = self.browser.state().active_tab();
        let target = edit_target(tab, row);
        let Some(target) = target else {
            return;
        };
        match self.values.editor_path().filter(|_| !choose) {
            Some(program) => {
                if let Err(error) = (self.editor.open)(&program, &target) {
                    self.ask_editor(target, Some(error));
                }
            }
            None => self.ask_editor(target, None),
        }
    }

    fn ask_editor(&mut self, target: Location, error: Option<String>) {
        let program = self.editor_value();
        self.open_prompt(PromptKind::ChooseEditor { target, program });
        if let Some(prompt) = &mut self.prompt {
            prompt.error = error.map(SharedString::from);
        }
    }

    pub(super) fn editor_value(&self) -> String {
        self.values
            .editor_path()
            .or_else(|| self.editor.suggestion.clone())
            .unwrap_or_default()
    }

    pub(super) fn choose_editor(
        &mut self,
        (program, target): (&str, &Location),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Option<String>> {
        let program = (self.editor.resolve)(program).map_err(Some)?;
        let opened = (self.editor.open)(&program, target).map_err(Some);
        self.change_setting(Change::text(Field::EditorPath, program), window, cx);
        opened
    }
}

fn edit_target(tab: &Tab, row: u64) -> Option<Location> {
    tab.listing()
        .and_then(|listing| listing.get(row))
        .filter(|entry| entry.name != "..")
        .and_then(|entry| tab.entry_location(&entry.name).ok())
}

#[cfg(test)]
mod tests {

    fn based_tab() -> rusty_core::Tab {
        use rusty_core::{AppState, Entry, Event, Listing, Location, apply_event, list_missing};
        let location = Location::new(r"C:\shown").unwrap();
        let mut state = AppState::new(location.clone());
        list_missing(&mut state);
        let tab = state.active_tab().id();
        let request = state.active_tab().pending_request().unwrap();
        apply_event(
            &mut state,
            Event::ListingLoaded {
                tab,
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
        tab
    }

    #[test]
    fn editor_resolves_a_results_entry_under_its_base() {
        assert_eq!(
            super::edit_target(&based_tab(), 0),
            Some(rusty_core::Location::new(r"C:\base\sub\a.txt").unwrap())
        );
    }
}
