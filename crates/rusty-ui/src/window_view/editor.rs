use std::rc::Rc;

use rusty_core::Location;

use super::FerriteWindow;
use super::prompt::PromptKind;
use crate::prelude::*;

type Remember = Rc<dyn Fn(&str) -> Result<String, String>>;
type OpenWith = Rc<dyn Fn(&str, &Location) -> Result<(), String>>;

pub struct EditorHooks {
    pub chosen: Option<String>,
    pub suggestion: Option<String>,
    pub remember: Remember,
    pub open: OpenWith,
}

impl EditorHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            chosen: None,
            suggestion: None,
            remember: Rc::new(|_| Err("Editors are not set up.".to_owned())),
            open: Rc::new(|_, _| Err("Editors are not set up.".to_owned())),
        }
    }
}

impl FerriteWindow {
    pub(super) fn edit_row(&mut self, row: u64, choose: bool) {
        let tab = self.browser.state().active_tab();
        let target = tab
            .listing()
            .and_then(|listing| listing.get(row))
            .filter(|entry| !entry.is_folder)
            .and_then(|entry| {
                tab.location()
                    .join(&rusty_core::ProviderPath::from_text(&entry.name))
                    .ok()
            });
        let Some(target) = target else {
            return;
        };
        match self.editor.chosen.clone().filter(|_| !choose) {
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
        self.editor
            .chosen
            .clone()
            .or_else(|| self.editor.suggestion.clone())
            .unwrap_or_default()
    }

    pub(super) fn choose_editor(
        &mut self,
        program: &str,
        target: &Location,
    ) -> Result<(), Option<String>> {
        let program = (self.editor.remember)(program).map_err(Some)?;
        let opened = (self.editor.open)(&program, target).map_err(Some);
        self.editor.chosen = Some(program);
        opened
    }
}
