use super::fields::{
    COLOR_THEMES, COPY_MODES, Control, DENSITIES, DROP_CONFIRMS, F, FieldSpec, HIDDEN, HINTS, Look,
    S, SORTS, THEMES, spec,
};

pub(super) const GENERAL: [FieldSpec; 22] = [
    spec(
        (F::Theme, S::Appearance, "appearance.mode"),
        ("Theme", "Follow Windows, or keep light or dark"),
        Control::Choices(&THEMES, Look::ThemePictures),
    ),
    spec(
        (F::Density, S::Appearance, "appearance.density"),
        ("Density", "Row height in file lists"),
        Control::Choices(&DENSITIES, Look::DensityPictures),
    ),
    spec(
        (F::ColorTheme, S::Appearance, "appearance.color-theme"),
        ("settings-color-theme", "settings-color-theme-help"),
        Control::Choices(&COLOR_THEMES, Look::ColorPictures),
    ),
    spec(
        (F::LightTheme, S::Appearance, "appearance.light-theme"),
        ("Light theme", "Used when the theme is light"),
        Control::Text,
    ),
    spec(
        (F::DarkTheme, S::Appearance, "appearance.dark-theme"),
        ("Dark theme", "Used when the theme is dark"),
        Control::Text,
    ),
    spec(
        (F::Font, S::Appearance, "appearance.font"),
        ("Interface font", "File lists, menus and dialogs"),
        Control::Text,
    ),
    spec(
        (F::FontSize, S::Appearance, "appearance.font-size"),
        ("Font size", "In pixels"),
        Control::Number,
    ),
    spec(
        (F::ModifierHints, S::Appearance, "appearance.modifier-hints"),
        ("Modifier hints", "What holding Ctrl, Shift or Alt shows"),
        Control::Choices(&HINTS, Look::Segments),
    ),
    spec(
        (
            F::ModifierHintsDelay,
            S::Appearance,
            "appearance.modifier-hints-delay-ms",
        ),
        ("Hint delay", "Milliseconds to hold, 150 to 2000"),
        Control::Number,
    ),
    spec(
        (F::HiddenFiles, S::Files, "files.hidden-files"),
        (
            "Hidden files",
            "Files and folders with the Hidden attribute",
        ),
        Control::Choices(&HIDDEN, Look::Segments),
    ),
    spec(
        (F::ShowSystem, S::Files, "files.show-system"),
        ("Show system files", "Files with the System attribute"),
        Control::Toggle,
    ),
    spec(
        (F::FoldersFirst, S::Files, "files.folders-first"),
        ("Folders first", "Keep folders above files in every sort"),
        Control::Toggle,
    ),
    spec(
        (F::Sort, S::Files, "files.sort"),
        ("Sort by", "Column used when a folder opens"),
        Control::Choices(&SORTS, Look::Segments),
    ),
    spec(
        (F::SortDescending, S::Files, "files.sort-descending"),
        ("Descending", "Largest, newest or Z first"),
        Control::Toggle,
    ),
    spec(
        (F::DropConfirm, S::Files, "files.drop-confirm"),
        ("settings-drop-confirm", "settings-drop-confirm-help"),
        Control::Choices(&DROP_CONFIRMS, Look::Segments),
    ),
    spec(
        (F::RenameLog, S::Files, "files.rename-log"),
        ("settings-rename-log", "settings-rename-log-help"),
        Control::Toggle,
    ),
    spec(
        (
            F::SpaceMeasuresFolders,
            S::Selection,
            "selection.space-measures-folders",
        ),
        (
            "Space measures folders",
            "Space on a folder marks it and shows its total size",
        ),
        Control::Toggle,
    ),
    spec(
        (
            F::PatternsSelectFolders,
            S::Selection,
            "selection.patterns-select-folders",
        ),
        (
            "Patterns select folders",
            "Select with a pattern marks matching folders too",
        ),
        Control::Toggle,
    ),
    spec(
        (F::TerminalProvider, S::Terminal, "terminal.provider"),
        ("Terminal", "The terminal Navo opens in a folder"),
        Control::Text,
    ),
    spec(
        (F::TerminalCommand, S::Terminal, "terminal.command"),
        ("Command", "Your own command line; {path} is the folder"),
        Control::Text,
    ),
    spec(
        (F::EditorPath, S::Editor, "editor.path"),
        ("Editor", "The program F4 opens files with"),
        Control::Text,
    ),
    spec(
        (F::CopyMode, S::Copy, "copy.mode"),
        (
            "Copy mode",
            "Auto copies several files at once where the drives allow; Standard copies one file at a time.",
        ),
        Control::Choices(&COPY_MODES, Look::Segments),
    ),
];
