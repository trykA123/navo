use crate::primitives::IconName;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Appearance,
    Files,
    Selection,
    Terminal,
    Editor,
    Copy,
    Display,
}

impl Section {
    pub const ALL: [Self; 7] = [
        Self::Appearance,
        Self::Files,
        Self::Selection,
        Self::Terminal,
        Self::Editor,
        Self::Copy,
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
    SpaceMeasuresFolders,
    PatternsSelectFolders,
    TerminalProvider,
    TerminalCommand,
    EditorPath,
    CopyMode,
    TextLineNumbers,
    TextLineEnds,
    ImageDetails,
    AutomotivePreset,
    AutomotiveInspector,
    AutomotiveMemoryStrip,
    AutomotiveCharts,
    AutomotiveRowBars,
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

pub const FIELDS: [FieldSpec; 27] = [
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
    spec(
        (F::TextLineNumbers, S::Display, "text.view.line-numbers"),
        (
            "Line numbers",
            "Shows the number of each line at its start.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::TextLineEnds, S::Display, "text.view.line-ends"),
        ("Line ends", "Marks where each line ends, as LF or CRLF."),
        Control::Toggle,
    ),
    spec(
        (F::ImageDetails, S::Display, "image.view.details"),
        (
            "Details",
            "Shows a panel with the camera, date and other EXIF data.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::AutomotivePreset, S::Display, "automotive.view.preset"),
        (
            "View",
            "Simple keeps the view calm, Rich turns every part on, Custom uses the toggles below.",
        ),
        Control::Choices(&PRESETS, Look::Segments),
    ),
    spec(
        (
            F::AutomotiveInspector,
            S::Display,
            "automotive.view.inspector",
        ),
        (
            "Open the inspector",
            "Opens the details drawer by itself when you pick a range or section.",
        ),
        Control::Toggle,
    ),
    spec(
        (
            F::AutomotiveMemoryStrip,
            S::Display,
            "automotive.view.memory-strip",
        ),
        (
            "Memory strip",
            "Shows a map of the address space beside the ranges list.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::AutomotiveCharts, S::Display, "automotive.view.charts"),
        (
            "Charts",
            "Shows bars, sparklines, bus load and signal curves where a view has them.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::AutomotiveRowBars, S::Display, "automotive.view.row-bars"),
        (
            "Row bars",
            "Shows a size bar and counts on each row of the ranges list.",
        ),
        Control::Toggle,
    ),
];

const GROUPS: [(&str, &str); 3] = [
    ("text", "Text"),
    ("image", "Image"),
    ("automotive", "Automotive"),
];

impl FieldSpec {
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
        )
    }
}
