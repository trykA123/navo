mod focus;
mod items;
mod view;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Div, ElementId, FocusHandle, InteractiveElement, IntoElement, KeyBinding,
    KeyDownEvent, ParentElement, RenderOnce, Role, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, deferred, div,
};

pub use focus::{Ring, Step, jump};
pub use items::{ITEM_COUNT, Item, MenuCommand, QuickSettings, Setting, index_of, item_at};

use crate::tokens::{self, Theme};

const CONTEXT: &str = "AppMenu";

gpui_kit::actions!(
    app_menu,
    [
        ToggleAppMenu,
        FocusLeft,
        FocusRight,
        FocusUp,
        FocusDown,
        Activate,
        CloseMenu
    ]
);

pub fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("f10", ToggleAppMenu, None),
        KeyBinding::new("left", FocusLeft, context),
        KeyBinding::new("right", FocusRight, context),
        KeyBinding::new("up", FocusUp, context),
        KeyBinding::new("down", FocusDown, context),
        KeyBinding::new("enter", Activate, context),
        KeyBinding::new("space", Activate, context),
        KeyBinding::new("escape", CloseMenu, context),
    ]);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppMenuEvent {
    Select(usize),
    Toggle(Setting),
    Run(MenuCommand),
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Install {
    Portable,
    Installed,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Identity {
    pub version: SharedString,
    pub install: Install,
    pub folder: SharedString,
}

impl Identity {
    fn title(&self) -> String {
        format!("Ferrite {}", self.version)
    }

    fn subtitle(&self) -> String {
        let install = match self.install {
            Install::Portable => "Portable",
            Install::Installed => "Installed",
        };
        format!("{install} · {}", self.folder)
    }
}

type EventHandler = Rc<dyn Fn(&AppMenuEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct AppMenu {
    id: ElementId,
    identity: Identity,
    settings: QuickSettings,
    selected: usize,
    focus: Option<FocusHandle>,
    on_event: Option<EventHandler>,
}

#[must_use]
pub fn event_for(item: Item) -> Option<AppMenuEvent> {
    match item {
        Item::Tile(setting) => Some(AppMenuEvent::Toggle(setting)),
        Item::Row(command) if command.is_available() => Some(AppMenuEvent::Run(command)),
        Item::Row(_) => None,
    }
}

impl AppMenu {
    pub fn new(id: impl Into<ElementId>, identity: Identity, settings: QuickSettings) -> Self {
        Self {
            id: id.into(),
            identity,
            settings,
            selected: 0,
            focus: None,
            on_event: None,
        }
    }

    #[must_use]
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index.min(ITEM_COUNT - 1);
        self
    }

    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    pub fn on_event(
        mut self,
        handler: impl Fn(&AppMenuEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }

    pub fn into_overlay(self, t: &Theme) -> AnyElement {
        let out = self.on_event.clone();
        let backdrop = div()
            .id("app-menu-backdrop")
            .test_support()
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(gpui_kit::MouseButton::Left, move |_, window, cx| {
                send(out.as_ref(), AppMenuEvent::Close, window, cx);
            });
        let anchored = div()
            .absolute()
            .top(t.spacing.header_height - t.spacing.xs)
            .left(t.spacing.md)
            .child(self);
        let layer = div().absolute().inset_0().child(backdrop).child(anchored);
        deferred(layer).with_priority(2).into_any_element()
    }
}

fn send(handler: Option<&EventHandler>, event: AppMenuEvent, window: &mut Window, cx: &mut App) {
    if let Some(handler) = handler {
        handler(&event, window, cx);
    }
}

fn ring() -> Ring {
    Ring {
        tiles: items::TILES.len(),
        rows: items::ROWS.len(),
        columns: items::COLUMNS,
    }
}

fn emit(
    handler: &Option<EventHandler>,
    event: Option<AppMenuEvent>,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let handler = handler.clone();
    move |window, cx| {
        if let Some(event) = event {
            send(handler.as_ref(), event, window, cx);
        }
    }
}

fn register(root: Stateful<Div>, selected: usize, handler: Option<EventHandler>) -> Stateful<Div> {
    let moved = |step| {
        emit(
            &handler,
            Some(AppMenuEvent::Select(ring().step(selected, step))),
        )
    };
    let (left, right, up, down) = (
        moved(Step::Left),
        moved(Step::Right),
        moved(Step::Up),
        moved(Step::Down),
    );
    let activate = emit(&handler, item_at(selected).and_then(event_for));
    let close = emit(&handler, Some(AppMenuEvent::Close));
    root.on_action(move |_: &FocusLeft, window, cx| left(window, cx))
        .on_action(move |_: &FocusRight, window, cx| right(window, cx))
        .on_action(move |_: &FocusUp, window, cx| up(window, cx))
        .on_action(move |_: &FocusDown, window, cx| down(window, cx))
        .on_action(move |_: &Activate, window, cx| activate(window, cx))
        .on_action(move |_: &CloseMenu, window, cx| close(window, cx))
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            let keystroke = &event.keystroke;
            if keystroke.modifiers.modified() {
                return;
            }
            let row = selected.checked_sub(items::TILES.len());
            if let Some(found) = jump(&items::row_labels(), row, &keystroke.key) {
                let event = AppMenuEvent::Select(items::TILES.len() + found);
                send(handler.as_ref(), event, window, cx);
            }
        })
}

impl RenderOnce for AppMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let look = view::Look {
            theme: t,
            selected: self.selected,
            send: self.on_event.clone(),
        };
        let header = view::header(t, &self.identity, window.scale_factor());
        let tiles = view::tiles(&look, self.settings);
        let rows = view::rows(&look);
        let panel = register(div().id(self.id), self.selected, self.on_event)
            .key_context(CONTEXT)
            .role(Role::Menu)
            .aria_label("Ferrite menu")
            .test_support()
            .when_some(self.focus.as_ref(), |d, focus| d.track_focus(focus));
        let panel = view::panel(t, panel)
            .child(header)
            .child(tiles)
            .children(rows);
        view::enter(panel, t, cx.reduce_motion())
    }
}
