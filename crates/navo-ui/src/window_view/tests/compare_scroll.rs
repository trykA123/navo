//! Compare panes share one scroll offset: every way to move it must repaint both cached panes.

use gpui_kit::AppContext as _;
use gpui_kit::test::TestQueryExt;
use gpui_kit::{
    AnyWindowHandle, InputEvent as _, Modifiers, Role, ScrollDelta, ScrollEvent, TestAppContext,
    point, px,
};

use super::compare::{Rig, first_labels, frames, rig};

/// A rig comparing two panes of 500 identical-looking files, settled and unscrolled.
fn scrolled_rig(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    let names: Vec<String> = (0..500).map(|n| format!("f{n:04}.txt")).collect();
    rig.send_files(cx, &names.iter().map(String::as_str).collect::<Vec<_>>());
    frames(cx, rig.window);
    let [left, right] = first_labels(cx, rig.window);
    assert!(
        left.starts_with("f0000") && right.starts_with("f0000"),
        "{left} / {right}"
    );
    rig
}

/// Turns the wheel down over the first row of the left (`right` false) or right pane.
fn wheel_over(cx: &mut TestAppContext, window: AnyWindowHandle, right: bool) {
    let at = cx
        .update_window(window, |_, window, _| {
            let edge = window.viewport_size().width / 2.;
            let items = window.find_all_by_role(Role::ListItem);
            let item = items
                .iter()
                .find(|item| (item.bounds().origin.x >= edge) == right)
                .expect("a row");
            item.bounds().center()
        })
        .expect("window");
    let none = Modifiers::default();
    let wheel = ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(-2000.))),
        modifiers: none,
        ..Default::default()
    };
    frames(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(crate::test_pointer::moved(None, at, none), cx);
    })
    .expect("window");
    frames(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(wheel.to_platform_input(), cx);
    })
    .expect("window");
    frames(cx, window);
}

#[gpui_kit::test]
fn a_wheel_turn_over_the_right_compare_pane_moves_the_left_pane_too(cx: &mut TestAppContext) {
    let rig = scrolled_rig(cx);
    wheel_over(cx, rig.window, true);
    let [left, right] = first_labels(cx, rig.window);
    assert!(
        !right.starts_with("f0000"),
        "the right pane scrolled: {right}"
    );
    assert_eq!(left, right, "both panes show the same rows");
}

#[gpui_kit::test]
fn keys_moving_past_the_visible_compare_rows_scroll_both_panes(cx: &mut TestAppContext) {
    let rig = scrolled_rig(cx);
    for _ in 0..60 {
        rig.press(cx, "down");
    }
    frames(cx, rig.window);
    let [left, right] = first_labels(cx, rig.window);
    assert!(!left.starts_with("f0000"), "the left pane scrolled: {left}");
    assert_eq!(left, right, "both panes show the same rows");
}
