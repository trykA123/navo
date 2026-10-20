use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Modifiers, MouseButton, Pixels, Point, TestAppContext, point, px};
use rusty_core::{Command, CursorMove, SpanMode};
use rusty_fs::{EntryKind, FsEntry};

use super::mouse::{LAST_ROW, below, gap_of, icon_of, row_bounds};
use super::{commands, entry, open_list, props, sample_rows};
use crate::test_pointer::{self, ctrl, dispatch, up};

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
    assert_eq!(
        commands(&harness, cx),
        [cursor(last), extend(3, SpanMode::Mark)]
    );
    dispatch(
        cx,
        window,
        test_pointer::moved(Some(MouseButton::Left), row_four, Modifiers::default()),
    );
    release(cx, window, row_four);
    let expected = [
        cursor(last),
        extend(3, SpanMode::Mark),
        extend(4, SpanMode::Mark),
    ];
    assert_eq!(commands(&harness, cx), expected);
    assert!(!is_drawn(cx, window));
}

#[gpui_kit::test]
fn a_lasso_from_below_that_comes_back_below_holds_no_rows(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let start = below(cx, window);
    let row_four = centre(cx, window, 4);
    lasso(cx, window, Modifiers::default(), &[start, row_four, start]);
    let last = LAST_ROW as u64;
    let expected = [
        cursor(last),
        extend(4, SpanMode::Mark),
        extend(last + 1, SpanMode::Mark),
    ];
    assert_eq!(commands(&harness, cx), expected);
}

#[gpui_kit::test]
fn a_lasso_dragged_past_the_bottom_edge_stops_at_the_last_shown_row(cx: &mut TestAppContext) {
    let rows: Vec<FsEntry> = (0..500)
        .map(|i| entry(&format!("f{i:03}.txt"), EntryKind::File, 1))
        .collect();
    let (window, harness) = open_list(cx, props(rows.into()));
    let start = gap_of(cx, window, 2);
    let far_below = point(start.x, start.y + px(5_000.));
    lasso(cx, window, Modifiers::default(), &[start, far_below]);
    let shown = harness.read_with(cx, |harness, _| harness.page_rows.last().copied());
    let shown = shown.expect("page rows");
    let Some(Command::SelectionExtend {
        to: CursorMove::Row(far),
        ..
    }) = commands(&harness, cx).last().cloned()
    else {
        panic!("no extend");
    };
    assert!((2..=shown).contains(&far), "{far} of {shown} shown");
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
    assert_eq!(sent.last(), Some(&extend(2, SpanMode::Mark)));
}

#[gpui_kit::test]
fn a_lasso_from_the_gap_right_of_a_name_spans_from_that_row(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let start = gap_of(cx, window, 1);
    let row_three = centre(cx, window, 3);
    lasso(cx, window, Modifiers::default(), &[start, row_three]);
    release(cx, window, row_three);
    let expected = [cursor(1), cursor(1), extend(3, SpanMode::Mark)];
    assert_eq!(commands(&harness, cx), expected);
}

#[gpui_kit::test]
fn ctrl_lasso_toggles_and_its_release_does_not_toggle_again(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let start = gap_of(cx, window, 1);
    let (row_three, row_one) = (centre(cx, window, 3), centre(cx, window, 1));
    lasso(cx, window, ctrl(), &[start, row_three, row_one]);
    dispatch(cx, window, up(MouseButton::Left, row_one, ctrl()));
    let expected = [
        cursor(1),
        extend(3, SpanMode::Toggle),
        extend(1, SpanMode::Toggle),
    ];
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
