//! Sideways scroll of the file diff (D39.2–D39.4, PC3, PC4): the wheel moves one shared offset,
//! a vertical swipe does not, layouts and reloads keep it, and wrap ignores sideways input.

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
