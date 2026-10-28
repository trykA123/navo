mod command_center;
mod recovery;
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
    MouseMarks,
    FileKinds,
    Columns,
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
}

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
