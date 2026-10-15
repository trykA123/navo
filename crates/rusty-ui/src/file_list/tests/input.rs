use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ScrollDelta, TestAppContext, point, px};
use rusty_core::{Command, CursorMove, ViewState};

use super::{actions, commands, generated_rows, open_list, props, sample_rows};
use crate::file_list::FileListAction;

fn cursor(to: CursorMove) -> Command {
    Command::PaneCursorMove { to }
}

#[gpui_kit::test]
fn navigation_keys_send_cursor_moves(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        for key in ["down", "up", "pagedown", "pageup", "end", "home"] {
            window.press(key, cx);
        }
    })
    .expect("window");
    let expected = [
        CursorMove::Down,
        CursorMove::Up,
        CursorMove::PageDown,
        CursorMove::PageUp,
        CursorMove::Last,
        CursorMove::First,
    ]
    .map(cursor);
    assert_eq!(commands(&harness, cx), expected);
}

#[gpui_kit::test]
fn space_and_insert_toggle_the_selection(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.press("space", cx);
        window.press("insert", cx);
    })
    .expect("window");
    assert_eq!(
        commands(&harness, cx),
        [Command::SelectionToggle, Command::SelectionToggleDown]
    );
}

#[gpui_kit::test]
fn a_click_moves_the_cursor_to_the_row(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.within("left").click(("row", 3usize), cx);
    })
    .expect("window");
    assert_eq!(commands(&harness, cx), [cursor(CursorMove::Row(3))]);
}

#[gpui_kit::test]
fn a_right_click_moves_the_cursor_then_toggles_the_row(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.within("left").right_click(("row", 4usize), cx);
    })
    .expect("window");
    assert_eq!(
        commands(&harness, cx),
        [cursor(CursorMove::Row(4)), Command::SelectionToggle]
    );
}

#[gpui_kit::test]
fn the_wheel_sends_the_new_top_row(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(generated_rows(1_000)));
    cx.update_window(window, |_, window, cx| {
        let ten_rows = ScrollDelta::Pixels(point(px(0.), px(-240.)));
        window.within("left").scroll("list", ten_rows, cx);
    })
    .expect("window");
    assert_eq!(commands(&harness, cx), [Command::PaneScroll { to: 10 }]);
}

#[gpui_kit::test]
fn enter_opens_a_folder(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| window.press("enter", cx))
        .expect("window");
    assert_eq!(actions(&harness, cx), [FileListAction::Open(0)]);
}

#[gpui_kit::test]
fn enter_on_a_file_sends_nothing(cx: &mut TestAppContext) {
    let mut on_file = props(sample_rows());
    on_file.view = ViewState {
        cursor: 2,
        scroll: 0,
    };
    let (window, harness) = open_list(cx, on_file);
    cx.update_window(window, |_, window, cx| window.press("enter", cx))
        .expect("window");
    assert_eq!(actions(&harness, cx), []);
    assert_eq!(commands(&harness, cx), []);
}

#[gpui_kit::test]
fn backspace_asks_for_the_parent_folder(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| window.press("backspace", cx))
        .expect("window");
    assert_eq!(actions(&harness, cx), [FileListAction::Up]);
}

#[gpui_kit::test]
fn page_rows_follow_the_list_height(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(generated_rows(1_000)));
    harness.update(cx, |harness, cx| {
        harness.height = px(264.);
        cx.notify();
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    let pages = harness.read_with(cx, |harness, _| harness.page_rows.clone());
    assert_eq!(pages, [19, 10]);
}

#[gpui_kit::test]
fn tab_asks_to_switch_panes(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| window.press("tab", cx))
        .expect("window");
    assert_eq!(actions(&harness, cx), [FileListAction::SwitchPane]);
}
