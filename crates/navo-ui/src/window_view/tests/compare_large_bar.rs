//! The large body's scrollbar spans the whole file (V38 review): a drag reaches rows far past
//! the 262,144-row list window, and the wheel crossing a window margin keeps the top row.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, Modifiers, MouseButton, Pixels, PlatformInput, Point, ScrollDelta,
    TestAppContext, point, px,
};

use super::compare_large::{lines, open_large, push};
use super::compare_screen::Rig;
use super::screen_access::{screen_read, screen_update};
use super::settle;
use crate::test_pointer::{dispatch, down, moved, up};

const TOTAL: u64 = 1_000_000;

/// The open large body's top row and the first row of its list window.
fn place(cx: &mut TestAppContext, rig: &Rig) -> (u64, u64) {
    let probe = screen_read(&rig.view, cx, |view, _| view.large_probe());
    let probe = probe.expect("a large body");
    (probe.top, probe.base)
}

/// The thumb's grab point near the track's top and the pointer that puts the thumb at `part`
/// of its travel (the thumb is 48 px long, grabbed 20 px down).
fn thumb(cx: &mut TestAppContext, rig: &Rig, part: f32) -> (Point<Pixels>, Point<Pixels>) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.find("compare-large-rows").bounds();
        let from = point(list.right() - px(6.), list.top() + px(20.));
        let to = point(from.x, from.y + (list.size.height - px(48.)) * part);
        (from, to)
    })
    .expect("window")
}

/// Sends a pointer event and lets the window settle.
fn send(cx: &mut TestAppContext, rig: &Rig, input: PlatformInput) {
    dispatch(cx, rig.window, input);
    settle(cx, &rig.view);
}

/// Holds the thumb, drags it to `part` of its travel and returns the held pointer position.
fn grab_to(cx: &mut TestAppContext, rig: &Rig, part: f32) -> Point<Pixels> {
    let (from, to) = thumb(cx, rig, part);
    let none = Modifiers::default();
    send(cx, rig, down(MouseButton::Left, from, none));
    for step in 1..=8 {
        let at = point(from.x, from.y + (to.y - from.y) * (step as f32 / 8.));
        send(cx, rig, moved(Some(MouseButton::Left), at, none));
    }
    to
}

/// Drags over `total` rows to 90 % of the thumb's travel and checks the top row mid-drag and
/// after mouse-up.
fn drag_case(cx: &mut TestAppContext, total: u64) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..10, (total, total)));
    let to = grab_to(cx, &rig, 0.9);
    // The thumb travels the content less one viewport of rows, so 90 % lands there.
    let viewport = cx
        .update_window(rig.window, |_, window, _| {
            window.find("compare-large-rows").bounds().size.height
        })
        .expect("window");
    let row_height = rig
        .view
        .update(cx, |_, cx| crate::tokens::current(cx).spacing.row_height);
    let rows_in_view = f64::from(f32::from(viewport) / f32::from(row_height));
    let wanted = ((total as f64 - rows_in_view) * 0.9).round() as u64;
    let (top, base) = place(cx, &rig);
    assert!(
        top.abs_diff(wanted) <= 4,
        "mid-drag: row {wanted} wanted, top {top}, base {base}"
    );
    send(cx, &rig, up(MouseButton::Left, to, Modifiers::default()));
    assert!(
        place(cx, &rig).0.abs_diff(top) <= 4,
        "the top row stays after mouse-up: {top} then {}",
        place(cx, &rig).0
    );
}

#[gpui_kit::test]
fn compare_large_scrollbar_drag_reaches_the_whole_file(cx: &mut TestAppContext) {
    drag_case(cx, TOTAL);
}

#[gpui_kit::test]
fn compare_large_scrollbar_drag_reaches_the_whole_ten_million_row_file(cx: &mut TestAppContext) {
    drag_case(cx, 10_000_000);
}

#[gpui_kit::test]
fn compare_large_hunks_arriving_in_a_drag_do_not_move_the_top_at_release(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..10, (5_000_000, 5_000_000)));
    let to = grab_to(cx, &rig, 0.5);
    let held = place(cx, &rig).0;
    push(cx, &rig, &sink, lines(0..10, (10_000_000, 10_000_000)));
    assert!(place(cx, &rig).0.abs_diff(held) <= 4, "the list stays put");
    send(cx, &rig, up(MouseButton::Left, to, Modifiers::default()));
    let top = place(cx, &rig).0;
    assert!(
        top.abs_diff(held) <= 4,
        "release keeps row {held}, top {top}"
    );
}

#[gpui_kit::test]
fn compare_large_wheel_across_a_window_margin_keeps_the_top_row(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..10, (TOTAL, TOTAL)));
    screen_update(&rig.view, cx, |view, cx| {
        view.large_scroll_to(150_000);
        cx.notify();
    });
    settle(cx, &rig.view);
    let row_height = rig.view.update(cx, |_, cx| {
        f32::from(crate::tokens::current(cx).spacing.row_height)
    });
    let before = place(cx, &rig);
    assert_eq!(before, (150_000, 0));
    let rows = 60_000_u64;
    cx.update_window(rig.window, |_, window, cx| {
        let delta = ScrollDelta::Pixels(point(px(0.), px(-(rows as f32 * row_height))));
        window.scroll("compare-large-rows", delta, cx);
    })
    .expect("window");
    settle(cx, &rig.view);
    let (top, base) = place(cx, &rig);
    assert!(base > 0, "the window moved past its margin: base {base}");
    assert_eq!(top, before.0 + rows, "the same absolute top row");
}
