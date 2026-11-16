//! LK1b: the compare screen opens its file diff with the look its host's defaults give.

use super::body::pair;
use super::rig;
use crate::window_view::compare_screen::look::{Look, LookDefaults, Preset};
use crate::window_view::tests::screen_access::{screen_read, screen_update};

/// A Simple preset with words off, set before the open, reaches the opened file diff.
#[gpui_kit::test]
fn simple_defaults_reach_the_opened_pair(cx: &mut gpui_kit::TestAppContext) {
    let rig = rig(cx);
    screen_update(&rig.view, cx, |screen, _| {
        screen.set_look(LookDefaults {
            preset: Preset::Simple,
            words: false,
            ..LookDefaults::default()
        });
    });
    rig.open_text(cx, pair(3, &[1]));
    let (look, _, _) = screen_read(&rig.view, cx, |s, _| s.file_look()).expect("open");
    assert!(!look.words, "words follow the Simple defaults");
}

/// Without `set_look` the opened pair keeps the Rich look.
#[gpui_kit::test]
fn the_default_rig_opens_rich(cx: &mut gpui_kit::TestAppContext) {
    let rig = rig(cx);
    rig.open_text(cx, pair(3, &[1]));
    let (look, _, _) = screen_read(&rig.view, cx, |s, _| s.file_look()).expect("open");
    assert_eq!(look, Look::rich());
}
