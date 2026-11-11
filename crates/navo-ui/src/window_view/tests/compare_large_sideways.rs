//! Sideways scroll of the large text body (W8j): the extent grows as wider line windows arrive
//! and never shrinks, the wheel moves only the shared offset, Left/Right step 8 columns, short
//! lines show no strip, and a reopened pair starts at offset 0.

use std::ops::Range;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, Modifiers, ScrollDelta, ScrollEvent, TestAppContext, point,
    px,
};

use super::compare_large::{lines, open_large, open_large_in, push};
use super::compare_screen::Rig;
use crate::compare_diff::{DiffSink, DiffUpdate};
use crate::window_view::compare_screen::SidewaysSeen;

/// A line of `cols` columns of `a`..`z`, as in the file diff's wide pair.
fn wide(cols: u32) -> String {
    (0..cols)
        .map(|c| char::from(b'a' + (c % 26) as u8))
        .collect()
}

/// Lines `rows` of both sides, every one `cols` columns wide, with the sides' line counts.
fn wide_lines(rows: Range<u64>, totals: (u64, u64), cols: u32) -> DiffUpdate {
    DiffUpdate::LargeLines {
        request: 1,
        rows: rows.clone(),
        left: rows.clone().map(|_| wide(cols)).collect(),
        right: rows.map(|_| wide(cols)).collect(),
        left_cut: false,
        right_cut: false,
        left_starts: Vec::new(),
        right_starts: Vec::new(),
        totals,
    }
}

/// Draws two frames: the first lets the text cells' probe land, the second cuts with it.
fn frames(cx: &mut TestAppContext, rig: &Rig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("window");
}

/// Turns the wheel over the middle of the large rows with `modifiers` held, then redraws.
fn wheel(cx: &mut TestAppContext, rig: &Rig, delta: ScrollDelta) {
    let modifiers = Modifiers::default();
    cx.update_window(rig.window, |_, window, cx| {
        let centre = window.find("compare-large-rows").bounds().center();
        window.dispatch_event(crate::test_pointer::moved(None, centre, modifiers), cx);
        window.render_frame(cx);
        let event = ScrollEvent {
            position: centre,
            delta,
            modifiers,
            ..Default::default()
        };
        window.dispatch_event(event.to_platform_input(), cx);
        window.render_frame(cx);
    })
    .expect("window");
}

fn sideways_px(cx: &mut TestAppContext, rig: &Rig, x: f32, y: f32) {
    wheel(cx, rig, ScrollDelta::Pixels(point(px(x), px(y))));
}

fn seen(cx: &mut TestAppContext, rig: &Rig) -> SidewaysSeen {
    rig.view
        .read_with(cx, |view, _| view.large_sideways())
        .expect("an opened large pair")
}

/// Whether the sideways strip's left (or only) bar is in the last frame.
fn strip_shown(cx: &mut TestAppContext, rig: &Rig) -> bool {
    cx.update_window(rig.window, |_, window, _| {
        window.try_find("compare-sideways-left").is_some()
    })
    .expect("window")
}

/// Opens a large pair and answers its first window with 500-column lines, drawn until the
/// cut is ready.
fn opened(cx: &mut TestAppContext) -> (Rig, DiffSink) {
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, wide_lines(0..40, (5_000, 5_000), 500));
    frames(cx, &rig);
    (rig, sink)
}

/// The cut that offset `offset` gives at column width `advance`: its first column and nudge.
fn expected_from(offset: f32, advance: f32) -> (u32, f32) {
    let from = (offset / advance).floor();
    (from as u32, offset - from * advance)
}

#[gpui_kit::test]
fn the_extent_grows_with_arriving_windows_and_never_shrinks(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, wide_lines(0..40, (5_000, 5_000), 500));
    frames(cx, &rig);
    let first = seen(cx, &rig);
    assert!(
        first.max.is_some_and(|max| max > 0.),
        "the 500-column window is measured: {first:?}"
    );
    push(cx, &rig, &sink, lines(40..80, (5_000, 5_000)));
    frames(cx, &rig);
    let narrow = seen(cx, &rig);
    assert_eq!(narrow.max, first.max, "a narrower window never shrinks it");
    push(cx, &rig, &sink, wide_lines(80..100, (5_000, 5_000), 700));
    frames(cx, &rig);
    let grown = seen(cx, &rig);
    assert!(
        grown.max > first.max,
        "a wider window grows the extent: {grown:?}"
    );
}

#[gpui_kit::test]
fn a_sideways_wheel_moves_only_the_large_offset(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, _sink) = opened(cx);
    let before = rig
        .view
        .read_with(cx, |view, _| view.large_probe())
        .expect("a large body");
    sideways_px(cx, &rig, -80., 0.);
    let now = seen(cx, &rig);
    assert_eq!(now.offset, 80., "{now:?}");
    let (from, nudge) = expected_from(80., now.advance);
    assert_eq!(now.cols.as_ref().map(|cols| cols.start), Some(from));
    assert!((now.nudge - nudge).abs() < 0.01, "{now:?}");
    let after = rig
        .view
        .read_with(cx, |view, _| view.large_probe())
        .expect("a large body");
    assert_eq!(
        (after.top, after.base),
        (before.top, before.base),
        "the vertical offset and the bar's row window stay"
    );
}

#[gpui_kit::test]
fn right_moves_the_large_body_eight_columns_and_left_stops_at_zero(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, _sink) = opened(cx);
    rig.press(cx, "left");
    assert_eq!(seen(cx, &rig).offset, 0., "left at 0 stays at 0");
    rig.press(cx, "right");
    let now = seen(cx, &rig);
    assert!(now.offset > 0., "the large body moves right: {now:?}");
    assert!((now.offset - 8. * now.advance).abs() < 0.01, "{now:?}");
    rig.press(cx, "left");
    assert_eq!(seen(cx, &rig).offset, 0.);
}

#[gpui_kit::test]
fn short_lines_leave_the_large_body_without_a_strip_or_offset(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (wide_rig, _sink) = opened(cx);
    assert!(
        strip_shown(cx, &wide_rig),
        "500-column lines show the strip"
    );
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..40, (5_000, 5_000)));
    frames(cx, &rig);
    assert!(!strip_shown(cx, &rig), "short lines have no strip");
    sideways_px(cx, &rig, -80., 0.);
    assert_eq!(seen(cx, &rig).offset, 0., "and nothing to scroll");
}

#[gpui_kit::test]
fn a_reopened_large_pair_starts_at_zero(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, _sink) = opened(cx);
    sideways_px(cx, &rig, -80., 0.);
    assert_eq!(seen(cx, &rig).offset, 80., "the wheel moved it first");
    rig.press(cx, "escape");
    open_large_in(cx, &rig);
    let now = seen(cx, &rig);
    assert_eq!(now.offset, 0., "a new pair starts at 0");
    assert!(now.max.is_none(), "and knows no extent yet: {now:?}");
}
