use rusty_ui::fkey_bar::{FKeyBar, KeyLayer};
use rusty_ui::prelude::*;
use rusty_ui::primitives::{
    Button, ButtonVariant, IconButton, IconName, ListScroll, SplitterState,
};
use rusty_ui::tokens::{self, Mode, Theme};
use rusty_ui::window_view::{ArchiveView, BodyKeys, FontView, ImageView, PagedView, ViewerBody};
use rusty_ui::{FerriteTitleBar, HeaderSearch};

use crate::automotive_sample::{MEMORY_NAME, MEMORY_STEPS, memory};
use crate::drives_page::{DrivesPage, Shows};
use crate::image_sample;
use crate::menu_page::{MenuPage, Shows as MenuShows};
use crate::operate_page::OperatePage;
pub use crate::pages::{PAGES, Page};
use crate::pane_page::PanePage;
use crate::sections;
use crate::shortcuts_sample;
use crate::structured_sample::{
    self as structured, MARKDOWN, MARKDOWN_NAME, TABLE, TABLE_NAME, TREE, TREE_NAME,
};
use crate::tokens_page;
use crate::viewer_menu_page::{Shows as ViewerMenuShows, ViewerMenuPage};
use crate::window_page::WindowPage;
use crate::window_samples::{hint_windows, job_windows, sheet_windows, window_element};
use crate::{rich_sample, viewer_sample};

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

const MENU_PAGES: [(Page, MenuShows); 4] = [
    (Page::Menu, MenuShows::Root),
    (Page::MenuSubmenu, MenuShows::SevenZip),
    (Page::MenuFilter, MenuShows::Filter),
    (Page::MenuFiles, MenuShows::Files),
];

pub struct Gallery {
    mode: Mode,
    page: Page,
    pane: Entity<PanePage>,
    operate: Entity<OperatePage>,
    hex: Entity<PagedView>,
    paged_text: Entity<PagedView>,
    drives: Entity<DrivesPage>,
    typed_path: Entity<DrivesPage>,
    tree: Option<Box<dyn ViewerBody>>,
    table: Option<Box<dyn ViewerBody>>,
    markdown: Option<Box<dyn ViewerBody>>,
    markdown_preview: Option<Box<dyn ViewerBody>>,
    memory: [Option<Box<dyn ViewerBody>>; 2],
    archive: Entity<ArchiveView>,
    font: Option<Entity<FontView>>,
    marks: Option<Entity<PagedView>>,
    image: Option<Entity<ImageView>>,
    menus: [(Page, Entity<MenuPage>); 4],
    viewer_menus: [Entity<ViewerMenuPage>; 2],
    main_window: Entity<WindowPage>,
    shift_window: Entity<WindowPage>,
    ctrl_window: Entity<WindowPage>,
    alt_window: Entity<WindowPage>,
    job_windows: [Entity<WindowPage>; 4],
    menu_window: Entity<WindowPage>,
    sheet_windows: [Entity<WindowPage>; 9],
    hint_windows: [Entity<WindowPage>; 4],
    crash_window: Entity<WindowPage>,
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
            operate: cx.new(OperatePage::new),
            hex: viewer_sample::hex_view(cx),
            paged_text: viewer_sample::text_view(cx),
            drives: cx.new(|cx| DrivesPage::new(Shows::Menu, cx)),
            typed_path: cx.new(|cx| DrivesPage::new(Shows::PathError, cx)),
            tree: structured::body(TREE_NAME, TREE, window, cx),
            table: structured::body(TABLE_NAME, TABLE, window, cx),
            markdown: structured::body(MARKDOWN_NAME, MARKDOWN, window, cx),
            markdown_preview: structured::editing(MARKDOWN_NAME, MARKDOWN, window, cx),
            memory: [(); 2].map(|()| structured::body(MEMORY_NAME, memory(), window, cx)),
            archive: rich_sample::archive_view(cx),
            font: rich_sample::font_view(cx),
            marks: None,
            image: image_sample::image_view(cx).ok(),
            menus: MENU_PAGES.map(|(page, shows)| (page, cx.new(|cx| MenuPage::new(shows, cx)))),
            viewer_menus: [ViewerMenuShows::Text, ViewerMenuShows::Tree]
                .map(|shows| cx.new(|cx| ViewerMenuPage::new(shows, window, cx))),
            main_window: cx.new(|cx| WindowPage::new(None, cx)),
            shift_window: cx.new(|cx| WindowPage::new(Some(KeyLayer::Shift), cx)),
            ctrl_window: cx.new(|cx| WindowPage::new(Some(KeyLayer::Ctrl), cx)),
            alt_window: cx.new(|cx| WindowPage::new(Some(KeyLayer::Alt), cx)),
            job_windows: job_windows(cx),
            menu_window: cx.new(WindowPage::with_menu),
            sheet_windows: sheet_windows(window, cx),
            hint_windows: hint_windows(cx),
            crash_window: cx.new(WindowPage::with_crash),
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

