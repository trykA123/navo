use rusty_ui::Focusable;
use rusty_ui::fkey_bar::{FKeyBar, KeyLayer};
use rusty_ui::prelude::*;
use rusty_ui::primitives::{
    Button, ButtonVariant, IconButton, IconName, ListScroll, SplitterState,
};
use rusty_ui::tokens::{self, Mode, Theme};
use rusty_ui::window_view::{
    ArchiveView, BodyKeys, FontView, ImageView, PagedView, PdfView, ViewerBody,
};
use rusty_ui::{FerriteTitleBar, HeaderSearch};

use crate::automotive_sample::{
    ELF_NAME, ELF_STEPS, MEMORY_NAME, MEMORY_STEPS, TRACE_NAME, TRACE_STEPS, elf, memory, trace,
};
use crate::bars_page::{self, DragTreemapBars, drag_treemap_bars};
use crate::columns_page::ColumnsPage;
use crate::command_center_page::{self, CommandCenterPage};
use crate::drives_page::{DrivesPage, Shows};
use crate::kinds_page::KindsPage;
use crate::map_sample::{MAP_NAME, MAP_STEPS, MAP_UNREAD_STEPS, map, map_unread};
use crate::menu_page::{MenuPage, Shows as MenuShows};
use crate::mouse_page::MousePage;
use crate::operate_page::OperatePage;
pub use crate::pages::{PAGES, Page};
use crate::pane_page::PanePage;
use crate::strip_page::StripPage;
use crate::structured_sample::{
    self as structured, CODE, CODE_NAME, MARKDOWN, MARKDOWN_NAME, TABLE, TABLE_NAME, TREE,
    TREE_NAME,
};
use crate::viewer_menu_page::{Shows as ViewerMenuShows, ViewerMenuPage};
use crate::window_page::WindowPage;
use crate::window_samples::{
    hint_windows, job_element, job_windows, parts_element, parts_windows, preview_element,
    preview_windows, settings_windows, sheet_windows, window_element,
};
use crate::{
    image_sample, pdf_sample, rich_sample, sections, shortcuts_sample, tokens_page, viewer_sample,
};

mod samples;

use samples::image_keys;

const TREE_STEPS: [&str; 9] = [
    "viewer::RowDown",
    "viewer::RowDown",
    "viewer::RowDown",
    "viewer::RowDown",
    "viewer::ScrollRight",
    "viewer::RowDown",
    "viewer::RowDown",
    "viewer::ScrollRight",
    "viewer::RowDown",
];
const TABLE_STEPS: [&str; 4] = [
    "viewer::ScrollRight",
    "viewer::ScrollRight",
    "viewer_table::SortColumn",
    "viewer_table::SortColumn",
];

const PDF_STEPS: [&str; 2] = ["viewer::LeftDrawer", "viewer::RightDrawer"];
const PDF_SEARCH: &str = "undervoltage";

const MENU_PAGES: [(Page, MenuShows); 9] = [
    (Page::Menu, MenuShows::Root),
    (Page::DropMenu, MenuShows::Drop),
    (Page::MenuSubmenu, MenuShows::SevenZip),
    (Page::MenuFilter, MenuShows::Filter),
    (Page::MenuFiles, MenuShows::Files),
    (Page::MenuBackground, MenuShows::Background),
    (Page::MenuEmptyFolder, MenuShows::EmptyFolder),
    (Page::MenuLoading, MenuShows::Loading),
    (Page::MenuFailed, MenuShows::Failed),
];

