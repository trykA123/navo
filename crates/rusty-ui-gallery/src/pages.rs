mod table;

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
        )
    }

    pub fn info(self) -> &'static PageInfo {
        &PAGES[self as usize]
    }
}
