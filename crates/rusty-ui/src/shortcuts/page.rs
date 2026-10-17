use gpui_kit::{EventEmitter, FocusHandle, Focusable};

use super::{Recorded, SaveKeymap, Saved};
use crate::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShortcutsEvent {
    Saved(Saved),
    OpenFile,
}

pub struct ShortcutsPage {
    save: SaveKeymap,
    focus: FocusHandle,
}

impl EventEmitter<ShortcutsEvent> for ShortcutsPage {}

impl Focusable for ShortcutsPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ShortcutsPage {
    pub fn new(save: SaveKeymap, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            save,
            focus: cx.focus_handle(),
        }
    }

    pub fn set_problem(&mut self, problem: Option<SharedString>, _: &mut Context<Self>) {
        let _ = (problem, &self.save);
        todo!()
    }

    pub fn set_query(&mut self, query: &str, _: &mut Window, _: &mut Context<Self>) {
        let _ = query;
        todo!()
    }

    pub fn select(&mut self, command: &str, _: &mut Context<Self>) {
        let _ = command;
        todo!()
    }

    pub fn start_recording(&mut self, _: &mut Context<Self>) {
        todo!()
    }

    pub fn take(&mut self, recorded: Recorded, _: &mut Context<Self>) {
        let _ = recorded;
        todo!()
    }

    pub fn replace(&mut self, _: &mut Context<Self>) {
        todo!()
    }

    pub fn cancel(&mut self, _: &mut Context<Self>) {
        todo!()
    }

    pub fn reset(&mut self, _: &mut Context<Self>) {
        todo!()
    }

    #[must_use]
    pub fn is_recording(&self) -> bool {
        todo!()
    }
}

#[cfg(test)]
impl ShortcutsPage {
    pub(crate) fn visible(&self) -> Vec<&'static str> {
        todo!()
    }
}

impl Render for ShortcutsPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
