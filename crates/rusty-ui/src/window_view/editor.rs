use std::sync::Arc;

use rusty_core::{Location, Tab};

use super::FerriteWindow;
use super::prompt::PromptKind;
use crate::prelude::*;
use crate::settings::{Change, Field};

type Resolve = Arc<dyn Fn(&str) -> Result<String, String> + Send + Sync>;
type OpenWith = Arc<dyn Fn(&str, &Location) -> Result<(), String> + Send + Sync>;

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
            resolve: Arc::new(|_| Err("Editors are not set up.".to_owned())),
            open: Arc::new(|_, _| Err("Editors are not set up.".to_owned())),
        }
    }
}

impl FerriteWindow {
    pub(super) fn edit_row(
        &mut self,
        (row, choose): (u64, bool),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = self.browser.state().active_tab();
        let Some(target) = edit_target(tab, row) else {
            return;
        };
        match self.values.editor_path().filter(|_| !choose) {
            Some(program) => self.open_in_editor((program, target), window, cx),
            None => self.ask_editor(target, None),
        }
    }

    fn open_in_editor(
        &mut self,
        (program, target): (String, Location),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self.editor.open.clone();
        self.run_off_thread(
            Box::new(move || {
                let opened = open(&program, &target);
                Box::new(move |view, _, _| view.editor_started(target, opened))
            }),
            window,
            cx,
        );
    }

    fn editor_started(&mut self, target: Location, opened: Result<(), String>) {
        if let Err(error) = opened {
            self.ask_editor(target, Some(error));
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
        (program, target): (&str, Location),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(prompt) = self.prompt.as_mut().filter(|prompt| !prompt.is_pending) else {
            return;
        };
        prompt.is_pending = true;
        let prompt = prompt.id;
        let (resolve, open) = (self.editor.resolve.clone(), self.editor.open.clone());
        let program = program.to_owned();
        self.run_off_thread(
            Box::new(move || {
                let result = resolve(&program).map(|path| {
                    let opened = open(&path, &target);
                    (path, opened)
                });
                let chosen = Chosen {
                    prompt,
                    target,
                    result,
                };
                Box::new(move |view, window, cx| view.editor_chosen(chosen, window, cx))
            }),
            window,
            cx,
        );
    }

    fn editor_chosen(&mut self, chosen: Chosen, window: &mut Window, cx: &mut Context<Self>) {
        let is_open = self.prompt.as_ref().is_some_and(|p| p.id == chosen.prompt);
        let (program, opened) = match chosen.result {
            Ok(started) => started,
            Err(problem) => {
                if let Some(prompt) = self.prompt.as_mut().filter(|_| is_open) {
                    prompt.error = Some(problem.into());
                    prompt.is_pending = false;
                }
                return;
            }
        };
        self.change_setting(Change::text(Field::EditorPath, program), window, cx);
        if is_open {
            self.close_prompt(window, cx);
        }
        self.editor_started(chosen.target, opened);
    }
}

struct Chosen {
    prompt: u64,
    target: Location,
    result: Result<(String, Result<(), String>), String>,
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
