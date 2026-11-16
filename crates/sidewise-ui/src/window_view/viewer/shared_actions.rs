//! Viewer actions shared with other screens (row movement, list and view-mode keys).

gpui_kit::actions!(
    viewer,
    [
        RowUp,
        RowDown,
        PageUp,
        PageDown,
        GoStart,
        GoEnd,
        ScrollLeft,
        ScrollRight,
        ListUp,
        ListDown,
        ListGo,
        ViewSimple,
        ViewRich,
        ViewCustom,
    ]
);
