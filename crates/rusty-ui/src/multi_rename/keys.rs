gpui_kit::actions!(
    multi_rename,
    [
        Rename,
        Close,
        TokenMenu,
        FocusPreview,
        FocusFields,
        PageUp,
        PageDown,
        NextProblem,
        PreviousProblem,
        SavePreset,
        OpenPresets,
        Reset,
        ToggleRow,
        EditNames,
        LoadNames,
        PasteNames,
        CopyNames,
        CounterPerFolder,
        MoveRowUp,
        MoveRowDown,
    ]
);

pub fn bind_keys(cx: &mut gpui_kit::App) {
    crate::commands::bind_namespace(cx, "multi_rename");
}