pub struct Gallery {
    mode: Mode,
    page: Page,
    pane: Entity<PanePage>,
    mouse: Entity<MousePage>,
    operate: Entity<OperatePage>,
    hex: Entity<PagedView>,
    paged_text: Entity<PagedView>,
    drives: Entity<DrivesPage>,
    typed_path: Entity<DrivesPage>,
    kinds: Entity<KindsPage>,
    columns: Entity<ColumnsPage>,
    drag_treemap: DragTreemapBars,
    strip: Entity<StripPage>,
    tree: Option<Box<dyn ViewerBody>>,
    table: Option<Box<dyn ViewerBody>>,
    markdown: Option<Box<dyn ViewerBody>>,
    markdown_preview: Option<Box<dyn ViewerBody>>,
    code: Option<Box<dyn ViewerBody>>,
    memory: [Option<Box<dyn ViewerBody>>; 2],
    elf: [Option<Box<dyn ViewerBody>>; 2],
    map: [Option<Box<dyn ViewerBody>>; 3],
    trace: [Option<Box<dyn ViewerBody>>; 2],
    archive: Entity<ArchiveView>,
    font: Option<Entity<FontView>>,
    marks: Option<Entity<PagedView>>,
    image: Option<Entity<ImageView>>,
    pdf: [Entity<PdfView>; 4],
    menus: [(Page, Entity<MenuPage>); 9],
    command_centers: [(Page, Entity<CommandCenterPage>); 7],
    viewer_menus: [Entity<ViewerMenuPage>; 8],
    main_window: Entity<WindowPage>,
    shift_window: Entity<WindowPage>,
    ctrl_window: Entity<WindowPage>,
    alt_window: Entity<WindowPage>,
    job_windows: [Entity<WindowPage>; 7],
    menu_window: Entity<WindowPage>,
    sheet_windows: [Entity<WindowPage>; 9],
    settings_windows: [Entity<WindowPage>; 4],
    hint_windows: [Entity<WindowPage>; 4],
    crash_window: Entity<WindowPage>,
    parts_windows: [Entity<WindowPage>; 3],
    preview_windows: [Entity<WindowPage>; 4],
    tab: usize,
    clicks: usize,
    scroll: ListScroll,
    split: Entity<SplitterState>,
}

impl Gallery {
    pub fn new(mode: Mode, window: &mut Window, cx: &mut App) -> Self {
        shortcuts_sample::use_user_keymap(cx);
        Self {
            mode,
            page: Page::Primitives,
            pane: cx.new(PanePage::new),
            mouse: cx.new(MousePage::new),
            operate: cx.new(OperatePage::new),
            hex: viewer_sample::hex_view(cx),
            paged_text: viewer_sample::text_view(cx),
            drives: cx.new(|cx| DrivesPage::new(Shows::Menu, cx)),
            typed_path: cx.new(|cx| DrivesPage::new(Shows::PathError, cx)),
            kinds: cx.new(KindsPage::new),
            columns: cx.new(|cx| ColumnsPage::new(window, cx)),
            drag_treemap: drag_treemap_bars(window, cx),
            strip: cx.new(StripPage::new),
            tree: structured::body(TREE_NAME, TREE, window, cx),
            table: structured::body(TABLE_NAME, TABLE, window, cx),
            markdown: structured::body(MARKDOWN_NAME, MARKDOWN, window, cx),
            markdown_preview: structured::editing(MARKDOWN_NAME, MARKDOWN, window, cx),
            code: structured::body(CODE_NAME, CODE, window, cx),
            memory: [(); 2].map(|()| structured::body(MEMORY_NAME, memory(), window, cx)),
            elf: [(); 2].map(|()| structured::binary(ELF_NAME, elf(), window, cx)),
            map: samples::map_bodies(window, cx),
            trace: [(); 2].map(|()| structured::body(TRACE_NAME, trace(), window, cx)),
            archive: rich_sample::archive_view(cx),
            font: rich_sample::font_view(cx),
            marks: None,
            image: image_sample::image_view(cx).ok(),
            pdf: pdf_sample::pdf_views(PDF_SEARCH, window, cx),
            menus: MENU_PAGES.map(|(page, shows)| (page, cx.new(|cx| MenuPage::new(shows, cx)))),
            command_centers: command_center_page::PAGES
                .map(|(page, shows)| (page, cx.new(|cx| CommandCenterPage::new(shows, cx)))),
            viewer_menus: samples::viewer_menu_pages(window, cx),
            main_window: cx.new(|cx| WindowPage::new(None, cx)),
            shift_window: cx.new(|cx| WindowPage::new(Some(KeyLayer::Shift), cx)),
            ctrl_window: cx.new(|cx| WindowPage::new(Some(KeyLayer::Ctrl), cx)),
            alt_window: cx.new(|cx| WindowPage::new(Some(KeyLayer::Alt), cx)),
            job_windows: job_windows(cx),
            menu_window: cx.new(WindowPage::with_menu),
            sheet_windows: sheet_windows(window, cx),
            settings_windows: settings_windows(window, cx),
            hint_windows: hint_windows(cx),
            crash_window: cx.new(WindowPage::with_crash),
            parts_windows: parts_windows(cx),
            preview_windows: preview_windows(cx),
            tab: 1,
            clicks: 0,
            scroll: ListScroll::new(),
            split: cx.new(|_| SplitterState::new(0.4)),
        }
    }

