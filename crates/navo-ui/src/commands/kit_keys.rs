// Key bindings for kit widgets (dialog focus, confirm dialog, context menu). Navo binds them
// here through its keymap; the kit modules only declare their actions (NC110).
use gpui_kit::App;

/// Binds the dialog focus keys (Tab, arrows, Space and Enter inside dialogs).
pub fn bind_dialog_focus(cx: &mut App) {
    super::bind_namespace(cx, "dialog_focus");
}

/// Binds the confirm dialog keys, plus the shared dialog focus keys.
pub fn bind_confirm_dialog(cx: &mut App) {
    bind_dialog_focus(cx);
    super::bind_namespace(cx, "confirm_dialog");
}

/// Binds the context menu keys.
pub fn bind_context_menu(cx: &mut App) {
    super::bind_namespace(cx, "context_menu");
}
