use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, Entity, FocusHandle, InputEvent as _,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    NavigationDirection, ParentElement, Pixels, Point, Render, Styled, TestAppContext, Window, div,
    point, px, size,
};
use rusty_core::shell_menu::{
    Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu, ferrite_actions,
};

use super::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::test_window::open_sized;

mod filter;
mod flyout;
mod hover;
mod icons;
mod keys;
mod loading;
mod motion;
mod place;

pub(super) const ROOT: &str = "context-menu";
pub(super) const SEVEN_ZIP: usize = 2;
pub(super) const SEND_TO: usize = 4;

pub(super) fn shell(command: u32, raw: &str, group: Group) -> MenuItem {
    MenuItem::new(
        ItemId::Shell {
            command,
            verb: None,
        },
        raw,
        group,
    )
}

pub(super) fn lazy(command: u32, raw: &str, group: Group) -> MenuItem {
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Lazy),
        ..shell(command, raw, group)
    }
}

pub(super) fn filled(command: u32, raw: &str, children: Vec<MenuItem>) -> MenuItem {
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(children)),
        ..shell(command, raw, Group::Extensions)
    }
}

fn seven_zip() -> MenuItem {
    let children = vec![
        shell(30, "Open archive", Group::Extensions),
        shell(31, "Extract files…", Group::Extensions),
        shell(32, "Extract Here", Group::Extensions),
        MenuItem::separator(Group::Extensions),
        MenuItem {
            enabled: false,
            ..shell(33, "Extract to \"dist\\\"", Group::Extensions)
        },
    ];
    filled(3, "7-Zip", children)
}

pub(super) fn hidden_actions() -> Vec<MenuItem> {
    ferrite_actions()
        .into_iter()
        .map(|item| MenuItem {
            hidden: true,
            ..item
        })
        .collect()
}

pub(super) fn zip_menu() -> ShellMenu {
    let mut items = vec![
        MenuItem {
            default: true,
            ..shell(1, "&Open", Group::Open)
        },
        lazy(2, "Open wit&h", Group::Open),
        seven_zip(),
        MenuItem::separator(Group::Extensions),
        lazy(5, "&Send to", Group::Share),
        MenuItem {
            hidden: true,
            ..shell(6, "Copy as path", Group::Share)
        },
        MenuItem::separator(Group::Share),
        shell(7, "Cu&t\tCtrl+X", Group::Edit),
        shell(8, "&Copy\tCtrl+C", Group::Edit),
        MenuItem {
            enabled: false,
            ..shell(9, "&Paste\tCtrl+V", Group::Edit)
        },
        MenuItem::separator(Group::Edit),
        shell(10, "Create &shortcut", Group::Other),
        shell(11, "Restore previous versions", Group::Other),
    ];
    items.extend(hidden_actions());
    ShellMenu { items }
}

pub(super) const ROOT_ROWS: [&str; 12] = [
    "Open",
    "Open with",
    "7-Zip",
    "-",
    "Send to",
    "-",
    "Cut",
    "Copy",
    "Paste",
    "-",
    "Create shortcut",
    "Restore previous versions",
];

pub(super) fn window_bounds() -> Bounds<Pixels> {
    Bounds {
        origin: Point::default(),
        size: size(px(800.), px(600.)),
    }
}

struct Harness {
    menu: Entity<ContextMenu>,
    restore: FocusHandle,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(div().size_full().track_focus(&self.restore))
            .child(self.menu.clone())
    }
}

pub(super) struct Opened {
    pub window: AnyWindowHandle,
    pub menu: Entity<ContextMenu>,
    pub events: Rc<RefCell<Vec<ContextMenuEvent>>>,
    restore: FocusHandle,
}

pub(super) fn open_menu(cx: &mut TestAppContext, anchor: Point<Pixels>) -> Opened {
    open_menu_with(cx, anchor, zip_menu())
}

pub(super) fn open_menu_with(
    cx: &mut TestAppContext,
    anchor: Point<Pixels>,
    menu: ShellMenu,
) -> Opened {
    open_menu_in(cx, (anchor, window_bounds()), menu)
}

