use super::fields::{
    COLOR_THEMES, COPY_MODES, COPY_VERIFY, Choice, Control, DENSITIES, DROP_CONFIRMS, F, FieldSpec,
    HIDDEN, HINTS, Look, RIGHT_BUTTONS, S, SIZE_MODES, SORTS, THEMES, choice, spec,
};

const TERMINAL_PROVIDERS: [Choice; 5] = [
    choice("settings-terminal-provider-system", "system"),
    choice(
        "settings-terminal-provider-windows-terminal",
        "windows-terminal",
    ),
    choice("settings-terminal-provider-powershell", "powershell"),
    choice("settings-terminal-provider-cmd", "cmd"),
    choice("settings-terminal-provider-custom", "custom"),
];

pub(super) const GENERAL: [FieldSpec; 32] = [
    spec(
        (F::Theme, S::Appearance, "appearance.mode"),
        ("Theme", "settings-theme-help"),
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
        (F::UiScale, S::Appearance, "appearance.ui-scale"),
        ("settings-ui-scale", "settings-ui-scale-help"),
        Control::Number,
    ),
    spec(
        (F::ModifierHints, S::Keyboard, "appearance.modifier-hints"),
        ("Modifier hints", "What holding Ctrl, Shift or Alt shows"),
        Control::Choices(&HINTS, Look::Segments),
    ),
    spec(
        (
            F::ModifierHintsDelay,
            S::Keyboard,
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
        (F::SizeMode, S::Files, "files.size-mode"),
        ("settings-size-mode", "settings-size-mode-help"),
        Control::Choices(&SIZE_MODES, Look::Segments),
    ),
    spec(
        (
            F::MeasureShownFolders,
            S::Files,
            "files.measure-shown-folders",
        ),
        (
            "settings-measure-shown-folders",
            "settings-measure-shown-folders-help",
        ),
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
            S::Files,
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
            S::Files,
            "selection.patterns-select-folders",
        ),
        (
            "Patterns select folders",
            "Select with a pattern marks matching folders too",
        ),
        Control::Toggle,
    ),
    spec(
        (F::RightButton, S::Files, "selection.right-button"),
        ("settings-right-button", "settings-right-button-help"),
        Control::Choices(&RIGHT_BUTTONS, Look::Segments),
    ),
    spec(
        (F::TerminalProvider, S::Tools, "terminal.provider"),
        (
            "settings-external-terminal",
            "settings-external-terminal-help",
        ),
        Control::Choices(&TERMINAL_PROVIDERS, Look::Segments),
    ),
    spec(
        (F::TerminalCommand, S::Tools, "terminal.command"),
        ("Command", "Your own command line; {path} is the folder"),
        Control::Text,
    ),
    spec(
        (F::EditorPath, S::Tools, "editor.path"),
        ("Editor", "The program F4 opens files with"),
        Control::Text,
    ),
    spec(
        (F::CopyMode, S::CopyDelete, "copy.mode"),
        ("settings-copy-mode", "settings-copy-mode-help"),
        Control::Choices(&COPY_MODES, Look::Segments),
    ),
    spec(
        (F::CopyVerify, S::CopyDelete, "copy.verify"),
        ("settings-copy-verify", "settings-copy-verify-help"),
        Control::Choices(&COPY_VERIFY, Look::Segments),
    ),
    spec(
        (F::RetainDeletes, S::CopyDelete, "retention.retain-deletes"),
        ("settings-retain-deletes", "settings-retain-deletes-help"),
        Control::Toggle,
    ),
    spec(
        (
            F::RetainOverwrites,
            S::CopyDelete,
            "retention.retain-overwrites",
        ),
        (
            "settings-retain-overwrites",
            "settings-retain-overwrites-help",
        ),
        Control::Toggle,
    ),
    spec(
        (F::RetentionDays, S::CopyDelete, "retention.days"),
        ("settings-retention-days", "settings-retention-days-help"),
        Control::Number,
    ),
    spec(
        (
            F::RetentionCapMib,
            S::CopyDelete,
            "retention.max-volume-mib",
        ),
        ("settings-retention-cap", "settings-retention-cap-help"),
        Control::Number,
    ),
    spec(
        (
            F::RetentionMinFreeMib,
            S::CopyDelete,
            "retention.min-free-mib",
        ),
        (
            "settings-retention-min-free",
            "settings-retention-min-free-help",
        ),
        Control::Number,
    ),
];
