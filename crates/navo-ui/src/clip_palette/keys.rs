use gpui_kit::App;

pub(super) const CONTEXT: &str = "ClipPalette";

gpui_kit::actions!(
    clip_palette,
    [
        NextMode,
        PreviousMode,
        MoveUp,
        MoveDown,
        Choose,
        Back,
        OpenActions,
        CopyOnly,
        PasteFiles,
        ToggleReveal,
        CopyUser,
        SaveRow,
        DeleteRow,
        OpenManage
    ]
);

/// Binds the palette's keys from the keymap.
pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "clip_palette");
}