    pub fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.mode = mode;
        tokens::apply(mode, cx);
        if let Some(image) = &self.image {
            image.update(cx, |image, cx| image.show_info(mode == Mode::Dark, cx));
        }
        cx.notify();
    }

    pub fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        cx.notify();
    }

    fn page_switch(&self, cx: &mut Context<Self>) -> impl Iterator<Item = Button> {
        let current = self.page;
        let pages: Vec<Button> = PAGES
            .iter()
            .filter(|info| info.in_switch)
            .map(|info| info.page)
            .map(|page| {
                let variant = if page == current {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Ghost
                };
                Button::new(page.info().label, page.info().label)
                    .variant(variant)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| this.set_page(page, cx)))
            })
            .collect();
        pages.into_iter()
    }

    fn header(&self, cx: &mut Context<Self>) -> FerriteTitleBar {
        let (glyph, label) = match self.mode {
            Mode::Light => (IconName::Moon, "Switch to the dark theme"),
            Mode::Dark => (IconName::Sun, "Switch to the light theme"),
        };
        let toggle = IconButton::new("theme-toggle", glyph, label).on_click(
            cx.listener(|this: &mut Self, _, _, cx| this.set_mode(this.mode.toggled(), cx)),
        );
        FerriteTitleBar::new(rusty_core::brand::NAME)
            .header(HeaderSearch::new())
            .header(toggle)
    }

    fn page_bar(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_wrap()
            .flex_none()
            .gap(px(4.))
            .px(t.spacing.xl)
            .py(t.spacing.xs)
            .border_b_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .children(self.page_switch(cx))
    }

    fn left_column(&self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let tab_listener = cx.listener(|this: &mut Self, index: &usize, _, cx| {
            this.tab = *index;
            cx.notify();
        });
        let click_listener = cx.listener(|this: &mut Self, _, _, cx| {
            this.clicks += 1;
            cx.notify();
        });
        column(t)
            .child(sections::buttons(t, self.clicks, click_listener))
            .child(sections::icon_buttons(t))
            .child(sections::keys(t))
            .child(sections::tooltips(t, window, cx))
            .child(sections::tabs(t, self.tab, tab_listener))
    }

    fn right_column(&self, t: &Theme) -> Div {
        column(t)
            .child(sections::virtual_list(t, &self.scroll))
            .child(sections::splitter(t, &self.split))
    }
}

fn column(t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xxl)
        .flex_1()
        .min_w_0()
}

impl Gallery {
    fn windows(&self) -> [&Entity<WindowPage>; 19] {
        let [
            sheet,
            sheet_left,
            sheet_narrow,
            settings,
            settings_files,
            shortcuts,
            recording,
            conflict,
            invalid,
        ] = &self.sheet_windows;
        let [hints_ctrl, hints_shift, hints_alt, hints_low] = &self.hint_windows;
        [
            &self.main_window,
            &self.shift_window,
            &self.ctrl_window,
            &self.alt_window,
            &self.menu_window,
            sheet,
            sheet_left,
            sheet_narrow,
            settings,
            settings_files,
            shortcuts,
            recording,
            conflict,
            invalid,
            hints_ctrl,
            hints_shift,
            hints_alt,
            hints_low,
            &self.crash_window,
        ]
    }
}

