//! Binds the context menu keys for the kit's own tests; the app binds them from its keymap.

/// Binds the context menu keys in both its contexts, as Navo's keymap does.
pub(super) fn bind_menu_keys(cx: &mut gpui_kit::App) {
    use super::super::keys::{
        MenuClassic, MenuDown, MenuEnd, MenuEnter, MenuEscape, MenuHome, MenuLeft, MenuPageDown,
        MenuPageUp, MenuRight, MenuUp,
    };
    use gpui_kit::KeyBinding;
    for context in ["ContextMenu", "ContextMenu > Input"] {
        cx.bind_keys([
            KeyBinding::new("up", MenuUp, Some(context)),
            KeyBinding::new("down", MenuDown, Some(context)),
            KeyBinding::new("home", MenuHome, Some(context)),
            KeyBinding::new("end", MenuEnd, Some(context)),
            KeyBinding::new("pageup", MenuPageUp, Some(context)),
            KeyBinding::new("pagedown", MenuPageDown, Some(context)),
            KeyBinding::new("enter", MenuEnter, Some(context)),
            KeyBinding::new("right", MenuRight, Some(context)),
            KeyBinding::new("left", MenuLeft, Some(context)),
            KeyBinding::new("backspace", MenuLeft, Some(context)),
            KeyBinding::new("escape", MenuEscape, Some(context)),
            KeyBinding::new("shift-f10", MenuClassic, Some(context)),
        ]);
    }
}
