use crate::primitives::IconName;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Appearance,
    Files,
    Selection,
    Terminal,
    Editor,
}

impl Section {
    pub const ALL: [Self; 5] = [
        Self::Appearance,
        Self::Files,
        Self::Selection,
        Self::Terminal,
        Self::Editor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Files => "Files",
            Self::Selection => "Selection",
            Self::Terminal => "Terminal",
            Self::Editor => "Editor",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Appearance => IconName::Contrast,
            Self::Files => IconName::Folder,
            Self::Selection => IconName::MousePointer,
            Self::Terminal => IconName::SquareTerminal,
            Self::Editor => IconName::FilePen,
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
    HiddenFiles,
    ShowSystem,
    FoldersFirst,
    Sort,
    SortDescending,
    SpaceMeasuresFolders,
    PatternsSelectFolders,
    TerminalProvider,
    TerminalCommand,
    EditorPath,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Choice {
    pub label: &'static str,
    pub toml: &'static str,
}

const fn choice(label: &'static str, toml: &'static str) -> Choice {
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
pub const HIDDEN: [Choice; 2] = [choice("Shown", "shown"), choice("Hidden", "hidden")];
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
    pub label: &'static str,
    pub help: &'static str,
    pub control: Control,
}

const fn spec(
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

use Field as F;
use Section as S;

pub const FIELDS: [FieldSpec; 16] = [
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
];

impl Field {
    pub fn spec(self) -> &'static FieldSpec {
        &FIELDS[self.index()]
    }

    pub fn index(self) -> usize {
        self as usize
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
        )
    }
}
