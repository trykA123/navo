use super::fields::{Control, F, FieldSpec, S, spec};

pub(super) const WINDOW_FIELDS: [FieldSpec; 15] = [
    spec(
        (
            F::WindowTabsWithOne,
            S::Display,
            "window.view.tabs-with-one",
        ),
        (
            "Tabs row with one tab",
            "Shows the tabs row of a pane even when it holds a single tab.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::WindowPathBar, S::Display, "window.view.path-bar"),
        (
            "Path bar",
            "Shows the folder path above the file list, and Alt+D opens the path field without it.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::WindowDriveButton, S::Display, "window.view.drive-button"),
        (
            "Drive button",
            "Shows the drive button in the pane header, and Alt+F1 and Alt+F2 open the drive list without it.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::WindowTitleSearch, S::Display, "window.view.title-search"),
        ("Search box", "Shows the search box in the title bar."),
        Control::Toggle,
    ),
    spec(
        (F::WindowFkeyBar, S::Display, "window.view.fkey-bar"),
        (
            "F-key bar",
            "Shows the function keys along the bottom, and the keys work without it.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::WindowIcons, S::Display, "window.view.icons"),
        (
            "File icons",
            "Shows a small icon before each name in the file list.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::WindowExtColumn, S::Display, "window.view.ext-column"),
        (
            "Extension column",
            "Shows the Ext column in the file list, and Ctrl+F4 sorts by extension without it.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::WindowSizeColumn, S::Display, "window.view.size-column"),
        (
            "Size column",
            "Shows the Size column in the file list, and Ctrl+F6 sorts by size without it.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::WindowDateColumn, S::Display, "window.view.date-column"),
        (
            "Date column",
            "Shows the Date column in the file list, and Ctrl+F5 sorts by date without it.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::WindowFreeSpace, S::Display, "window.view.free-space"),
        (
            "Free space",
            "Shows the free space of the current drive in the pane footer.",
        ),
        Control::Toggle,
    ),
    spec(
        (
            F::WindowMarkedTotals,
            S::Display,
            "window.view.marked-totals",
        ),
        (
            "Marked totals",
            "Shows how many items are marked and their size in the pane footer.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::WindowContents, S::Display, "window.view.contents"),
        (
            "Folder contents",
            "Shows the file and folder count and the total size in the pane footer.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::WindowExtWidth, S::Display, "window.view.ext-width"),
        ("settings-ext-width", "settings-column-width-help"),
        Control::Number,
    ),
    spec(
        (F::WindowSizeWidth, S::Display, "window.view.size-width"),
        ("settings-size-width", "settings-column-width-help"),
        Control::Number,
    ),
    spec(
        (F::WindowDateWidth, S::Display, "window.view.date-width"),
        ("settings-date-width", "settings-column-width-help"),
        Control::Number,
    ),
];