fn root(t: &Theme) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(t.colors.background)
        .text_color(t.colors.text)
        .font_family(t.typography.ui_font.clone())
        .text_size(t.typography.body)
        .line_height(t.typography.line_height)
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let root = root(&t);
        let [display, upgrade, find, search] = &self.settings_windows;
        match self.page {
            Page::SettingsDisplay => return root.child(display.clone()),
            Page::SettingsUpgrade => return root.child(upgrade.clone()),
            Page::SettingsFind => return root.child(find.clone()),
            Page::SearchSettings => return root.child(search.clone()),
            _ => {}
        }
        if let Some(page) = self.window_page() {
            let is_narrow = matches!(self.page, Page::WindowNarrow | Page::SheetNarrow);
            return root.when(is_narrow, |r| r.bg(t.colors.line)).child(page);
        }
        let root = root.child(self.header(cx)).child(self.page_bar(&t, cx));
        if let Some(page) = bars_page::page_of(&self.drag_treemap, self.page) {
            return root.child(page);
        }
        match self.page {
            Page::Pane => root.child(self.pane.clone()),
            Page::MouseMarks => root.child(self.mouse.clone()),
            Page::Operate => root.child(self.operate.clone()),
            Page::Hex => root.child(self.hex.clone()),
            Page::PagedText => root.child(self.paged_text.clone()),
            Page::Drives => root.child(self.drives.clone()),
            Page::TypedPath => root.child(self.typed_path.clone()),
            Page::FileKinds => root.child(self.kinds.clone()),
            Page::Columns => root.child(self.columns.clone()),
            Page::MemoryStrip => root.child(self.strip.clone()),
            Page::Archive => root.child(self.archive.clone()),
            Page::Font => root.children(self.font.clone()),
            Page::Marks => root.child(self.marks_page(window, cx)),
            Page::Image => root.children(self.image.clone()).child(image_keys()),
            _ if self.menu_page(self.page).is_some() => root.children(self.menu_page(self.page)),
            _ if self.viewer_menu().is_some() => root.children(self.viewer_menu()),
            _ if self.command_center().is_some() => root.children(self.command_center()),
            Page::Primitives => root.child(self.primitives(&t, window, cx)),
            page if page.is_viewer_sample() => self.sample_page(root, cx),
            Page::Tokens | Page::Palettes => root.child(tokens_page::render(self.page, &t)),
            Page::PdfCalm | Page::PdfSearch | Page::PdfDrawers | Page::PdfOutline => {
                root.children(self.pdf_sample().cloned())
            }
            _ => root,
        }
    }
}

impl Gallery {
    fn window_page(&self) -> Option<AnyElement> {
        parts_element(self.page, &self.parts_windows)
            .or_else(|| preview_element(self.page, &self.preview_windows))
            .or_else(|| job_element(self.page, &self.job_windows))
            .or_else(|| window_element(self.page, self.windows()))
    }

    fn viewer_menu(&self) -> Option<Entity<ViewerMenuPage>> {
        match self.page {
            Page::ViewerMenuText => Some(self.viewer_menus[0].clone()),
            Page::ViewerMenuTree => Some(self.viewer_menus[1].clone()),
            Page::ViewMenuText => Some(self.viewer_menus[2].clone()),
            Page::ViewerEncoding => Some(self.viewer_menus[3].clone()),
            Page::ViewMenuAutomotive => Some(self.viewer_menus[4].clone()),
            Page::ViewMenuElf => Some(self.viewer_menus[5].clone()),
            Page::ViewMenuTrace => Some(self.viewer_menus[6].clone()),
            Page::ViewMenuMarkdown => Some(self.viewer_menus[7].clone()),
            _ => None,
        }
    }

    fn menu_page(&self, page: Page) -> Option<Entity<MenuPage>> {
        self.menus
            .iter()
            .find(|(shown, _)| *shown == page)
            .map(|(_, menu)| menu.clone())
    }

    fn marks_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<PagedView> {
        self.marks
            .get_or_insert_with(|| viewer_sample::marks_view(window, cx))
            .clone()
    }

    fn primitives(&self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .gap(t.spacing.xxl * 2.)
            .p(t.spacing.xxl)
            .child(self.left_column(t, window, cx))
            .child(self.right_column(t))
    }
}
