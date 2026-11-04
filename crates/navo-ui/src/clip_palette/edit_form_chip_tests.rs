use gpui_kit::TestAppContext;

use super::ClipMode;
use super::edit_form_tests::{opened, pick};
use crate::commands::{Keymap, set_active};

#[gpui_kit::test]
fn the_form_chips_show_the_keys_of_the_keymap(cx: &mut TestAppContext) {
    let text = "[[bind]]\ncommand = \"clip_palette.save_form\"\nkeys = [\"ctrl-s\"]\n\n\
                [[bind]]\ncommand = \"clip_palette.toggle_reveal\"\nkeys = [\"alt-p\"]\n";
    let keymap = Keymap::builtin().overlay(text).expect("keymap").keymap;
    cx.update(|cx| set_active(cx, keymap));
    let (palette, _) = opened(cx, Vec::new());
    pick(&palette, ClipMode::Secrets, cx);
    palette.press("ctrl-n", cx);
    assert_eq!(
        palette.label_of(("clip-form-save", 1usize), cx).as_deref(),
        Some("Ctrl+S")
    );
    assert_eq!(
        palette.label_of("clip-form-reveal-key", cx).as_deref(),
        Some("Alt+P")
    );
}
