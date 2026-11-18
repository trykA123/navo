//! Follow across a reload: a language change keeps a paused Follow in place, and an encoding that
//! forces the Hex view leaves no Follow running there. Both run on the encoding test rig.

use gpui_kit::TestAppContext;

use super::encoding::{FRENCH, Rig, latin1, open};
use super::menu::run_item;
use super::search::with_paged;
use super::{redraw, text_of, top, visible};
use crate::window_view::tests::settle;
use crate::window_view::viewer::{EDIT_LIMIT, PagedKind};

/// A log of many lines, past the edit limit, so it opens as paged text.
fn open_followed(cx: &mut TestAppContext) -> Rig {
    let rig = open(cx, latin1(&FRENCH.repeat(300)), Some(EDIT_LIMIT + 1));
    rig.press(cx, "alt-f");
    assert_eq!(text_of(cx, rig.window, "paged-follow"), "Following");
    rig
}

/// Picks `text` in the language picker opened with Ctrl+Shift+L.
fn choose_language(cx: &mut TestAppContext, rig: &Rig, text: &str) {
    rig.press(cx, "ctrl-shift-l");
    run_item(cx, rig.window, text);
    settle(cx, &rig.view);
    redraw(cx, rig.window);
}

#[gpui_kit::test]
fn a_language_chosen_while_follow_is_paused_keeps_it_paused_at_the_same_row(
    cx: &mut TestAppContext,
) {
    let rig = open_followed(cx);
    rig.press(cx, "pageup");
    let paused = top(cx, &rig.view);
    assert!(text_of(cx, rig.window, "paged-follow").starts_with("Paused"));
    assert!(paused > 0, "the view left the start");
    choose_language(cx, &rig, "Plain text");
    assert!(
        text_of(cx, rig.window, "paged-follow").starts_with("Paused"),
        "still paused"
    );
    assert_eq!(top(cx, &rig.view), paused, "the same row");
}

#[gpui_kit::test]
fn an_encoding_that_forces_hex_leaves_no_follow_running(cx: &mut TestAppContext) {
    let rig = open_followed(cx);
    rig.choose(cx, "Shift_JIS");
    let (kind, is_following) =
        with_paged(cx, &rig.view, |paged, _| (paged.kind, paged.is_following()));
    assert_eq!(kind, PagedKind::Hex, "Shift_JIS needs the whole file");
    assert!(!is_following, "Follow works in the text view only");
    assert!(!visible(cx, rig.window, "paged-follow"), "no Follow badge");
}
