//! Tests for the settings navo pushes into the kit theme.

use gpui_kit::TestAppContext;
use gpui_kit::component::theme::Theme as KitTheme;

use crate::tokens::{Mode, apply};

#[gpui_kit::test]
fn the_text_caret_stops_blinking_when_idle(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        for mode in [Mode::Light, Mode::Dark] {
            apply(mode, cx);
            let blink = KitTheme::global(cx).cursor_blink;
            assert!(
                blink.is_steady() || blink.idle_timeout().is_some(),
                "an idle focused field must not wake the app forever: {blink:?}"
            );
        }
    });
}
