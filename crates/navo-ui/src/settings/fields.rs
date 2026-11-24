use super::clipboard_fields::CLIPBOARD_FIELDS;
use super::compare_fields::COMPARE_FIELDS;
use super::general_fields::GENERAL;
use super::git_fields::GIT_FIELDS;
pub use super::pages::Section;
use super::search_fields::SEARCH_ROWS;
use super::tray_fields::TRAY_FIELDS;
use super::update_fields::UPDATE_FIELDS;
use super::viewer_fields::VIEWER_FIELDS;
use super::window_fields::WINDOW_FIELDS;
use crate::prelude::SharedString;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Field {
    Theme,
    Density,
    ColorTheme,
    LightTheme,
    DarkTheme,
    Font,
    FontSize,
    UiScale,
    ModifierHints,
    ModifierHintsDelay,
    HiddenFiles,
    ShowSystem,
    FoldersFirst,
    Sort,
    SortDescending,
    SizeMode,
    MeasureShownFolders,
    DropConfirm,
    RenameLog,
    SpaceMeasuresFolders,
    PatternsSelectFolders,
    RightButton,
    TerminalProvider,
    TerminalCommand,
    EditorPath,
    CopyMode,
    CopyVerify,
    RetainDeletes,
    RetainOverwrites,
    RetentionDays,
    RetentionCapMib,
    RetentionMinFreeMib,
    SearchIndex,
    SearchDrives,
    SearchExclude,
    SearchNetwork,
    SearchRemovable,
    SearchRemember,
    GitBadges,
    GitTool,
    GitToolCommand,
    GitEngine,
    GitFetch,
    GitFetchMinutes,
    GitNetwork,
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
    WindowExtWidth,
    WindowSizeWidth,
    WindowDateWidth,
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
    AutomotiveModules,
    KeepInTray,
    SummonKey,
    ClipboardOn,
    ClipboardRecord,
    ClipboardKey,
    ClipboardPosition,
    CompareGitEngine,
    CompareIgnoreList,
    CompareRespectGitignore,
    CompareBudgetMib,
    CompareContentCheck,
    CheckUpdates,
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
pub const COLOR_THEMES: [Choice; 6] = [
    choice("settings-color-theme-open-sea", "open-sea"),
    choice("settings-color-theme-red-bill", "red-bill"),
    choice("settings-color-theme-black-cap", "black-cap"),
    choice("settings-color-theme-fine-mesh", "fine-mesh"),
    choice("settings-color-theme-grain", "grain"),
    choice("settings-color-theme-ink-and-amber", "ink-and-amber"),
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
pub const COPY_MODES: [Choice; 2] = [
    choice("settings-copy-mode-auto", "auto"),
    choice("settings-copy-mode-standard", "standard"),
];
pub const COPY_VERIFY: [Choice; 3] = [
    choice("Auto", "auto"),
    choice("Always", "always"),
    choice("Off", "off"),
];
pub const SYNTAX_THEMES: [Choice; 2] = [
    choice("settings-syntax-theme-oxidised-iron", "oxidised-iron"),
    choice("settings-syntax-theme-mocha", "mocha"),
];
pub const SIZE_MODES: [Choice; 2] = [
    choice("settings-size-mode-apparent", "apparent"),
    choice("settings-size-mode-on-disk", "on-disk"),
];
pub const DROP_CONFIRMS: [Choice; 3] = [
    choice("settings-drop-confirm-never", "never"),
    choice("settings-drop-confirm-moves", "moves"),
    choice("settings-drop-confirm-always", "always"),
];
pub const RIGHT_BUTTONS: [Choice; 2] = [
    choice("settings-right-button-total-commander", "total-commander"),
    choice("settings-right-button-windows", "windows"),
];
pub const SORTS: [Choice; 4] = [
    choice("Name", "name"),
    choice("Size", "size"),
    choice("Date", "date"),
    choice("Extension", "kind"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    Segments,
    ThemePictures,
    ColorPictures,
    DensityPictures,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    Choices(&'static [Choice], Look),
    Toggle,
    Text,
    Number,
    /// One tool of the Git page, stored as its id.
    Tool,
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

pub const FIELDS: [FieldSpec; 87] = join([
    &GENERAL,
    &SEARCH_ROWS,
    &GIT_FIELDS,
    &WINDOW_FIELDS,
    &VIEWER_FIELDS,
    &TRAY_FIELDS,
    &CLIPBOARD_FIELDS,
    &COMPARE_FIELDS,
    &UPDATE_FIELDS,
]);

const fn join<const N: usize>(parts: [&[FieldSpec]; 9]) -> [FieldSpec; N] {
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

const MESSAGE_PREFIX: &str = "settings-";

fn shown(text: &'static str) -> SharedString {
    if text.starts_with(MESSAGE_PREFIX) {
        navo_l10n::tr(text).into()
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
}
