//! Tests of the start row menu: a right click selects and opens three lines, Delete forgets a
//! recent row, and a named row dims "Remove from recent".

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    TestAppContext, VisualTestContext,
};
use navo_compare_profile::recent::StartRows;

use crate::menu_host::menu_view;
use crate::start::StartEvent;
use crate::start_tests::{Screen, TempDir, act, record, row, screen};

/// A screen with two recent rows and one named row.
fn setup(cx: &mut TestAppContext, name: &str) -> (Screen, TempDir) {
    let dir = TempDir::new(name);
    let mut named = row("n1", "e", "f");
    named.name = Some("Release".into());
    let rows = StartRows {
        recent: vec![row("r1", "a", "b"), row("r2", "c", "d")],
        named: vec![named],
    };
    let s = screen(cx, rows, dir.path());
    cx.update(|cx| {
        crate::menu_host::bind_menu_keys(cx);
    });
    (s, dir)
}

/// Presses and releases the right button on the centre of row `index`.
fn right_click(cx: &mut TestAppContext, window: AnyWindowHandle, index: usize) {
    let at = cx
        .update_window(window, |_, w, _| {
            w.find(("start-row", index)).bounds().center()
        })
        .expect("window");
    let mut vcx = VisualTestContext::from_window(window, cx);
    vcx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        button: MouseButton::Right,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
}

/// The enabled flags of the open menu's lines.
fn enabled(cx: &mut TestAppContext, s: &Screen) -> Vec<bool> {
    let menu = s.1.read_with(cx, |screen, _| menu_view(&screen.menu));
    let menu = menu.expect("open menu");
    menu.read_with(cx, |menu, _| {
        menu.menu().items.iter().map(|item| item.enabled).collect()
    })
}

#[gpui_kit::test]
fn right_click_selects_and_shows_three_lines(cx: &mut TestAppContext) {
    let (s, _dir) = setup(cx, "menu-open");
    right_click(cx, s.0, 1);
    cx.run_until_parked();
    assert_eq!(s.1.read_with(cx, |screen, _| screen.selected), Some(1));
    assert_eq!(enabled(cx, &s), [true, true, true]);
}

#[gpui_kit::test]
fn delete_emits_forget_for_a_recent_row(cx: &mut TestAppContext) {
    let (s, _dir) = setup(cx, "menu-delete");
    let log = record(cx, &s.1);
    act(cx, &s, |screen, window, cx| screen.enter_list(window, cx));
    cx.simulate_keystrokes(s.0, "down delete");
    assert!(matches!(
        log.borrow().as_slice(),
        [StartEvent::Forget { id }] if id == "r2"
    ));
    cx.simulate_keystrokes(s.0, "down delete");
    assert_eq!(log.borrow().len(), 1, "a named row is not forgotten");
}

#[gpui_kit::test]
fn a_named_row_dims_remove(cx: &mut TestAppContext) {
    let (s, _dir) = setup(cx, "menu-named");
    right_click(cx, s.0, 2);
    cx.run_until_parked();
    assert_eq!(enabled(cx, &s), [true, true, false]);
}

/// Clicks line `line` of the open menu with the left button and settles.
fn click_line(cx: &mut TestAppContext, window: AnyWindowHandle, line: usize) {
    cx.update_window(window, |_, w, cx| {
        w.render_frame(cx);
        w.within("context-menu")
            .within(("menu-level", 0usize))
            .click(("menu-row", line), cx);
    })
    .expect("window");
    cx.run_until_parked();
}

/// Whether the row list holds the focus.
fn list_focused(cx: &mut TestAppContext, s: &Screen) -> bool {
    cx.update_window(s.0, |_, w, cx| s.1.read(cx).list_focus.is_focused(w))
        .expect("window")
}

/// A mouse-chosen Copy paths gives the focus back to the row list and keeps the selection.
#[gpui_kit::test]
fn copy_paths_by_mouse_refocuses_the_list(cx: &mut TestAppContext) {
    let (s, _dir) = setup(cx, "menu-copy-focus");
    act(cx, &s, |screen, window, cx| screen.enter_list(window, cx));
    right_click(cx, s.0, 1);
    cx.run_until_parked();
    click_line(cx, s.0, 1);
    assert!(s.1.read_with(cx, |screen, _| screen.menu.is_none()));
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("c\nd"));
    assert!(
        list_focused(cx, &s),
        "the list had the focus when the menu opened"
    );
    assert_eq!(s.1.read_with(cx, |screen, _| screen.selected), Some(1));
}

/// Escape closes the menu and gives the focus back to the row list.
#[gpui_kit::test]
fn escape_refocuses_the_list(cx: &mut TestAppContext) {
    let (s, _dir) = setup(cx, "menu-escape-focus");
    act(cx, &s, |screen, window, cx| screen.enter_list(window, cx));
    right_click(cx, s.0, 1);
    cx.run_until_parked();
    cx.simulate_keystrokes(s.0, "escape");
    cx.run_until_parked();
    assert!(s.1.read_with(cx, |screen, _| screen.menu.is_none()));
    assert!(list_focused(cx, &s));
    assert_eq!(s.1.read_with(cx, |screen, _| screen.selected), Some(1));
}
