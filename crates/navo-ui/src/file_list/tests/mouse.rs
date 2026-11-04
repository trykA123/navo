use std::sync::Arc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Bounds, Modifiers, MouseButton, Pixels, Point, TestAppContext, point, px,
};
use navo_core::{Command, CursorMove, Selection, SpanMode};

use super::{actions, commands, open_list, open_list_with, props, sample_rows};
use crate::file_list::{FileListAction, RightButton};
use crate::test_pointer::{self, ctrl, dispatch, down, moved, shift, up};

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

fn extend(row: u64, mode: SpanMode) -> Command {
    Command::SelectionExtend {
        to: CursorMove::Row(row),
        mode,
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
    assert_eq!(
        commands(&harness, cx),
        [extend(3, SpanMode::Mark), extend(1, SpanMode::Mark)]
    );
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
fn a_click_below_the_last_row_sends_nothing(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let at = below(cx, window);
    test_pointer::click(cx, window, (at, ctrl()));
    test_pointer::click(cx, window, (at, Modifiers::default()));
    assert_eq!(commands(&harness, cx), []);
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
    let expected = [
        cursor(1),
        cursor(1),
        extend(3, SpanMode::Mark),
        extend(2, SpanMode::Mark),
    ];
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
    marked.marked = Arc::new(selection);
    let (window, harness) = open_list(cx, marked);
    right_drag(cx, window, &[1, 3]);
    assert_eq!(
        commands(&harness, cx),
        [cursor(1), cursor(1), extend(3, SpanMode::Unmark)]
    );
}

fn marked_props(rows: &[u64]) -> crate::file_list::FileListProps {
    let mut shown = props(sample_rows());
    let mut selection = Selection::new(6);
    for row in rows {
        selection.toggle(*row);
    }
    shown.marked = Arc::new(selection);
    shown
}

fn prefetch(adds_cursor: bool) -> FileListAction {
    FileListAction::MenuPrefetch {
        adds_cursor,
        extended: false,
    }
}

fn menu_at(at: Point<Pixels>) -> FileListAction {
    FileListAction::ContextMenu {
        at,
        extended: false,
    }
}

#[gpui_kit::test]
fn in_windows_mode_a_right_click_opens_the_menu_on_release_only(cx: &mut TestAppContext) {
    let (window, harness) = open_list_with(cx, props(sample_rows()), RightButton::Windows);
    let at = row_bounds(cx, window, 2).center();
    let right = Modifiers::default();
    dispatch(cx, window, down(MouseButton::Right, at, right));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(600));
    cx.run_until_parked();
    assert_eq!(actions(&harness, cx), [prefetch(false)]);
    assert_eq!(commands(&harness, cx), [cursor(2)]);
    dispatch(cx, window, up(MouseButton::Right, at, right));
    assert_eq!(actions(&harness, cx), [prefetch(false), menu_at(at)]);
    assert_eq!(commands(&harness, cx), [cursor(2), cursor(2)]);
}

#[gpui_kit::test]
fn in_windows_mode_an_unmarked_row_joins_the_marks(cx: &mut TestAppContext) {
    let (window, harness) = open_list_with(cx, marked_props(&[1]), RightButton::Windows);
    let at = row_bounds(cx, window, 3).center();
    let right = Modifiers::default();
    dispatch(cx, window, down(MouseButton::Right, at, right));
    dispatch(cx, window, up(MouseButton::Right, at, right));
    let toggle = Command::SelectionToggle;
    assert_eq!(commands(&harness, cx), [cursor(3), cursor(3), toggle]);
    assert_eq!(actions(&harness, cx), [prefetch(true), menu_at(at)]);
}

#[gpui_kit::test]
fn in_windows_mode_a_marked_row_does_not_toggle(cx: &mut TestAppContext) {
    let (window, harness) = open_list_with(cx, marked_props(&[1, 3]), RightButton::Windows);
    let at = row_bounds(cx, window, 3).center();
    let right = Modifiers::default();
    dispatch(cx, window, down(MouseButton::Right, at, right));
    dispatch(cx, window, up(MouseButton::Right, at, right));
    assert_eq!(commands(&harness, cx), [cursor(3), cursor(3)]);
}

#[gpui_kit::test]
fn in_windows_mode_a_right_drag_marks_nothing_and_opens_no_menu(cx: &mut TestAppContext) {
    let (window, harness) = open_list_with(cx, props(sample_rows()), RightButton::Windows);
    right_drag(cx, window, &[1, 3, 2]);
    assert_eq!(commands(&harness, cx), [cursor(1)]);
    assert_eq!(
        actions(&harness, cx),
        [prefetch(false), FileListAction::MenuCancel]
    );
}

#[gpui_kit::test]
fn in_windows_mode_a_same_row_right_drag_past_the_threshold_opens_no_menu(cx: &mut TestAppContext) {
    let (window, harness) = open_list_with(cx, props(sample_rows()), RightButton::Windows);
    let at = row_bounds(cx, window, 2).center();
    let right = Modifiers::default();
    let end = at + point(px(12.), px(0.));
    dispatch(cx, window, down(MouseButton::Right, at, right));
    dispatch(cx, window, moved(Some(MouseButton::Right), end, right));
    dispatch(cx, window, up(MouseButton::Right, end, right));
    let expected = [prefetch(false), FileListAction::MenuCancel];
    assert_eq!(actions(&harness, cx), expected);
    assert_eq!(commands(&harness, cx), [cursor(2)]);
}

#[gpui_kit::test]
fn in_windows_mode_a_right_wiggle_inside_the_threshold_still_opens_the_menu(
    cx: &mut TestAppContext,
) {
    let (window, harness) = open_list_with(cx, props(sample_rows()), RightButton::Windows);
    let at = row_bounds(cx, window, 2).center();
    let right = Modifiers::default();
    let end = at + point(px(2.), px(1.));
    dispatch(cx, window, down(MouseButton::Right, at, right));
    dispatch(cx, window, moved(Some(MouseButton::Right), end, right));
    dispatch(cx, window, up(MouseButton::Right, end, right));
    assert_eq!(actions(&harness, cx), [prefetch(false), menu_at(at)]);
}

#[gpui_kit::test]
fn in_windows_mode_a_release_outside_the_list_cancels_the_menu(cx: &mut TestAppContext) {
    let (window, harness) = open_list_with(cx, props(sample_rows()), RightButton::Windows);
    let at = row_bounds(cx, window, 2).center();
    let right = Modifiers::default();
    dispatch(cx, window, down(MouseButton::Right, at, right));
    dispatch(
        cx,
        window,
        up(MouseButton::Right, point(px(-40.), px(-40.)), right),
    );
    let expected = [prefetch(false), FileListAction::MenuCancel];
    assert_eq!(actions(&harness, cx), expected);
    assert_eq!(commands(&harness, cx), [cursor(2)]);
}
