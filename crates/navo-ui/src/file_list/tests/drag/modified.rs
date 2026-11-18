//! Tests for the 6 px drag threshold on Shift or Ctrl presses in the file list.
//! Each gesture runs with and without a redraw after the press, since GPUI redraws at its own pace.

use gpui_kit::{Modifiers, MouseButton, TestAppContext};
use navo_core::{Command, CursorMove, SpanMode};

use super::*;

fn extend(row: u64) -> Command {
    Command::SelectionExtend {
        to: CursorMove::Row(row),
        mode: SpanMode::Mark,
    }
}

/// Presses at `from`, moves once by `dx` px and releases there; true when a drag was active.
/// No frame is drawn between the events unless `redraw` asks for one right after the press.
fn gesture(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    (from, dx): (Point<Pixels>, f32),
    (modifiers, redraw): (Modifiers, bool),
) -> bool {
    let to = point(from.x + px(dx), from.y);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_event(down(MouseButton::Left, from, modifiers), cx);
        if redraw {
            window.render_frame(cx);
        }
        window.dispatch_event(moved(Some(MouseButton::Left), to, modifiers), cx);
        let is_dragging = cx.has_active_drag();
        window.dispatch_event(up(MouseButton::Left, to, modifiers), cx);
        is_dragging
    })
    .expect("window")
}

fn a_small_shift_move_extends(cx: &mut TestAppContext, redraw: bool) {
    let (window, host) = open_host(cx, true);
    let at = name_of(cx, window, LAYOUT);
    let dragged = gesture(cx, window, (at, 3.), (shift(), redraw));
    cx.run_until_parked();
    assert!(!dragged);
    assert_eq!(read(&host, cx, |h| h.commands.clone()), [extend(3)]);
    assert!(read(&host, cx, |h| h.starts.is_empty() && h.drops.is_empty()));
}

fn a_far_shift_move_drags(cx: &mut TestAppContext, redraw: bool) {
    let (window, host) = open_host(cx, true);
    let at = name_of(cx, window, CARGO);
    let dragged = gesture(cx, window, (at, 7.), (shift(), redraw));
    cx.run_until_parked();
    assert!(dragged);
    assert_eq!(read(&host, cx, |h| h.starts.len()), 1);
    assert!(read(&host, cx, |h| h.commands.is_empty()));
}

#[gpui_kit::test]
fn a_shift_press_that_moves_3_px_before_a_redraw_still_extends_the_marks(cx: &mut TestAppContext) {
    a_small_shift_move_extends(cx, false);
}

#[gpui_kit::test]
fn a_shift_press_that_moves_3_px_after_a_redraw_still_extends_the_marks(cx: &mut TestAppContext) {
    a_small_shift_move_extends(cx, true);
}

#[gpui_kit::test]
fn one_shift_move_of_7_px_before_a_redraw_starts_a_drag(cx: &mut TestAppContext) {
    a_far_shift_move_drags(cx, false);
}

#[gpui_kit::test]
fn one_shift_move_of_7_px_after_a_redraw_starts_a_drag(cx: &mut TestAppContext) {
    a_far_shift_move_drags(cx, true);
}

#[gpui_kit::test]
fn a_ctrl_press_that_moves_4_px_still_toggles(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let at = name_of(cx, window, 1);
    assert!(!gesture(cx, window, (at, 4.), (ctrl(), false)));
    cx.run_until_parked();
    let commands = read(&host, cx, |h| h.commands.clone());
    assert_eq!(commands, [cursor(1), Command::SelectionToggle]);
}

#[gpui_kit::test]
fn a_plain_press_keeps_the_small_drag_threshold(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let at = name_of(cx, window, CARGO);
    assert!(gesture(cx, window, (at, 3.), (Modifiers::default(), false)));
    cx.run_until_parked();
    assert_eq!(read(&host, cx, |h| h.starts.len()), 1);
}

#[gpui_kit::test]
fn a_shift_press_released_without_shift_still_extends_the_marks(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let at = name_of(cx, window, LAYOUT);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_event(down(MouseButton::Left, at, shift()), cx);
        window.dispatch_event(up(MouseButton::Left, at, Modifiers::default()), cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(read(&host, cx, |h| h.commands.clone()), [extend(3)]);
}

#[gpui_kit::test]
fn a_right_release_does_not_finish_a_pending_shift_click(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, true);
    let at = name_of(cx, window, LAYOUT);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_event(down(MouseButton::Left, at, shift()), cx);
        window.dispatch_event(up(MouseButton::Right, at, shift()), cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert!(read(&host, cx, |h| h.commands.is_empty()));
}
