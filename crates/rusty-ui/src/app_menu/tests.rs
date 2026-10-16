use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, TestAppContext, Window, div,
};

use super::{
    AppMenu, AppMenuEvent, ITEM_COUNT, Identity, Install, Item, MenuCommand, Ring, Setting, Step,
    index_of, item_at, jump,
};
use crate::test_window;
use crate::tokens::{self, ThemeChoice};

const RING: Ring = Ring {
    tiles: 5,
    rows: 6,
    columns: 3,
};

#[test]
fn right_and_left_walk_the_whole_ring_and_wrap() {
    assert_eq!(RING.step(0, Step::Right), 1);
    assert_eq!(RING.step(4, Step::Right), 5);
    assert_eq!(RING.step(10, Step::Right), 0);
    assert_eq!(RING.step(0, Step::Left), 10);
    assert_eq!(RING.step(5, Step::Left), 4);
}

#[test]
fn down_moves_a_grid_row_then_into_the_menu_rows() {
    assert_eq!(RING.step(0, Step::Down), 3);
    assert_eq!(RING.step(1, Step::Down), 4);
    assert_eq!(RING.step(2, Step::Down), 5);
    assert_eq!(RING.step(3, Step::Down), 5);
    assert_eq!(RING.step(5, Step::Down), 6);
    assert_eq!(RING.step(10, Step::Down), 0);
}

#[test]
fn up_moves_a_grid_row_and_leaves_the_rows_for_the_last_tile() {
    assert_eq!(RING.step(4, Step::Up), 1);
    assert_eq!(RING.step(5, Step::Up), 4);
    assert_eq!(RING.step(6, Step::Up), 5);
    assert_eq!(RING.step(0, Step::Up), 10);
    assert_eq!(RING.step(2, Step::Up), 10);
}

#[test]
fn a_single_tile_still_joins_the_ring() {
    let ring = Ring {
        tiles: 1,
        rows: 6,
        columns: 3,
    };
    assert_eq!(ring.step(0, Step::Down), 1);
    assert_eq!(ring.step(1, Step::Up), 0);
    assert_eq!(ring.step(0, Step::Up), 6);
}

#[test]
fn a_letter_jumps_to_the_next_row_starting_with_it() {
    let labels = ["Command palette", "New window", "Copy", "Quit"];
    assert_eq!(jump(&labels, None, "q"), Some(3));
    assert_eq!(jump(&labels, None, "C"), Some(0));
    assert_eq!(jump(&labels, Some(0), "c"), Some(2));
    assert_eq!(jump(&labels, Some(2), "c"), Some(0));
    assert_eq!(jump(&labels, None, "z"), None);
    assert_eq!(jump(&labels, None, "space"), None);
    assert_eq!(jump(&labels, None, ","), None);
}

#[test]
fn the_tiles_come_first_then_the_six_rows() {
    assert_eq!(item_at(0), Some(Item::Tile(Setting::Theme)));
    assert_eq!(item_at(1), Some(Item::Row(MenuCommand::CommandPalette)));
    assert_eq!(
        item_at(ITEM_COUNT - 1),
        Some(Item::Row(MenuCommand::AllSettings))
    );
    assert_eq!(item_at(ITEM_COUNT), None);
    assert_eq!(index_of(Item::Row(MenuCommand::Quit)), Some(5));
}

#[test]
fn only_quit_runs_today() {
    let available: Vec<MenuCommand> = (0..ITEM_COUNT)
        .filter_map(item_at)
        .filter_map(|item| match item {
            Item::Row(command) if command.is_available() => Some(command),
            _ => None,
        })
        .collect();
    assert_eq!(available, [MenuCommand::Quit]);
}

struct Shell {
    theme: ThemeChoice,
    selected: usize,
    focus: FocusHandle,
    events: Rc<RefCell<Vec<AppMenuEvent>>>,
}

