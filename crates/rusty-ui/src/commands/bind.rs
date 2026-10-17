use gpui_kit::{App, KeyBinding};

use super::Keymap;

pub(crate) fn key_bindings(keymap: &Keymap, cx: &App, namespace: Option<&str>) -> Vec<KeyBinding> {
    let _ = (keymap, cx, namespace);
    todo!()
}
