use gpui_kit::TestAppContext;

use super::{frame, open};
use crate::side_sheet::Side;

#[gpui_kit::test]
fn the_close_button_shows_its_key_and_fires_its_callback(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    assert_eq!(rig.label("side-sheet-close", cx).as_deref(), Some("Close"));
    assert_eq!(
        rig.label("side-sheet-close-key", cx).as_deref(),
        Some("Ctrl+Q")
    );
    rig.click("side-sheet-close", cx);
    assert_eq!(rig.closes.get(), 1);
}

#[gpui_kit::test]
fn the_footer_shows_each_hint_with_its_keys_and_label(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    assert_eq!(
        rig.label(("side-sheet-hint", 0usize), cx).as_deref(),
        Some("[ ]")
    );
    assert_eq!(
        rig.label(("side-sheet-hint", 1usize), cx).as_deref(),
        Some("Browse")
    );
}

#[gpui_kit::test]
fn the_accent_ring_is_an_option(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    assert_eq!(rig.is_selected(cx), Some(false));
    rig.set_ringed(true, cx);
    assert_eq!(rig.is_selected(cx), Some(true));
}

#[gpui_kit::test]
fn the_sheet_carries_its_label(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    assert_eq!(rig.sheet_label(cx).as_deref(), Some("Preview"));
}