fn identity() -> Identity {
    Identity {
        version: "0.4.2".into(),
        install: Install::Portable,
        folder: r"D:\Apps\Ferrite".into(),
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let menu = AppMenu::new("app-menu", identity(), self.theme)
            .selected(self.selected)
            .track_focus(&self.focus)
            .on_event(
                cx.listener(|shell: &mut Self, event: &AppMenuEvent, _, cx| {
                    shell.events.borrow_mut().push(*event);
                    if let AppMenuEvent::Select(index) = event {
                        shell.selected = *index;
                    }
                    if *event == AppMenuEvent::Toggle(Setting::Theme) {
                        shell.theme = shell.theme.next();
                    }
                    cx.notify();
                }),
            );
        div()
            .relative()
            .size_full()
            .child(div().id("outside").size_full())
            .child(menu.into_overlay(&t))
    }
}

struct Rig {
    window: AnyWindowHandle,
    shell: Entity<Shell>,
    events: Rc<RefCell<Vec<AppMenuEvent>>>,
}

fn open(cx: &mut TestAppContext) -> Rig {
    let events: Rc<RefCell<Vec<AppMenuEvent>>> = Rc::default();
    let log = events.clone();
    let (window, shell) = test_window::open(cx, move |window, cx| {
        super::bind_keys(cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Shell {
            theme: ThemeChoice::System,
            selected: 0,
            focus,
            events: log,
        }
    });
    Rig {
        window,
        shell,
        events,
    }
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, key: &str) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.press(key, cx);
            window.render_frame(cx);
        })
        .expect("window");
    }

    fn click(&self, cx: &mut TestAppContext, id: (&'static str, usize)) {
        cx.update_window(self.window, |_, window, cx| window.click(id, cx))
            .expect("window");
    }

    fn selected(&self, cx: &mut TestAppContext) -> usize {
        self.shell.read_with(cx, |shell, _| shell.selected)
    }

    fn take(&self) -> Vec<AppMenuEvent> {
        self.events.borrow_mut().drain(..).collect()
    }
}

#[gpui_kit::test]
fn the_menu_shows_the_identity_the_theme_tile_and_six_rows(cx: &mut TestAppContext) {
    let rig = open(cx);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let menu = window.find("app-menu");
        assert_eq!(menu.label(), Some("Ferrite menu"));
        assert_eq!(
            window.find("app-menu-identity").label(),
            Some(r"Ferrite 0.4.2, Portable · D:\Apps\Ferrite")
        );
        let tile = window.find(("app-menu-tile", 0usize));
        assert_eq!(tile.label(), Some("Theme"));
        assert_eq!(tile.value(), Some("System"));
        assert_eq!(tile.selected(), Some(true));
        let labels: Vec<String> = (1..ITEM_COUNT)
            .map(|index| {
                let row = window.find(("app-menu-row", index));
                row.label().unwrap_or_default().to_owned()
            })
            .collect();
        assert_eq!(
            labels,
            [
                "Command palette",
                "New window",
                "Workspaces",
                "Help and key map",
                "Quit",
                "All settings"
            ]
        );
        assert!(window.try_find(("app-menu-tile", 1usize)).is_none());
    })
    .expect("window");
}

#[gpui_kit::test]
fn unavailable_rows_are_shown_and_marked(cx: &mut TestAppContext) {
    let rig = open(cx);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        for index in 1..ITEM_COUNT {
            let row = window.find(("app-menu-row", index));
            assert!(row.visible(), "row {index} is hidden");
            let expected = (index != 5).then_some("Not available yet");
            assert_eq!(row.value(), expected, "row {index}");
        }
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_menu_sits_under_the_title_bar_on_the_left(cx: &mut TestAppContext) {
    let rig = open(cx);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let t = tokens::current(cx);
        let bounds = window.find("app-menu").bounds();
        assert_eq!(bounds.left(), t.spacing.md);
        assert!(bounds.top() >= t.spacing.header_height - t.spacing.md);
        assert!(bounds.top() <= t.spacing.header_height);
    })
    .expect("window");
}

