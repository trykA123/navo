//! The paged header at the minimum window width: the View button, and so the mouse route to
//! Language, stays on screen beside a paused Follow badge and an open Find or Go to bar.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Bounds, Pixels, TestAppContext, px};

use super::language_picker::open_huge_sized;
use super::structured::fake_with;
use super::{Log, Setup, hooks, options_with, redraw, text_of};
use crate::window_view::tests::{open_options_sized, press, type_in};
use crate::window_view::viewer::Kinds;

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

/// Opens a small `app.log` in the editor body, 480 px wide, with Find open and `needle` typed.
fn editor_with_find(cx: &mut TestAppContext, needle: &str) -> AnyWindowHandle {
    let text = "alpha error one\nerror two\nerror three\n".repeat(50);
    let files = hooks(Setup::new(text.into_bytes()), &Log::default());
    let options = options_with(fake_with("app.log", 1950), files, Kinds::builtin());
    let (window, view) = open_options_sized(cx, options, px(NARROW));
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, needle);
    redraw(cx, window);
    window
}

/// Asserts View, the Find field and Match case sit inside the window and in that order.
fn assert_editor_header_fits(cx: &mut TestAppContext, window: AnyWindowHandle) {
    let field = bounds(cx, window, "editor-input");
    let case = bounds(cx, window, "match-case");
    let view = bounds(cx, window, "viewer-view");
    assert!(field.left() >= px(0.), "{field:?}");
    assert!(field.right() <= case.left(), "{field:?} overlaps {case:?}");
    assert!(case.right() <= view.left(), "{case:?} overlaps {view:?}");
    assert!(view.right() <= px(NARROW), "View is pushed off: {view:?}");
}

#[gpui_kit::test]
fn editor_header_fits_with_find_open_at_480_px(cx: &mut TestAppContext) {
    let window = editor_with_find(cx, "");
    assert_editor_header_fits(cx, window);
}

#[gpui_kit::test]
fn editor_header_fits_with_find_and_a_match_count_at_480_px(cx: &mut TestAppContext) {
    let window = editor_with_find(cx, "error");
    text_of(cx, window, "find-count");
    assert_editor_header_fits(cx, window);
}
