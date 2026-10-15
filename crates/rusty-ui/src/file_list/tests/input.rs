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
fn enter_on_a_file_opens_it(cx: &mut TestAppContext) {
    let mut on_file = props(sample_rows());
    on_file.view = ViewState {
        cursor: 2,
        scroll: 0,
    };
    let (window, harness) = open_list(cx, on_file);
    cx.update_window(window, |_, window, cx| window.press("enter", cx))
        .expect("window");
    assert_eq!(actions(&harness, cx), [FileListAction::Open(2)]);
}

#[gpui_kit::test]
fn a_double_click_moves_the_cursor_and_opens_the_row(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.within("left").double_click(("row", 3usize), cx);
    })
    .expect("window");
    assert_eq!(actions(&harness, cx), [FileListAction::Open(3)]);
    assert!(commands(&harness, cx).contains(&cursor(CursorMove::Row(3))));
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

fn pressed(key: &str, cx: &mut TestAppContext) -> Vec<FileListAction> {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| window.press(key, cx))
        .expect("window");
    actions(&harness, cx)
}

#[gpui_kit::test]
fn f5_asks_to_copy(cx: &mut TestAppContext) {
    assert_eq!(
        pressed("f5", cx),
        [FileListAction::Operate(rusty_core::OpKind::Copy)]
    );
}

#[gpui_kit::test]
fn f6_asks_to_move(cx: &mut TestAppContext) {
    assert_eq!(
        pressed("f6", cx),
        [FileListAction::Operate(rusty_core::OpKind::Move)]
    );
}

#[gpui_kit::test]
fn f8_and_delete_ask_to_recycle(cx: &mut TestAppContext) {
    let recycle = [FileListAction::Operate(rusty_core::OpKind::Recycle)];
    assert_eq!(pressed("f8", cx), recycle);
    assert_eq!(pressed("delete", cx), recycle);
}

#[gpui_kit::test]
fn shift_delete_and_shift_f8_ask_to_delete_permanently(cx: &mut TestAppContext) {
    let delete = [FileListAction::Operate(rusty_core::OpKind::Delete)];
    assert_eq!(pressed("shift-delete", cx), delete);
    assert_eq!(pressed("shift-f8", cx), delete);
}

#[gpui_kit::test]
fn alt_arrows_go_back_and_forward(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.press("alt-left", cx);
        window.press("alt-right", cx);
    })
    .expect("window");
    assert_eq!(
        commands(&harness, cx),
        [Command::PaneBack, Command::PaneForward]
    );
}

#[gpui_kit::test]
fn the_mouse_side_buttons_go_back_and_forward(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, NavigationDirection};
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let position = window
            .within("left")
            .find(("row", 1usize))
            .bounds()
            .center();
        for direction in [NavigationDirection::Back, NavigationDirection::Forward] {
            let event = MouseDownEvent {
                button: MouseButton::Navigate(direction),
                position,
                modifiers: gpui_kit::Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            };
            window.dispatch_event(event.to_platform_input(), cx);
        }
    })
    .expect("window");
    assert_eq!(
        commands(&harness, cx),
        [Command::PaneBack, Command::PaneForward]
    );
}

fn filtered(text: &str) -> crate::file_list::FileListProps {
    crate::file_list::FileListProps {
        filter: Some(text.to_owned().into()),
        ..props(sample_rows())
    }
}

#[gpui_kit::test]
fn typed_letters_go_to_the_filter(cx: &mut TestAppContext) {
    use crate::file_list::FilterEdit;
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.press("p", cx);
        window.press("shift-a", cx);
        window.press("ctrl-p", cx);
    })
    .expect("window");
    assert_eq!(
        actions(&harness, cx),
        [
            FileListAction::Filter(FilterEdit::Push('p')),
            FileListAction::Filter(FilterEdit::Push('A')),
        ]
    );
}

#[gpui_kit::test]
fn backspace_edits_an_active_filter_instead_of_going_up(cx: &mut TestAppContext) {
    use crate::file_list::FilterEdit;
    let (window, harness) = open_list(cx, filtered("pa"));
    cx.update_window(window, |_, window, cx| window.press("backspace", cx))
        .expect("window");
    assert_eq!(
        actions(&harness, cx),
        [FileListAction::Filter(FilterEdit::Pop)]
    );
}

#[gpui_kit::test]
fn escape_clears_an_active_filter(cx: &mut TestAppContext) {
    use crate::file_list::FilterEdit;
    let (window, harness) = open_list(cx, filtered("pa"));
    cx.update_window(window, |_, window, cx| window.press("escape", cx))
        .expect("window");
    assert_eq!(
        actions(&harness, cx),
        [FileListAction::Filter(FilterEdit::Clear)]
    );
}

#[gpui_kit::test]
fn escape_without_a_filter_does_nothing(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| window.press("escape", cx))
        .expect("window");
    assert_eq!(actions(&harness, cx), []);
}

#[gpui_kit::test]
fn f2_and_shift_f6_ask_to_rename_the_cursor_row_and_f7_asks_for_a_folder(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.press("f2", cx);
        window.press("shift-f6", cx);
        window.press("f7", cx);
    })
    .expect("window");
    assert_eq!(
        actions(&harness, cx),
        [
            FileListAction::Rename(0),
            FileListAction::Rename(0),
            FileListAction::NewFolder,
        ]
    );
}

#[gpui_kit::test]
fn plus_minus_and_star_work_on_marks(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.press("+", cx);
        window.press("-", cx);
        window.press("*", cx);
    })
    .expect("window");
    assert_eq!(
        actions(&harness, cx),
        [
            FileListAction::SelectPattern { add: true },
            FileListAction::SelectPattern { add: false },
        ]
    );
    assert_eq!(commands(&harness, cx), [Command::SelectionInvert]);
}

#[gpui_kit::test]
fn plus_minus_and_star_go_to_an_active_filter(cx: &mut TestAppContext) {
    use crate::file_list::FilterEdit;
    let (window, harness) = open_list(cx, filtered("pa"));
    cx.update_window(window, |_, window, cx| {
        window.press("-", cx);
        window.press("+", cx);
        window.press("*", cx);
    })
    .expect("window");
    assert_eq!(
        actions(&harness, cx),
        [
            FileListAction::Filter(FilterEdit::Push('-')),
            FileListAction::Filter(FilterEdit::Push('+')),
            FileListAction::Filter(FilterEdit::Push('*')),
        ]
    );
    assert_eq!(commands(&harness, cx), []);
}

#[gpui_kit::test]
fn ctrl_f3_to_f6_sort_by_name_ext_date_and_size(cx: &mut TestAppContext) {
    use crate::file_list::SortBy;
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        for key in ["ctrl-f3", "ctrl-f4", "ctrl-f5", "ctrl-f6"] {
            window.press(key, cx);
        }
    })
    .expect("window");
    assert_eq!(
        actions(&harness, cx),
        [
            FileListAction::Sort(SortBy::Name),
            FileListAction::Sort(SortBy::Ext),
            FileListAction::Sort(SortBy::Date),
            FileListAction::Sort(SortBy::Size),
        ]
    );
}

#[gpui_kit::test]
fn a_header_click_sorts_by_that_column(cx: &mut TestAppContext) {
    use crate::file_list::SortBy;
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("left").click(("sort", 2usize), cx);
        window.within("left").click(("sort", 3usize), cx);
    })
    .expect("window");
    assert_eq!(
        actions(&harness, cx),
        [
            FileListAction::Sort(SortBy::Size),
            FileListAction::Sort(SortBy::Date),
        ]
    );
}
