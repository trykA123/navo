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
    Archive,
    Font,
    Marks,
    Image,
    Menu,
    MenuSubmenu,
    MenuFilter,
    MenuFiles,
    ViewerMenuText,
    ViewerMenuTree,
    Tokens,
    Window,
    WindowShift,
    WindowAlt,
    WindowNarrow,
    WindowJob,
    WindowJobPill,
    WindowJobs,
    WindowJobsPill,
    AppMenu,
    Sheet,
    SheetLeft,
    SheetNarrow,
}

pub struct PageInfo {
    pub page: Page,
    pub label: &'static str,
    pub stem: &'static str,
    pub in_switch: bool,
}

const fn info(page: Page, label: &'static str, stem: &'static str, in_switch: bool) -> PageInfo {
    PageInfo {
        page,
        label,
        stem,
        in_switch,
    }
}

pub const PAGES: [PageInfo; 34] = [
    info(Page::Primitives, "Parts", "p7-gallery", true),
    info(Page::Pane, "Pane", "p7-pane", true),
    info(Page::Operate, "Operate", "p8-confirm", true),
    info(Page::Hex, "Hex", "p13-hex", true),
    info(Page::PagedText, "Text", "p13-text", true),
    info(Page::Drives, "Drives", "p14-drives", true),
    info(Page::TypedPath, "Path", "p14-path", true),
    info(Page::Tree, "Tree", "p15-tree", false),
    info(Page::Table, "Table", "p15-table", false),
    info(Page::Markdown, "Markdown", "p15-markdown", false),
    info(
        Page::MarkdownPreview,
        "Markdown preview",
        "p15-markdown-preview",
        false,
    ),
    info(Page::Archive, "Zip", "p15-archive", false),
    info(Page::Font, "Font", "p15-font", false),
    info(Page::Marks, "Marks", "p15-marks", false),
    info(Page::Image, "Image", "p15-image", true),
    info(Page::Menu, "Menu", "p17-menu", false),
    info(Page::MenuSubmenu, "Menu 7-Zip", "p17-menu-7zip", false),
    info(Page::MenuFilter, "Menu filter", "p17-menu-filter", false),
    info(Page::MenuFiles, "Menu files", "p17-menu-files", false),
    info(
        Page::ViewerMenuText,
        "Viewer menu",
        "p17-viewer-menu-text",
        false,
    ),
    info(
        Page::ViewerMenuTree,
        "Viewer menu, tree",
        "p17-viewer-menu-tree",
        false,
    ),
    info(Page::Tokens, "Tokens", "p19-tokens", false),
    info(Page::Window, "Window", "p19-window", false),
    info(
        Page::WindowShift,
        "Window, Shift held",
        "p19-fkeys-shift",
        false,
    ),
    info(Page::WindowAlt, "Window, Alt held", "p19-fkeys-alt", false),
    info(
        Page::WindowNarrow,
        "Window at 1100 px",
        "p19-window-1100",
        false,
    ),
    info(
        Page::WindowJob,
        "Window, copy running",
        "p19-window-job",
        false,
    ),
    info(
        Page::WindowJobPill,
        "Window, copy pill",
        "p19-job-pill",
        false,
    ),
    info(Page::WindowJobs, "Window, two jobs", "p19-jobs-card", false),
    info(
        Page::WindowJobsPill,
        "Window, two jobs pill",
        "p19-jobs-pill",
        false,
    ),
    info(Page::AppMenu, "App menu", "p19-app-menu", false),
    info(Page::Sheet, "Side sheet", "p21-sheet", false),
    info(Page::SheetLeft, "Side sheet, left", "p21-sheet-left", false),
    info(
        Page::SheetNarrow,
        "Side sheet at 1100 px",
        "p21-sheet-1100",
        false,
    ),
];

impl Page {
    pub fn info(self) -> &'static PageInfo {
        &PAGES[self as usize]
    }
}
