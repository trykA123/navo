mod path_bar;
mod segments;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::{App, ElementId, IntoElement, RenderOnce, SharedString, Window};
use rusty_core::Command;

pub use path_bar::PathBar;
pub use segments::{PathSegment, path_segments};

use crate::primitives::{IconName, Tab, Tabs};

type CommandHandler = Rc<dyn Fn(&Command, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PaneTabs {
    id: ElementId,
    titles: Vec<SharedString>,
    active: usize,
    on_command: Option<CommandHandler>,
}

impl PaneTabs {
    pub fn new(
        id: impl Into<ElementId>,
        titles: impl IntoIterator<Item = impl Into<SharedString>>,
        active: usize,
    ) -> Self {
        Self {
            id: id.into(),
            titles: titles.into_iter().map(Into::into).collect(),
            active,
            on_command: None,
        }
    }

    pub fn on_command(
        mut self,
        handler: impl Fn(&Command, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_command = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for PaneTabs {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let tabs = self
            .titles
            .into_iter()
            .map(|title| Tab::new(title).icon(IconName::Folder));
        let tabs = Tabs::new(self.id, tabs).selected(self.active);
        match self.on_command {
            Some(handler) => tabs.on_select(move |index, window, cx| {
                if let Ok(tab) = u32::try_from(*index) {
                    handler(&Command::PaneActivateTab { tab }, window, cx);
                }
            }),
            None => tabs,
        }
    }
}
