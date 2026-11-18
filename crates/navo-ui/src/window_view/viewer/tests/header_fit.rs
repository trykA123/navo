//! The paged header at the minimum window width: the View button, and so the mouse route to
//! Language, stays on screen beside a paused Follow badge and an open Find or Go to bar.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Bounds, Pixels, TestAppContext, px};

use super::language_picker::open_huge_sized;
use super::{redraw, text_of};
use crate::window_view::tests::{press, type_in};

const NARROW: f32 = 480.;

/// The bounds of the header element `id`.
fn bounds(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) -> Bounds<Pixels> {
    redraw(cx, window);
    cx.update_window(window, |_, window, _| window.find(id).bounds())
        .expect("window")
}

/// Asserts the View button lies inside the window and the open bar ends before it.
fn assert_view_reachable(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    bar: Option<&'static str>,
) {
    let view = bounds(cx, window, "viewer-view");
    assert!(view.left() >= px(0.), "{view:?}");
    assert!(view.right() <= px(NARROW), "View is pushed off: {view:?}");
    if let Some(bar) = bar {
        let bar = bounds(cx, window, bar);
        assert!(bar.right() <= view.left(), "{bar:?} covers {view:?}");
        assert!(bar.left() >= px(0.), "{bar:?}");
    }
}

#[gpui_kit::test]
fn view_stays_on_screen_with_follow_paused_and_find_open_at_480_px(cx: &mut TestAppContext) {
    let (window, view) = open_huge_sized(cx, px(NARROW));
    press(cx, window, &view, "alt-f");
    press(cx, window, &view, "up");
    assert!(text_of(cx, window, "paged-follow").starts_with("Paused"));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "disk");
    assert_view_reachable(cx, window, Some("paged-bar"));
}

#[gpui_kit::test]
fn view_stays_on_screen_with_go_to_open_and_follow_paused_at_480_px(cx: &mut TestAppContext) {
    let (window, view) = open_huge_sized(cx, px(NARROW));
    press(cx, window, &view, "alt-f");
    press(cx, window, &view, "up");
    press(cx, window, &view, "ctrl-g");
    assert_view_reachable(cx, window, Some("paged-bar"));
}

#[gpui_kit::test]
fn view_stays_on_screen_with_follow_paused_and_nothing_open_at_480_px(cx: &mut TestAppContext) {
    let (window, view) = open_huge_sized(cx, px(NARROW));
    press(cx, window, &view, "alt-f");
    press(cx, window, &view, "up");
    assert_view_reachable(cx, window, None);
}
