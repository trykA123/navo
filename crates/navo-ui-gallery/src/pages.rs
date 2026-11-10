mod clip_palette;
mod command_center;
mod later;
mod recovery;
mod setup;
mod table;
mod tray;

pub use table::PAGES;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Primitives,
    Pane,
    Operate,
    Hex,
    PagedText,
    Drives,
    TypedPath,
    Tree,
    Table,
    Markdown,
    MarkdownPreview,
    Code,
    Archive,
    Font,
    Marks,
    Image,
    Menu,
    MenuSubmenu,
    MenuFilter,
    MenuFiles,
    MenuBackground,
    MenuEmptyFolder,
    MenuLoading,
    MenuFailed,
    DropMenu,
    ViewerMenuText,
    ViewerMenuTree,
    Tokens,
    Window,
    WindowShift,
    WindowCtrl,
    WindowAlt,
    WindowNarrow,
    WindowJob,
    WindowJobStrip,
    WindowJobs,
    WindowJobsStrip,
    WindowActivityDrawer,
    SixStrip,
    Six390,
    SixDrawer,
    AppMenu,
    Sheet,
    SheetLeft,
    SheetNarrow,
    Settings,
    SettingsFiles,
    Shortcuts,
    ShortcutsRecording,
    ShortcutsConflict,
    ShortcutsInvalid,
    HintsCtrl,
    HintsShift,
    HintsAlt,
    HintsLow,
    CrashNotice,
    MemoryImage,
    MemoryDrawers,
    ViewMenuText,
    ViewerEncoding,
    ViewMenuAutomotive,
    ViewMenuElf,
    ViewMenuTrace,
    SettingsDisplay,
    ElfSymbols,
    ElfDrawers,
    TraceTable,
    TraceDrawers,
    MapTables,
    MapDrawers,
    MapUnread,
    PdfCalm,
    PdfSearch,
    PdfDrawers,
    PdfOutline,
    WindowPartsOn,
    WindowPartsOff,
    WindowViewMenu,
    ViewMenuMarkdown,
    PreviewPartsOn,
    PreviewNoBars,
    PreviewNoLargest,
    PreviewPartsOff,
    SettingsUpgrade,
    SettingsFind,
    SearchSettings,
    IndexProgress,
    SettingsHelpUpdates,
    SettingsHelpUpdatesPhone,
    SettingsHelpUnpackaged,
    MouseMarks,
    FileKinds,
    Columns,
    Compare,
    CompareDiff,
    CompareDiff720,
    CompareStream,
    CompareStream720,
    CommandCenter,
    CommandCenterRecent,
    CommandCenterCommands,
    CommandCenterPath,
    CommandCenterArgument,
    CommandCenterNoMatch,
    CommandCenterIndexing,
    Drag,
    MemoryStrip,
    Treemap,
    UsageBars,
    MultiRename,
    MultiRenamePreview,
    MultiRenamePreviewLarge,
    MultiRenameTool,
    MultiRenameToolMenu,
    ElfRich,
    ElfNoStrip,
    ElfNoBars,
    ElfNoWho,
    ElfNoRowBars,
    MapRich,
    MapNoStrip,
    MapNoBars,
    MapNoWho,
    MapNoRowBars,
    ElfStripKeys,
    MdfTable,
    RecoveryNotice,
    RecoveryNoticeNarrow,
    Palettes,
    WorkspaceTray,
    WorkspaceTrayPhone,
    LayoutSwitcher,
    LayoutSwitcherPhone,
    LayoutSave,
    LayoutRemove,
    RecentlyDeleted,
    RecentlyDeletedPhone,
    GitBadges,
    GitBadgesMinimum,
    GitMenu,
    GitMenuMinimum,
    SettingsGit,
    SettingsGitMinimum,
    ClipPalette,
    ClipPalette390,
    ClipPaletteMenu,
    ClipPaletteMenu390,
    ClipPaletteSecret,
    ClipPaletteSecret390,
    ClipPaletteShown,
    ClipPaletteShown390,
    ClipPaletteCountdown,
    ClipPaletteCountdown390,
    ClipPaletteSecretMenu,
    ClipPaletteSecretMenu390,
    ClipPopUp,
    ClipPopUp390,
    ClipPopUpMenu,
    ClipManage,
    ClipManage390,
    ClipNotes,
    ClipNotes390,
    ClipEdit,
    ClipEdit390,
    ClipEditSecret,
    ClipEditSecret390,
    SetupWelcome,
    SetupWelcomeUpdate,
    SetupLicence,
    SetupLicenceAccepted,
    SetupFolder,
    SetupFolderLocked,
    SetupFolderProblem,
    SetupShortcuts,
    SetupInstalling,
    SetupInstallingSlow,
    SetupFinish,
    SetupFailedRetry,
    SetupFailedLeftover,
    SetupBlocked,
    SetupBlockedRetry,
    SetupSame,
    SetupNewer,
    SetupAmbiguous,
    SetupAmbiguousNotice,
    SetupElevated,
    SetupNoPayload,
}

#[derive(Clone, Copy)]
pub struct PageInfo {
    pub page: Page,
    pub label: &'static str,
    pub stem: &'static str,
    pub in_switch: bool,
}

impl Page {
    pub fn is_viewer_sample(self) -> bool {
        matches!(
            self,
            Self::Tree
                | Self::Table
                | Self::Markdown
                | Self::MarkdownPreview
                | Self::Code
                | Self::MemoryImage
                | Self::MemoryDrawers
                | Self::ElfSymbols
                | Self::ElfDrawers
                | Self::TraceTable
                | Self::TraceDrawers
                | Self::MapTables
                | Self::MapDrawers
                | Self::MapUnread
        )
    }

    pub fn info(self) -> &'static PageInfo {
        &PAGES[self as usize]
    }
}

pub(super) const fn info(
    page: Page,
    label: &'static str,
    stem: &'static str,
    in_switch: bool,
) -> PageInfo {
    PageInfo {
        page,
        label,
        stem,
        in_switch,
    }
}

/// The pages of `head` followed by those of `tail`; a length that does not add up fails the build.
pub(super) const fn join<const N: usize>(head: &[PageInfo], tail: &[PageInfo]) -> [PageInfo; N] {
    let mut all = [head[0]; N];
    let mut at = 0;
    while at < N {
        all[at] = if at < head.len() {
            head[at]
        } else {
            tail[at - head.len()]
        };
        at += 1;
    }
    all
}