    fn sample(&self) -> Option<&dyn ViewerBody> {
        match self.page {
            Page::Tree => self.tree.as_deref(),
            Page::Table => self.table.as_deref(),
            Page::Markdown => self.markdown.as_deref(),
            Page::MarkdownPreview => self.markdown_preview.as_deref(),
            Page::MemoryImage => self.memory[0].as_deref(),
            Page::MemoryDrawers => self.memory[1].as_deref(),
            _ => None,
        }
    }

    pub fn focus_sample(&self, window: &mut Window, cx: &mut App) {
        if let Some(body) = self.sample() {
            window.focus(&body.focus_handle(cx), cx);
        }
    }

    pub fn drive_sample(&self, window: &mut Window, cx: &mut App) {
        let steps: &[&str] = match self.page {
            Page::Tree => &TREE_STEPS,
            Page::Table => &TABLE_STEPS,
            Page::MemoryDrawers => &MEMORY_STEPS,
            _ => &[],
        };
        for name in steps {
            if let Ok(action) = cx.build_action(name, None) {
                window.dispatch_action(action, cx);
            }
        }
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
        FerriteTitleBar::new("Ferrite")
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

fn image_keys() -> FKeyBar {
    let kinds = [("Alt+1", "Text"), ("3", "Hex"), ("Esc", "Close")];
    FKeyBar::new("image-keys", ImageView::KEYS.into_iter().chain(kinds))
}

fn sample_keys(body: &dyn ViewerBody, cx: &App) -> FKeyBar {
    let (BodyKeys::Modal(keys) | BodyKeys::Normal(keys)) = body.keys(cx);
    let viewer = [
        ("1", "Text"),
        ("3", "Hex"),
        ("N", "Next file"),
        ("P", "Previous file"),
        ("Esc", "Close"),
    ];
    FKeyBar::new("sample-keys", keys.into_iter().chain(viewer))
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
    fn windows(&self) -> [&Entity<WindowPage>; 23] {
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
        let [job, job_pill, two, two_pill] = &self.job_windows;
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
            job,
            job_pill,
            two,
            two_pill,
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
        if let Some(page) = window_element(self.page, self.windows()) {
            let is_narrow = matches!(self.page, Page::WindowNarrow | Page::SheetNarrow);
            return root.when(is_narrow, |r| r.bg(t.colors.line)).child(page);
        }
        let root = root.child(self.header(cx)).child(self.page_bar(&t, cx));
        match self.page {
            Page::Pane => root.child(self.pane.clone()),
            Page::Operate => root.child(self.operate.clone()),
            Page::Hex => root.child(self.hex.clone()),
            Page::PagedText => root.child(self.paged_text.clone()),
            Page::Drives => root.child(self.drives.clone()),
            Page::TypedPath => root.child(self.typed_path.clone()),
            Page::Archive => root.child(self.archive.clone()),
            Page::Font => root.children(self.font.clone()),
            Page::Marks => root.child(self.marks_page(window, cx)),
            Page::Image => root.children(self.image.clone()).child(image_keys()),
            Page::Menu | Page::MenuSubmenu | Page::MenuFilter | Page::MenuFiles => {
                root.children(self.menu_page(self.page))
            }
            Page::ViewerMenuText | Page::ViewerMenuTree => root.children(self.viewer_menu()),
            Page::Primitives => root.child(self.primitives(&t, window, cx)),
            Page::Tree
            | Page::Table
            | Page::Markdown
            | Page::MarkdownPreview
            | Page::MemoryImage
            | Page::MemoryDrawers => {
                let has_keys = matches!(
                    self.page,
                    Page::Tree | Page::MemoryImage | Page::MemoryDrawers
                );
                match self.sample() {
                    Some(body) => root
                        .child(body.view())
                        .when(has_keys, |root| root.child(sample_keys(body, cx))),
                    None => root.child("This sample did not load."),
                }
            }
            Page::Tokens => root.child(tokens_page::render(&t)),
            _ => root,
        }
    }
}

impl Gallery {
    fn viewer_menu(&self) -> Option<Entity<ViewerMenuPage>> {
        match self.page {
            Page::ViewerMenuText => Some(self.viewer_menus[0].clone()),
            Page::ViewerMenuTree => Some(self.viewer_menus[1].clone()),
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
