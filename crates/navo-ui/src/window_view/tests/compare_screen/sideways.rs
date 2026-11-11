//! Sideways scroll of the file diff (D39.2–D39.4, PC3, PC4): the wheel (alone, and with Shift
//! as gpui delivers it) moves one shared offset, a vertical swipe does not, layouts and reloads
//! keep it, and wrap ignores sideways input.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, Modifiers, ScrollDelta, ScrollEvent, TestAppContext, point,
    px,
};

use super::body::pair;
use super::{Rig, rig};
use crate::compare_diff::{DiffUpdate, Layout, PairKind, PairText};
use crate::window_view::compare_screen::SidewaysSeen;

/// A 40-row pair whose every line is 500 columns of `a`..`z`.
fn wide_pair() -> PairText {
    let line: String = (0..500u32)
        .map(|c| char::from(b'a' + (c % 26) as u8))
        .collect();
    let mut text = pair(40, &[3]);
    text.left = vec![line.clone(); 40];
    text.right = vec![line; 40];
    text
}

/// Draws two frames: the first lets the text cells' probe land, the second cuts with it.
fn frames(rig: &Rig, cx: &mut TestAppContext) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("window");
}

/// Opens the wide pair and draws until the cut is ready.
fn opened(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx);
    rig.open_text(cx, wide_pair());
    frames(&rig, cx);
    rig
}

