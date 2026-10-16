mod card;
mod drive_bar;
mod drive_menu;
mod drives;
mod path_bar;
mod path_input;
mod segments;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::{App, ElementId, IntoElement, RenderOnce, SharedString, Window};
use rusty_core::Command;

pub use card::{card_header, card_shadow, pane_card};
pub use drive_bar::DriveBar;
pub use drive_menu::{DriveMenu, DriveMenuEvent, drive_for_key};
pub use drives::{DriveIcon, DriveInfo, current_drive};
pub use path_bar::PathBar;
pub use path_input::PathInput;
pub use segments::{PathSegment, path_segments};

use crate::primitives::{IconName, Tab, Tabs};

pub fn bind_keys(cx: &mut App) {
    drive_menu::bind_keys(cx);
    path_input::bind_keys(cx);
}

type CommandHandler = Rc<dyn Fn(&Command, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PaneTabs {
    id: ElementId,
    titles: Vec<SharedString>,
    active: usize,
    is_highlighted: bool,
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
            is_highlighted: false,
            on_command: None,
        }
    }

    pub fn highlighted(mut self, is_highlighted: bool) -> Self {
        self.is_highlighted = is_highlighted;
        self
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
        let tabs = Tabs::new(self.id, tabs)
            .selected(self.active)
            .highlighted(self.is_highlighted);
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