#[gpui_kit::test]
fn arrows_move_one_focus_through_the_tile_and_the_rows(cx: &mut TestAppContext) {
    let rig = open(cx);
    rig.press(cx, "down");
    assert_eq!(rig.selected(cx), 1);
    rig.press(cx, "down");
    rig.press(cx, "right");
    assert_eq!(rig.selected(cx), 3);
    rig.press(cx, "up");
    rig.press(cx, "up");
    rig.press(cx, "up");
    assert_eq!(rig.selected(cx), 0);
    rig.press(cx, "left");
    assert_eq!(rig.selected(cx), ITEM_COUNT - 1);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let last = window.find(("app-menu-row", ITEM_COUNT - 1));
        assert_eq!(last.selected(), Some(true));
        assert_eq!(
            window.find(("app-menu-tile", 0usize)).selected(),
            Some(false)
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn enter_and_space_cycle_the_theme_tile(cx: &mut TestAppContext) {
    let rig = open(cx);
    rig.press(cx, "enter");
    rig.press(cx, "space");
    assert_eq!(
        rig.take(),
        [
            AppMenuEvent::Toggle(Setting::Theme),
            AppMenuEvent::Toggle(Setting::Theme)
        ]
    );
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find(("app-menu-tile", 0usize)).value(), Some("Dark"));
    })
    .expect("window");
}

#[gpui_kit::test]
fn clicking_the_tile_toggles_it(cx: &mut TestAppContext) {
    let rig = open(cx);
    rig.click(cx, ("app-menu-tile", 0));
    assert!(rig.take().contains(&AppMenuEvent::Toggle(Setting::Theme)));
}

#[gpui_kit::test]
fn unavailable_rows_do_nothing_when_activated(cx: &mut TestAppContext) {
    let rig = open(cx);
    for index in [1usize, 2, 3, 4, 6] {
        rig.click(cx, ("app-menu-row", index));
        rig.press(cx, "enter");
    }
    let acted: Vec<AppMenuEvent> = rig
        .take()
        .into_iter()
        .filter(|event| !matches!(event, AppMenuEvent::Select(_)))
        .collect();
    assert!(acted.is_empty(), "unexpected {acted:?}");
}

#[gpui_kit::test]
fn quit_runs_from_enter_and_from_a_click(cx: &mut TestAppContext) {
    let rig = open(cx);
    rig.press(cx, "q");
    assert_eq!(rig.selected(cx), 5);
    rig.press(cx, "enter");
    rig.click(cx, ("app-menu-row", 5));
    let runs = rig
        .take()
        .into_iter()
        .filter(|event| *event == AppMenuEvent::Run(MenuCommand::Quit))
        .count();
    assert_eq!(runs, 2);
}

#[gpui_kit::test]
fn a_letter_jumps_to_its_row(cx: &mut TestAppContext) {
    let rig = open(cx);
    rig.press(cx, "h");
    assert_eq!(rig.selected(cx), 4);
    rig.press(cx, "a");
    assert_eq!(rig.selected(cx), 6);
    rig.press(cx, "x");
    assert_eq!(rig.selected(cx), 6);
}

#[gpui_kit::test]
fn escape_and_a_click_outside_close_the_menu(cx: &mut TestAppContext) {
    let rig = open(cx);
    rig.press(cx, "escape");
    assert_eq!(rig.take(), [AppMenuEvent::Close]);
    cx.update_window(rig.window, |_, window, cx| {
        window.click("app-menu-backdrop", cx)
    })
    .expect("window");
    assert_eq!(rig.take(), [AppMenuEvent::Close]);
}

#[gpui_kit::test]
fn a_click_inside_the_menu_does_not_close_it(cx: &mut TestAppContext) {
    let rig = open(cx);
    cx.update_window(rig.window, |_, window, cx| {
        window.click("app-menu-identity", cx)
    })
    .expect("window");
    assert!(!rig.take().contains(&AppMenuEvent::Close));
}