/// Turns the wheel over the middle of the file rows with `modifiers` held, then redraws.
fn wheel(rig: &Rig, cx: &mut TestAppContext, delta: ScrollDelta, modifiers: Modifiers) {
    cx.update_window(rig.window, |_, window, cx| {
        let centre = window.find("compare-file-rows").bounds().center();
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

fn sideways_px(rig: &Rig, cx: &mut TestAppContext, x: f32, y: f32) {
    let delta = ScrollDelta::Pixels(point(px(x), px(y)));
    wheel(rig, cx, delta, Modifiers::default());
}

fn seen(rig: &Rig, cx: &mut TestAppContext) -> SidewaysSeen {
    rig.view
        .update(cx, |view, _| view.file_sideways())
        .expect("an opened text pair")
}

/// The cut that offset `offset` gives at column width `advance`: its first column and nudge.
fn expected_from(offset: f32, advance: f32) -> (u32, f32) {
    let from = (offset / advance).floor();
    (from as u32, offset - from * advance)
}

#[gpui_kit::test]
fn a_sideways_wheel_moves_the_offset_and_the_cut_follows(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    sideways_px(&rig, cx, -80., 0.);
    let now = seen(&rig, cx);
    assert_eq!(now.offset, 80., "{now:?}");
    let (from, nudge) = expected_from(80., now.advance);
    assert_eq!(now.cols.as_ref().map(|cols| cols.start), Some(from));
    assert!((now.nudge - nudge).abs() < 0.01, "{now:?}");
    assert_eq!(
        rig.probe(cx, Layout::SideBySide).1.first(),
        Some(&0),
        "no vertical move"
    );
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    wheel(&rig, cx, ScrollDelta::Lines(point(-3., 0.)), shift);
    let now = seen(&rig, cx);
    assert!(
        (now.offset - (80. + 3. * now.advance)).abs() < 0.01,
        "{now:?}"
    );
    sideways_px(&rig, cx, -100_000., 0.);
    let now = seen(&rig, cx);
    assert_eq!(Some(now.offset), now.max, "clamps at the widest line");
}

#[gpui_kit::test]
fn a_vertical_swipe_with_jitter_keeps_the_offset(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    sideways_px(&rig, cx, -3., -40.);
    assert_eq!(seen(&rig, cx).offset, 0.);
}

#[gpui_kit::test]
fn both_sides_and_both_layouts_share_one_offset(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    sideways_px(&rig, cx, -80., 0.);
    let side_by_side = seen(&rig, cx);
    rig.view.update(cx, |view, _| {
        view.file_look_mut().expect("open").layout = Some(Layout::Unified);
    });
    frames(&rig, cx);
    let unified = seen(&rig, cx);
    assert_eq!(unified.offset, 80.);
    assert_eq!(
        unified.cols.map(|cols| cols.start),
        side_by_side.cols.map(|cols| cols.start)
    );
}

#[gpui_kit::test]
fn wrap_ignores_sideways_input_and_keeps_the_offset(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    sideways_px(&rig, cx, -80., 0.);
    let set_wrap = |cx: &mut TestAppContext, wrap: bool| {
        rig.view.update(cx, |view, _| {
            view.file_look_mut().expect("open").wrap = wrap;
        });
        frames(&rig, cx);
    };
    set_wrap(cx, true);
    sideways_px(&rig, cx, -80., 0.);
    let wrapped = seen(&rig, cx);
    assert_eq!((wrapped.offset, wrapped.cols), (80., None));
    set_wrap(cx, false);
    let now = seen(&rig, cx);
    assert_eq!(
        now.cols.map(|cols| cols.start),
        Some(expected_from(80., now.advance).0)
    );
}

#[gpui_kit::test]
fn a_reload_keeps_the_offset(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    sideways_px(&rig, cx, -80., 0.);
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    let sink = rig.diff.pair_sink();
    let reopened = DiffUpdate::Opened {
        request: 2,
        kind: PairKind::Text,
        text: Some(Arc::new(wide_pair())),
    };
    rig.push(cx, &sink, reopened);
    frames(&rig, cx);
    assert_eq!(seen(&rig, cx).offset, 80.);
}

/// Whether the sideways strip's left (or only) bar is in the last frame.
fn strip_shown(rig: &Rig, cx: &mut TestAppContext) -> bool {
    cx.update_window(rig.window, |_, window, _| {
        window.try_find("compare-sideways-left").is_some()
    })
    .expect("window")
}

#[gpui_kit::test]
fn right_moves_eight_columns_and_left_stops_at_zero(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    rig.press(cx, "left");
    assert_eq!(seen(&rig, cx).offset, 0., "left at 0 stays at 0");
    rig.press(cx, "right");
    let now = seen(&rig, cx);
    assert!((now.offset - 8. * now.advance).abs() < 0.01, "{now:?}");
    rig.press(cx, "left");
    assert_eq!(seen(&rig, cx).offset, 0.);
}

#[gpui_kit::test]
fn the_strip_shows_only_for_wide_lines_without_wrap(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let short = rig(cx);
    short.open_text(cx, pair(40, &[3]));
    frames(&short, cx);
    assert!(!strip_shown(&short, cx), "short lines have no strip");
    let rig = opened(cx);
    assert!(strip_shown(&rig, cx), "500-column lines show the strip");
    rig.view.update(cx, |view, _| {
        view.file_look_mut().expect("open").wrap = true;
    });
    frames(&rig, cx);
    assert!(!strip_shown(&rig, cx), "wrap hides the strip");
}

#[gpui_kit::test]
fn widths_for_a_replaced_text_are_dropped(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(40, &[3]));
    frames(&rig, cx);
    let stale = Arc::new(wide_pair());
    let cols = crate::compare_diff::LineCols::measure(&stale);
    rig.view
        .update(cx, |view, cx| view.install_file_widths(&stale, cols, cx));
    frames(&rig, cx);
    assert_eq!(
        seen(&rig, cx).max,
        Some(0.),
        "the open text keeps its own widths"
    );
}

#[gpui_kit::test]
fn widths_are_measured_off_the_ui_thread(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    (rig.diff.pair_sink())(DiffUpdate::Opened {
        request: rig.diff.requests()[0].id,
        kind: PairKind::Text,
        text: Some(Arc::new(wide_pair())),
    });
    rig.view.update(cx, |view, cx| view.poll_compare_screen(cx));
    frames(&rig, cx);
    assert_eq!(seen(&rig, cx).max, None, "no extent before the task ran");
    cx.run_until_parked();
    frames(&rig, cx);
    assert!(seen(&rig, cx).max.is_some_and(|max| max > 0.));
}

#[gpui_kit::test]
fn a_shifted_wheel_moves_the_file_diff_sideways(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    // A vertical wheel with Shift: gpui turns it into this horizontal `Lines` delta on
    // Windows, X11 and Wayland before the body handler sees it.
    wheel(&rig, cx, ScrollDelta::Lines(point(-3., 0.)), shift);
    let now = seen(&rig, cx);
    assert!(
        (now.offset - 3. * now.advance).abs() < 0.01,
        "Shift plus wheel moves the body: {now:?}"
    );
}
