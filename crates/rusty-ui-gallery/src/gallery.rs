use rusty_ui::prelude::*;
use rusty_ui::primitives::{
    Button, ButtonVariant, IconButton, IconName, ListScroll, SplitterState,
};
use rusty_ui::tokens::{self, Mode, Theme};
use rusty_ui::window_view::{ImageView, PagedView, ViewerBody};
use rusty_ui::{FerriteTitleBar, HeaderSearch};

use crate::drives_page::{DrivesPage, Shows};
use crate::image_sample;
use crate::operate_page::OperatePage;
use crate::pane_page::PanePage;
use crate::sections;
use crate::structured_sample::{
    self as structured, MARKDOWN, MARKDOWN_NAME, TABLE, TABLE_NAME, TREE, TREE_NAME,
};
use crate::viewer_sample;

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
    Image,
}

impl Page {
    pub const ALL: [Self; 11] = [
        Self::Primitives,
        Self::Pane,
        Self::Operate,
        Self::Hex,
        Self::PagedText,
        Self::Drives,
        Self::TypedPath,
        Self::Tree,
        Self::Table,
        Self::Markdown,
        Self::Image,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Primitives => "Parts",
            Self::Pane => "Pane",
            Self::Operate => "Operate",
            Self::Hex => "Hex",
            Self::PagedText => "Text",
            Self::Drives => "Drives",
            Self::TypedPath => "Path",
            Self::Tree => "Tree",
            Self::Table => "Table",
            Self::Markdown => "Markdown",
            Self::Image => "Image",
        }
    }
}

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
    image: Option<Entity<ImageView>>,
    tab: usize,
    clicks: usize,
    scroll: ListScroll,
    split: Entity<SplitterState>,
}

impl Gallery {
    pub fn new(mode: Mode, window: &mut Window, cx: &mut App) -> Self {
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
            image: image_sample::image_view(cx).ok(),
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
        let pages: Vec<Button> = Page::ALL
            .into_iter()
            .map(|page| {
                let variant = if page == current {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Ghost
                };
                Button::new(page.label(), page.label())
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

fn column(t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xxl)
        .flex_1()
        .min_w_0()
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let root = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.colors.background)
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .line_height(t.typography.line_height)
            .child(self.header(cx))
            .child(self.page_bar(&t, cx));
        match self.page {
            Page::Pane => root.child(self.pane.clone()),
            Page::Operate => root.child(self.operate.clone()),
            Page::Hex => root.child(self.hex.clone()),
            Page::PagedText => root.child(self.paged_text.clone()),
            Page::Drives => root.child(self.drives.clone()),
            Page::TypedPath => root.child(self.typed_path.clone()),
            Page::Image => root.children(self.image.clone()),
            Page::Primitives => root.child(self.primitives(&t, window, cx)),
            Page::Tree | Page::Table | Page::Markdown => match self.sample() {
                Some(body) => root.child(body.view()),
                None => root.child("This sample did not load."),
            },
        }
    }
}

impl Gallery {
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
