mod card;
mod collapse;
mod crumb_menu;
mod drive_menu;
mod drive_pill;
mod drives;
mod path_bar;
mod path_input;
mod segments;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ElementId, Entity, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use rusty_core::Command;

pub use card::{
    PaneMarks, card_header, card_shadow, marked_header, marker_tag, on_accent, pane_card,
    target_outline,
};
pub use drive_menu::{DriveMenu, DriveMenuEvent, drive_for_key};
pub use drive_pill::DrivePill;
pub use drives::{DriveIcon, DriveInfo, current_drive};
pub use path_bar::PathBar;
pub use path_input::PathInput;
pub use segments::{PathSegment, path_segments};

use crate::drag::Landing;
use crate::file_list::DropPayload;
use crate::primitives::popup_menu::PopupMenu;
use crate::primitives::{IconName, Tab, Tabs, icon};
use crate::tokens;

pub fn bind_keys(cx: &mut App) {
    drive_menu::bind_keys(cx);
    crate::primitives::popup_menu::bind_keys(cx);
    path_input::bind_keys(cx);
}

type CommandHandler = Rc<dyn Fn(&Command, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PaneTabs {
    id: ElementId,
    titles: Vec<SharedString>,
    active: usize,
    is_highlighted: bool,
    is_on_accent: bool,
    on_command: Option<CommandHandler>,
    landing: Landing<usize>,
    menu: Option<Entity<PopupMenu>>,
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
            is_on_accent: false,
            on_command: None,
            landing: Landing::default(),
            menu: None,
        }
    }

    pub(crate) fn menu(mut self, state: Entity<PopupMenu>) -> Self {
        self.menu = Some(state);
        self
    }

    pub fn highlighted(mut self, is_highlighted: bool) -> Self {
        self.is_highlighted = is_highlighted;
        self
    }

    pub fn on_accent(mut self, is_on_accent: bool) -> Self {
        self.is_on_accent = is_on_accent;
        self
    }

    pub fn on_command(
        mut self,
        handler: impl Fn(&Command, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_command = Some(Rc::new(handler));
        self
    }

    pub fn on_drop(
        mut self,
        handler: impl Fn(&usize, &DropPayload, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.landing.on_drop(handler);
        self
    }

    pub fn on_hover(
        mut self,
        handler: impl Fn(&usize, bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.landing.on_hover(handler);
        self
    }
}

fn new_tab_button(t: &tokens::Theme, handler: Option<CommandHandler>) -> impl IntoElement {
    div()
        .id("new-tab")
        .role(Role::Button)
        .aria_label("New tab")
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(t.spacing.tab_height)
        .rounded(t.radii.pill)
        .cursor_pointer()
        .hover(|style| style.bg(t.colors.hover))
        .when_some(handler, |d, handler| {
            d.on_click(move |_, window, cx| {
                handler(&Command::PaneOpenTab { location: None }, window, cx);
            })
        })
        .child(icon(
            IconName::Plus,
            t.spacing.icon - px(1.),
            t.colors.muted,
        ))
}

impl RenderOnce for PaneTabs {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let bar = self.is_on_accent.then(|| on_accent(t));
        let tabs = self
            .titles
            .into_iter()
            .map(|title| Tab::new(title).icon(IconName::Folder));
        let plus = new_tab_button(bar.as_ref().unwrap_or(t), self.on_command.clone());
        let mut tabs = Tabs::new(self.id, tabs)
            .selected(self.active)
            .highlighted(self.is_highlighted)
            .trailing(plus, t.spacing.tab_height)
            .when_some(self.menu, Tabs::menu)
            .when_some(bar, Tabs::bar_theme);
        tabs.set_landing(self.landing);
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
