use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, Entity, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Pixels, Point, Render, Role, Styled, TestAppContext, Window, div,
    point, px, size,
};
use rusty_core::shell_menu::{
    ActionContext, Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu, ferrite_actions,
};

use super::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::test_window::open;

mod filter;
mod keys;
mod place;
mod submenu;

pub(super) const ROOT: &str = "context-menu";

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

fn lazy(command: u32, raw: &str, group: Group) -> MenuItem {
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Lazy),
        ..shell(command, raw, group)
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
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(children)),
        ..shell(3, "7-Zip", Group::Extensions)
    }
}

pub(super) fn zip_menu() -> ShellMenu {
    let mut items = ferrite_actions(ActionContext {
        count: 1,
        one_per_pane: false,
    });
    items.extend([
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
        shell(7, "Cu&t\tCtrl+X", Group::Edit),
        shell(8, "&Copy\tCtrl+C", Group::Edit),
        MenuItem {
            enabled: false,
            ..shell(9, "&Paste\tCtrl+V", Group::Edit)
        },
        MenuItem {
            rare: true,
            ..shell(10, "Create &shortcut", Group::Other)
        },
        MenuItem {
            rare: true,
            ..shell(11, "Restore previous versions", Group::Other)
        },
    ]);
    ShellMenu { items }
}

pub(super) const SEND_TO: usize = 11;

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
    let events: Rc<RefCell<Vec<ContextMenuEvent>>> = Rc::default();
    let sink = events.clone();
    let (window, harness) = open(cx, move |window, cx| {
        let restore = cx.focus_handle();
        let input = ContextMenuInput {
            menu: zip_menu(),
            context: "ferrite-0.4.2-win64.zip · 6.8 MB".into(),
            anchor,
            bounds: window_bounds(),
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

    pub fn click(&self, depth: usize, row: usize, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window
                .within(ROOT)
                .within(("menu-level", depth))
                .click(("menu-row", row), cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    pub fn hover(&self, depth: usize, row: usize, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window
                .within(ROOT)
                .within(("menu-level", depth))
                .hover(("menu-row", row), cx);
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

    pub fn rows(&self, depth: usize, cx: &mut TestAppContext) -> Vec<String> {
        self.with_level(depth, cx, |window| {
            (0usize..)
                .map_while(|index| window.try_find(("menu-row", index)))
                .map(|row| match (row.role(), row.label()) {
                    (Some(Role::Heading), Some(label)) => format!("# {label}"),
                    (_, Some(label)) => label.to_owned(),
                    (_, None) => "-".to_owned(),
                })
                .collect()
        })
    }

    pub fn selected(&self, depth: usize, cx: &mut TestAppContext) -> Option<String> {
        self.with_level(depth, cx, |window| {
            (0usize..)
                .map_while(|index| window.try_find(("menu-row", index)))
                .find(|row| row.selected() == Some(true))
                .and_then(|row| row.label().map(str::to_owned))
        })
    }

    pub fn find(&self, id: (&'static str, usize), cx: &mut TestAppContext) -> Option<String> {
        self.with_level(0, cx, |window| {
            window
                .try_find(id)
                .map(|found| found.label().unwrap_or_default().to_owned())
        })
    }

    pub fn has_level(&self, depth: usize, cx: &mut TestAppContext) -> bool {
        self.with_root(cx, |window| {
            window.try_find(("menu-level", depth)).is_some()
        })
    }

    pub fn level_bounds(&self, depth: usize, cx: &mut TestAppContext) -> Bounds<Pixels> {
        self.with_root(cx, |window| window.find(("menu-level", depth)).bounds())
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
        depth: usize,
        cx: &mut TestAppContext,
        read: impl FnOnce(&mut gpui_kit::test::ScopedWindow<'_>) -> R,
    ) -> R {
        self.with_root(cx, |root| read(&mut root.within(("menu-level", depth))))
    }
}
