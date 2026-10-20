use std::rc::Rc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Bounds, Modifiers, MouseButton, Pixels, Point, TestAppContext, point, px,
};
use rusty_core::{Command, CursorMove, Selection};

use super::{actions, commands, open_list, props, sample_rows};
use crate::file_list::FileListAction;
use crate::test_pointer::{self, ctrl, dispatch, down, shift, up};

pub(super) const LAST_ROW: usize = 5;

pub(super) fn row_bounds(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    row: usize,
) -> Bounds<Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("left").find(("row", row)).bounds()
    })
    .expect("window")
}

pub(super) fn icon_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    row: usize,
) -> Point<Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut list = window.within("left");
        list.within(("row", row)).find("row-icon").bounds().center()
    })
    .expect("window")
}

pub(super) fn gap_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    row: usize,
) -> Point<Pixels> {
    let bounds = row_bounds(cx, window, row);
    point(bounds.left() + px(300.), bounds.center().y)
}

pub(super) fn below(cx: &mut TestAppContext, window: AnyWindowHandle) -> Point<Pixels> {
    let bounds = row_bounds(cx, window, LAST_ROW);
    point(bounds.left() + px(300.), bounds.bottom() + px(60.))
}

fn cursor(row: u64) -> Command {
    Command::PaneCursorMove {
        to: CursorMove::Row(row),
    }
}

fn extend(row: u64, unmark: bool) -> Command {
    Command::SelectionExtend {
        to: CursorMove::Row(row),
        unmark,
    }
}

fn click_row(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize, modifiers: Modifiers) {
    let at = row_bounds(cx, window, row).center();
    test_pointer::click(cx, window, (at, modifiers));
}

#[gpui_kit::test]
fn ctrl_click_toggles_two_rows_and_moves_the_cursor(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    click_row(cx, window, 2, ctrl());
    click_row(cx, window, 4, ctrl());
    let toggle = Command::SelectionToggle;
    let expected = [cursor(2), toggle.clone(), cursor(4), toggle];
    assert_eq!(commands(&harness, cx), expected);
}

#[gpui_kit::test]
fn shift_click_and_ctrl_shift_click_extend_from_the_cursor(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    click_row(cx, window, 3, shift());
    let both = Modifiers {
        control: true,
        shift: true,
        ..Modifiers::default()
    };
    click_row(cx, window, 1, both);
    assert_eq!(commands(&harness, cx), [extend(3, false), extend(1, false)]);
}

#[gpui_kit::test]
fn a_mouse_down_with_ctrl_or_shift_does_not_move_the_cursor(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    for modifiers in [ctrl(), shift()] {
        let at = row_bounds(cx, window, 3).center();
        dispatch(cx, window, down(MouseButton::Left, at, modifiers));
        assert_eq!(commands(&harness, cx), []);
    }
}

#[gpui_kit::test]
fn a_plain_click_moves_the_cursor_on_mouse_down(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let at = row_bounds(cx, window, 3).center();
    dispatch(
        cx,
        window,
        down(MouseButton::Left, at, Modifiers::default()),
    );
    assert_eq!(commands(&harness, cx), [cursor(3)]);
    dispatch(cx, window, up(MouseButton::Left, at, Modifiers::default()));
    assert_eq!(commands(&harness, cx), [cursor(3)]);
}

#[gpui_kit::test]
fn a_plain_click_below_the_last_row_unmarks_all(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let at = below(cx, window);
    test_pointer::click(cx, window, (at, ctrl()));
    assert_eq!(commands(&harness, cx), []);
    test_pointer::click(cx, window, (at, Modifiers::default()));
    assert_eq!(commands(&harness, cx), [Command::SelectionClear]);
}

fn right_drag(cx: &mut TestAppContext, window: AnyWindowHandle, rows: &[usize]) {
    let path: Vec<_> = rows
        .iter()
        .map(|row| row_bounds(cx, window, *row).center())
        .collect();
    let right = (MouseButton::Right, Modifiers::default());
    test_pointer::drag(cx, window, right, &path);
    let last = path.last().copied().unwrap_or_default();
    dispatch(
        cx,
        window,
        up(MouseButton::Right, last, Modifiers::default()),
    );
}

#[gpui_kit::test]
fn a_right_drag_paints_a_span_and_shrinks_back_without_a_toggle(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    right_drag(cx, window, &[1, 3, 2]);
    let expected = [cursor(1), cursor(1), extend(3, false), extend(2, false)];
    assert_eq!(commands(&harness, cx), expected);
    let cancels = actions(&harness, cx)
        .into_iter()
        .filter(|action| *action == FileListAction::MenuCancel)
        .count();
    assert_eq!(cancels, 1);
}

#[gpui_kit::test]
fn a_right_drag_from_a_marked_row_unmarks(cx: &mut TestAppContext) {
    let mut marked = props(sample_rows());
    let mut selection = Selection::new(6);
    selection.toggle(1);
    marked.marked = Rc::new(selection);
    let (window, harness) = open_list(cx, marked);
    right_drag(cx, window, &[1, 3]);
    assert_eq!(
        commands(&harness, cx),
        [cursor(1), cursor(1), extend(3, true)]
    );
}