pub(super) fn open_menu_in(
    cx: &mut TestAppContext,
    (anchor, bounds): (Point<Pixels>, Bounds<Pixels>),
    menu: ShellMenu,
) -> Opened {
    let events: Rc<RefCell<Vec<ContextMenuEvent>>> = Rc::default();
    let sink = events.clone();
    let (window, harness) = open_sized(cx, bounds.size.width, move |window, cx| {
        let restore = cx.focus_handle();
        let input = ContextMenuInput {
            menu,
            context: "ferrite-0.4.2-win64.zip · 6.8 MB".into(),
            anchor,
            bounds,
            restore: Some(restore.clone()),
        };
        let menu = cx.new(|cx| ContextMenu::new(input, window, cx));
        cx.subscribe(&menu, move |_, event: &ContextMenuEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
        .detach();
        Harness { menu, restore }
    });
    let (menu, restore) = harness.read_with(cx, |harness, _| {
        (harness.menu.clone(), harness.restore.clone())
    });
    let opened = Opened {
        window,
        menu,
        events,
        restore,
    };
    opened.frame(cx);
    opened
}

pub(super) fn open_default(cx: &mut TestAppContext) -> Opened {
    open_menu(cx, point(px(40.), px(20.)))
}

impl Opened {
    pub fn frame(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        cx.run_until_parked();
    }

    pub fn press(&self, keys: &[&str], cx: &mut TestAppContext) {
        for key in keys {
            cx.update_window(self.window, |_, window, cx| window.press(key, cx))
                .expect("window");
            cx.run_until_parked();
        }
    }

    pub fn type_text(&self, text: &str, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.input(text, cx))
            .expect("window");
        cx.run_until_parked();
    }

    pub fn click(&self, row: usize, cx: &mut TestAppContext) {
        self.click_in(0, row, cx);
    }

    pub fn click_in(&self, level: usize, row: usize, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window
                .within(ROOT)
                .within(("menu-level", level))
                .click(("menu-row", row), cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    pub fn mouse_back(&self, cx: &mut TestAppContext) {
        let position = self.level_bounds(cx).center();
        cx.update_window(self.window, |_, window, cx| {
            let event = MouseDownEvent {
                button: MouseButton::Navigate(NavigationDirection::Back),
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            };
            window.dispatch_event(event.to_platform_input(), cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    pub fn hover(&self, row: usize, cx: &mut TestAppContext) {
        self.hover_in(0, row, cx);
    }

    pub fn hover_in(&self, level: usize, row: usize, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window
                .within(ROOT)
                .within(("menu-level", level))
                .hover(("menu-row", row), cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    pub fn move_to(&self, position: Point<Pixels>, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let event = MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: Default::default(),
            };
            window.dispatch_event(event.to_platform_input(), cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    pub fn wait(&self, millis: u64, cx: &mut TestAppContext) {
        cx.executor().advance_clock(Duration::from_millis(millis));
        cx.run_until_parked();
        self.frame(cx);
    }

    pub fn restore_is_focused(&self, cx: &mut TestAppContext) -> bool {
        cx.update_window(self.window, |_, window, _| self.restore.is_focused(window))
            .expect("window")
    }

    pub fn events(&self) -> Vec<ContextMenuEvent> {
        self.events.borrow().clone()
    }

    pub fn rows(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.rows_in(0, cx)
    }

    pub fn rows_in(&self, level: usize, cx: &mut TestAppContext) -> Vec<String> {
        self.with_level(level, cx, |window| {
            (0usize..)
                .map_while(|index| window.try_find(("menu-row", index)))
                .map(|row| row.label().unwrap_or("-").to_owned())
                .collect()
        })
    }

    pub fn selected(&self, cx: &mut TestAppContext) -> Option<String> {
        self.selected_in(0, cx)
    }

    pub fn selected_in(&self, level: usize, cx: &mut TestAppContext) -> Option<String> {
        self.with_level(level, cx, |window| {
            (0usize..)
                .map_while(|index| window.try_find(("menu-row", index)))
                .find(|row| row.selected() == Some(true))
                .and_then(|row| row.label().map(str::to_owned))
        })
    }

    pub fn levels(&self, cx: &mut TestAppContext) -> usize {
        self.with_root(cx, |window| {
            (0usize..)
                .take_while(|level| window.try_find(("menu-level", *level)).is_some())
                .count()
        })
    }

    pub fn loading_bar(&self, level: usize, cx: &mut TestAppContext) -> Option<Bounds<Pixels>> {
        self.with_level(level, cx, |window| {
            window
                .try_find(("menu-loading", level))
                .map(|found| found.bounds())
        })
    }

    pub fn sweep(&self, level: usize, cx: &mut TestAppContext) -> Option<Bounds<Pixels>> {
        self.with_level(level, cx, |window| {
            window
                .try_find(("menu-loading-sweep", level))
                .map(|found| found.bounds())
        })
    }

    pub fn find(&self, id: (&'static str, usize), cx: &mut TestAppContext) -> Option<String> {
        self.with_level(0, cx, |window| {
            window
                .try_find(id)
                .map(|found| found.label().unwrap_or_default().to_owned())
        })
    }

    pub fn has_classic(&self, cx: &mut TestAppContext) -> bool {
        self.with_root(cx, |window| window.try_find("menu-classic").is_some())
    }

    pub fn row_bounds(&self, row: usize, cx: &mut TestAppContext) -> Bounds<Pixels> {
        self.row_bounds_in(0, row, cx)
    }

    pub fn row_bounds_in(
        &self,
        level: usize,
        row: usize,
        cx: &mut TestAppContext,
    ) -> Bounds<Pixels> {
        self.with_level(level, cx, |window| window.find(("menu-row", row)).bounds())
    }

    pub fn level_bounds(&self, cx: &mut TestAppContext) -> Bounds<Pixels> {
        self.level_bounds_in(0, cx)
    }

    pub fn level_bounds_in(&self, level: usize, cx: &mut TestAppContext) -> Bounds<Pixels> {
        self.with_root(cx, |window| window.find(("menu-level", level)).bounds())
    }

    pub fn filter_text(&self, cx: &mut TestAppContext) -> String {
        self.menu
            .read_with(cx, |menu, cx| menu.query(cx).to_string())
    }

    fn with_root<R>(
        &self,
        cx: &mut TestAppContext,
        read: impl FnOnce(&mut gpui_kit::test::ScopedWindow<'_>) -> R,
    ) -> R {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            read(&mut window.within(ROOT))
        })
        .expect("window")
    }

    fn with_level<R>(
        &self,
        level: usize,
        cx: &mut TestAppContext,
        read: impl FnOnce(&mut gpui_kit::test::ScopedWindow<'_>) -> R,
    ) -> R {
        self.with_root(cx, |root| read(&mut root.within(("menu-level", level))))
    }
}
