//! The Settings pages: their names and icons, and the group and order of every shown
//! setting on its page (the README's "old place to new place" map).

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use super::fields::{FIELDS, Field as F, FieldSpec};
use crate::primitives::IconName;

/// One Settings page in the nav. Each field names its page in its `*_fields.rs` spec.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Appearance,
    Window,
    Files,
    CopyDelete,
    Search,
    Viewers,
    Tools,
    TrayClipboard,
    Keyboard,
    About,
}

#[expect(
    non_upper_case_globals,
    reason = "old section names that the row view still asks for; both are now About"
)]
impl Section {
    pub const Help: Self = Self::About;
    pub const Diagnostics: Self = Self::About;
}

impl Section {
    pub const ALL: [Self; 10] = [
        Self::Appearance,
        Self::Window,
        Self::Files,
        Self::CopyDelete,
        Self::Search,
        Self::Viewers,
        Self::Tools,
        Self::TrayClipboard,
        Self::Keyboard,
        Self::About,
    ];

    /// The ftl message id of the page name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "settings-page-appearance",
            Self::Window => "settings-page-window",
            Self::Files => "settings-page-files",
            Self::CopyDelete => "settings-page-copy-delete",
            Self::Search => "settings-page-search",
            Self::Viewers => "settings-page-viewers",
            Self::Tools => "settings-page-tools",
            Self::TrayClipboard => "settings-page-tray-clipboard",
            Self::Keyboard => "settings-page-keyboard",
            Self::About => "settings-page-about",
        }
    }

    /// True when the label is an ftl message id rather than plain text.
    pub fn is_translated(self) -> bool {
        self.label().starts_with("settings-")
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Appearance => IconName::Contrast,
            Self::Window => IconName::AppWindow,
            Self::Files => IconName::Folder,
            Self::CopyDelete => IconName::Shield,
            Self::Search => IconName::Search,
            Self::Viewers => IconName::Eye,
            Self::Tools => IconName::Wrench,
            Self::TrayClipboard => IconName::Clipboard,
            Self::Keyboard => IconName::Keyboard,
            Self::About => IconName::CircleQuestionMark,
        }
    }
}

/// A titled run of settings on one page, in the order the page lists them; `label` is an
/// ftl message id.
pub struct Group {
    pub label: &'static str,
    pub fields: &'static [F],
}

const fn group(label: &'static str, fields: &'static [F]) -> Group {
    Group { label, fields }
}

/// Every shown field once, page by page in nav order (each field's spec names its page);
/// retired fields are in no group.
pub const GROUPS: [Group; 23] = [
    group(
        "settings-group-look",
        &[F::Theme, F::ColorTheme, F::Density],
    ),
    group(
        "settings-group-text-and-size",
        &[F::FontSize, F::UiScale, F::Font],
    ),
    group(
        "settings-group-window-parts",
        &[
            F::WindowTabsWithOne,
            F::WindowPathBar,
            F::WindowDriveButton,
            F::WindowTitleSearch,
            F::WindowFkeyBar,
            F::WindowIcons,
        ],
    ),
    group(
        "settings-group-file-list-columns",
        &[
            F::WindowExtColumn,
            F::WindowSizeColumn,
            F::WindowDateColumn,
            F::WindowExtWidth,
            F::WindowSizeWidth,
            F::WindowDateWidth,
        ],
    ),
    group(
        "settings-group-pane-footer",
        &[F::WindowFreeSpace, F::WindowMarkedTotals, F::WindowContents],
    ),
    group(
        "settings-group-in-the-list",
        &[
            F::HiddenFiles,
            F::ShowSystem,
            F::FoldersFirst,
            F::Sort,
            F::SortDescending,
        ],
    ),
    group(
        "settings-group-folder-sizes",
        &[F::SizeMode, F::MeasureShownFolders, F::SpaceMeasuresFolders],
    ),
    group(
        "settings-group-mouse-and-actions",
        &[
            F::RightButton,
            F::PatternsSelectFolders,
            F::DropConfirm,
            F::RenameLog,
        ],
    ),
    group("settings-group-copy", &[F::CopyVerify, F::CopyMode]),
    group(
        "settings-safety",
        &[
            F::RetainDeletes,
            F::RetainOverwrites,
            F::RetentionDays,
            F::RetentionCapMib,
            F::RetentionMinFreeMib,
        ],
    ),
    group(
        "settings-group-index",
        &[
            F::SearchIndex,
            F::SearchNetwork,
            F::SearchRemovable,
            F::SearchDrives,
            F::SearchExclude,
        ],
    ),
    group("settings-group-ctrl-k-history", &[F::SearchRemember]),
    group(
        "settings-group-text-and-code",
        &[F::TextLineNumbers, F::TextLineEnds, F::TextSyntaxTheme],
    ),
    group(
        "settings-group-images-and-markdown",
        &[F::ImageDetails, F::MarkdownPreviewSplit],
    ),
    group(
        "settings-group-folder-preview",
        &[F::PreviewTypeBars, F::PreviewLargestFiles],
    ),
    group(
        "settings-group-automotive-files",
        &[
            F::AutomotivePreset,
            F::AutomotiveInspector,
            F::AutomotiveModules,
        ],
    ),
    group(
        "settings-group-terminal-and-editor",
        &[F::TerminalProvider, F::TerminalCommand, F::EditorPath],
    ),
    group(
        "settings-group-git",
        &[
            F::GitBadges,
            F::GitTool,
            F::GitToolCommand,
            F::GitFetch,
            F::GitFetchMinutes,
            F::GitEngine,
            F::GitNetwork,
        ],
    ),
    group(
        "settings-group-compare",
        &[
            F::CompareIgnoreList,
            F::CompareRespectGitignore,
            F::CompareContentCheck,
            F::CompareGitEngine,
            F::CompareBudgetMib,
        ],
    ),
    group("settings-group-tray", &[F::KeepInTray, F::SummonKey]),
    group(
        "settings-group-clipboard",
        &[
            F::ClipboardOn,
            F::ClipboardRecord,
            F::ClipboardKey,
            F::ClipboardPosition,
        ],
    ),
    group(
        "settings-group-modifier-hints",
        &[F::ModifierHints, F::ModifierHintsDelay],
    ),
    group("settings-group-navo", &[F::CheckUpdates]),
];

/// Every grouped row as `(row, spec)`, in page order and then in the order the page shows them.
pub fn ordered_rows() -> impl Iterator<Item = (usize, &'static FieldSpec)> {
    GROUPS
        .iter()
        .flat_map(|group| group.fields)
        .map(|field| (field.index(), &FIELDS[field.index()]))
}

/// The shown group title of `field` in the current language, or `None` for a retired field.
pub fn group_of(field: F) -> Option<&'static str> {
    GROUPS
        .iter()
        .find(|group| group.fields.contains(&field))
        .map(|group| interned(navo_l10n::tr(group.label)))
}

/// Interns a translated text so the row view can keep its `&'static str` headings; each
/// distinct text leaks once (a few dozen short strings per language).
fn interned(text: String) -> &'static str {
    static TEXTS: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let mut texts = TEXTS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(known) = texts.get(text.as_str()) {
        return known;
    }
    let leaked: &'static str = Box::leak(text.into_boxed_str());
    texts.insert(leaked);
    leaked
}
