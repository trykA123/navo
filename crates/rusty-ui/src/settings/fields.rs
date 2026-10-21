use super::search_fields::SEARCH_ROWS;
use super::viewer_fields::VIEWER_FIELDS;
use super::window_fields::WINDOW_FIELDS;
use crate::prelude::SharedString;
use crate::primitives::IconName;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Appearance,
    Files,
    Selection,
    Terminal,
    Editor,
    Copy,
    Search,
    Display,
}

impl Section {
    pub const ALL: [Self; 8] = [
        Self::Appearance,
        Self::Files,
        Self::Selection,
        Self::Terminal,
        Self::Editor,
        Self::Copy,
        Self::Search,
        Self::Display,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Files => "Files",
            Self::Selection => "Selection",
            Self::Terminal => "Terminal",
            Self::Editor => "Editor",
            Self::Copy => "Copy",
            Self::Search => "Search",
            Self::Display => "Display",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Appearance => IconName::Contrast,
            Self::Files => IconName::Folder,
            Self::Selection => IconName::MousePointer,
            Self::Terminal => IconName::SquareTerminal,
            Self::Editor => IconName::FilePen,
            Self::Copy => IconName::Copy,
            Self::Search => IconName::Search,
            Self::Display => IconName::Eye,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Field {
    Theme,
    Density,
    LightTheme,
    DarkTheme,
    Font,
    FontSize,
    ModifierHints,
    ModifierHintsDelay,
    HiddenFiles,
    ShowSystem,
    FoldersFirst,
    Sort,
    SortDescending,
    DropConfirm,
    SpaceMeasuresFolders,
    PatternsSelectFolders,
    TerminalProvider,
    TerminalCommand,
    EditorPath,
    CopyMode,
    SearchIndex,
    SearchDrives,
    SearchExclude,
    SearchNetwork,
    SearchRemovable,
    SearchRemember,
    WindowTabsWithOne,
    WindowPathBar,
    WindowDriveButton,
    WindowTitleSearch,
    WindowFkeyBar,
    WindowIcons,
    WindowExtColumn,
    WindowSizeColumn,
    WindowDateColumn,
    WindowFreeSpace,
    WindowMarkedTotals,
    WindowContents,
    TextLineNumbers,
    TextLineEnds,
    TextSyntaxTheme,
    ImageDetails,
    MarkdownPreviewSplit,
    PreviewTypeBars,
    PreviewLargestFiles,
    AutomotivePreset,
    AutomotiveInspector,
    AutomotiveMemoryStrip,
    AutomotiveCharts,
    AutomotiveRowBars,
    AutomotiveUsageBars,
    AutomotiveWhoUses,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Choice {
    label: &'static str,
    pub toml: &'static str,
}

pub(super) const fn choice(label: &'static str, toml: &'static str) -> Choice {
    Choice { label, toml }
}

pub const THEMES: [Choice; 3] = [
    choice("System", "system"),
    choice("Light", "light"),
    choice("Dark", "dark"),
];
pub const DENSITIES: [Choice; 3] = [
    choice("Compact", "compact"),
    choice("Normal", "normal"),
    choice("Comfortable", "comfortable"),
];
pub const HINTS: [Choice; 3] = [
    choice("Off", "off"),
    choice("F-keys only", "fkeys"),
    choice("All shortcuts", "all"),
];
pub const HIDDEN: [Choice; 2] = [choice("Shown", "shown"), choice("Hidden", "hidden")];
pub const PRESETS: [Choice; 3] = [
    choice("Simple", "simple"),
    choice("Rich", "rich"),
    choice("Custom", "custom"),
];
pub const COPY_MODES: [Choice; 2] = [choice("Auto", "auto"), choice("Standard", "standard")];
pub const SYNTAX_THEMES: [Choice; 2] = [
    choice("settings-syntax-theme-oxidised-iron", "oxidised-iron"),
    choice("settings-syntax-theme-mocha", "mocha"),
];
pub const DROP_CONFIRMS: [Choice; 3] = [
    choice("settings-drop-confirm-never", "never"),
    choice("settings-drop-confirm-moves", "moves"),
    choice("settings-drop-confirm-always", "always"),
];
pub const SORTS: [Choice; 4] = [
    choice("Name", "name"),
    choice("Size", "size"),
    choice("Date", "date"),
    choice("Kind", "kind"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    Segments,
    ThemePictures,
    DensityPictures,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    Choices(&'static [Choice], Look),
    Toggle,
    Text,
    Number,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FieldSpec {
    pub field: Field,
    pub section: Section,
    pub key: &'static str,
    label: &'static str,
    pub help: &'static str,
    pub control: Control,
}

pub(super) const fn spec(
    (field, section, key): (Field, Section, &'static str),
    (label, help): (&'static str, &'static str),
    control: Control,
) -> FieldSpec {
    FieldSpec {
        field,
        section,
        key,
        label,
        help,
        control,
    }
}

pub(super) use Field as F;
pub(super) use Section as S;

const GENERAL: [FieldSpec; 20] = [
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
        ("Terminal", "The terminal Ferrite opens in a folder"),
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

pub const FIELDS: [FieldSpec; 52] = join([&GENERAL, &SEARCH_ROWS, &WINDOW_FIELDS, &VIEWER_FIELDS]);

const fn join<const N: usize>(parts: [&[FieldSpec]; 4]) -> [FieldSpec; N] {
    let mut all = [parts[0][0]; N];
    let mut at = 0;
    let mut part = 0;
    while part < parts.len() {
        let mut index = 0;
        while index < parts[part].len() {
            all[at] = parts[part][index];
            at += 1;
            index += 1;
        }
        part += 1;
    }
    assert!(at == N, "FIELDS has the wrong length");
    all
}

const GROUPS: [(&str, &str); 6] = [
    ("window", "Window"),
    ("text", "Text"),
    ("image", "Image"),
    ("markdown", "Markdown"),
    ("preview", "Preview"),
    ("automotive", "Automotive"),
];

const MESSAGE_PREFIX: &str = "settings-";

fn shown(text: &'static str) -> SharedString {
    if text.starts_with(MESSAGE_PREFIX) {
        rusty_l10n::tr(text).into()
    } else {
        text.into()
    }
}

impl Choice {
    pub fn text(&self) -> SharedString {
        shown(self.label)
    }
}

impl FieldSpec {
    pub fn label(&self) -> SharedString {
        shown(self.label)
    }

    pub fn help(&self) -> SharedString {
        shown(self.help)
    }

    pub fn table(&self) -> &'static str {
        self.key.split('.').next().unwrap_or_default()
    }

    pub fn group(&self) -> Option<&'static str> {
        let table = self.table();
        GROUPS
            .iter()
            .find(|(name, _)| *name == table)
            .map(|(_, label)| *label)
    }
}

impl Field {
    pub fn spec(self) -> &'static FieldSpec {
        &FIELDS[self.index()]
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn is_window_part(self) -> bool {
        (Self::WindowTabsWithOne.index()..=Self::WindowContents.index()).contains(&self.index())
    }

    pub fn is_applied(self) -> bool {
        !matches!(
            self,
            Self::LightTheme
                | Self::DarkTheme
                | Self::Font
                | Self::FontSize
                | Self::TerminalProvider
                | Self::TerminalCommand
                | Self::AutomotiveMemoryStrip
                | Self::AutomotiveCharts
                | Self::AutomotiveRowBars
                | Self::AutomotiveUsageBars
                | Self::AutomotiveWhoUses
        )
    }
}
