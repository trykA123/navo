use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Modifiers, MouseButton, Pixels, Point, TestAppContext};
use rusty_core::{Command, CursorMove};

use super::mouse::{LAST_ROW, below, gap_of, icon_of, row_bounds};
use super::{commands, open_list, props, sample_rows};
use crate::test_pointer::{self, ctrl, dispatch, up};

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

fn centre(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize) -> Point<Pixels> {
    row_bounds(cx, window, row).center()
}

fn lasso(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    modifiers: Modifiers,
    path: &[Point<Pixels>],
) {
    test_pointer::drag(cx, window, (MouseButton::Left, modifiers), path);
}

fn release(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>) {
    dispatch(cx, window, up(MouseButton::Left, at, Modifiers::default()));
}

fn is_drawn(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("left").try_find("lasso").is_some()
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_lasso_from_below_marks_the_crossed_rows_and_shrinks_back(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let start = below(cx, window);
    let (row_three, row_four) = (centre(cx, window, 3), centre(cx, window, 4));
    lasso(cx, window, Modifiers::default(), &[start, row_three]);
    assert!(is_drawn(cx, window));
    let last = LAST_ROW as u64;
    assert_eq!(commands(&harness, cx), [cursor(last), extend(3, false)]);
    dispatch(
        cx,
        window,
        test_pointer::moved(Some(MouseButton::Left), row_four, Modifiers::default()),
    );
    release(cx, window, row_four);
    let expected = [cursor(last), extend(3, false), extend(4, false)];
    assert_eq!(commands(&harness, cx), expected);
    assert!(!is_drawn(cx, window));
}

#[gpui_kit::test]
fn a_lasso_keeps_earlier_marks_and_does_not_unmark_all(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let start = below(cx, window);
    let row_two = centre(cx, window, 2);
    lasso(cx, window, Modifiers::default(), &[start, row_two]);
    release(cx, window, row_two);
    let sent = commands(&harness, cx);
    assert!(!sent.contains(&Command::SelectionClear), "{sent:?}");
    assert_eq!(sent.last(), Some(&extend(2, false)));
}

#[gpui_kit::test]
fn a_lasso_from_the_gap_right_of_a_name_spans_from_that_row(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let start = gap_of(cx, window, 1);
    let row_three = centre(cx, window, 3);
    lasso(cx, window, Modifiers::default(), &[start, row_three]);
    release(cx, window, row_three);
    let expected = [cursor(1), cursor(1), extend(3, false)];
    assert_eq!(commands(&harness, cx), expected);
}

#[gpui_kit::test]
fn ctrl_lasso_unmarks_and_its_release_does_not_toggle(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let start = gap_of(cx, window, 1);
    let (row_three, row_one) = (centre(cx, window, 3), centre(cx, window, 1));
    lasso(cx, window, ctrl(), &[start, row_three, row_one]);
    dispatch(cx, window, up(MouseButton::Left, row_one, ctrl()));
    let expected = [cursor(1), extend(3, true), extend(1, true)];
    assert_eq!(commands(&harness, cx), expected);
}

#[gpui_kit::test]
fn a_drag_that_starts_on_a_name_does_not_lasso(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let start = icon_of(cx, window, 1);
    let row_three = centre(cx, window, 3);
    lasso(cx, window, Modifiers::default(), &[start, row_three]);
    assert!(!is_drawn(cx, window));
    release(cx, window, row_three);
    assert_eq!(commands(&harness, cx), [cursor(1)]);
}
