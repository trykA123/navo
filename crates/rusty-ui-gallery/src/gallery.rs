use rusty_ui::fkey_bar::FKeyBar;
use rusty_ui::prelude::*;
use rusty_ui::primitives::{
    Button, ButtonVariant, IconButton, IconName, ListScroll, SplitterState,
};
use rusty_ui::tokens::{self, Mode, Theme};
use rusty_ui::window_view::{ArchiveView, FontView, ImageView, PagedView};
use rusty_ui::{FerriteTitleBar, HeaderSearch};

use crate::drives_page::{DrivesPage, Shows};
use crate::image_sample;
use crate::operate_page::OperatePage;
use crate::pane_page::PanePage;
use crate::sections;
use crate::{rich_sample, viewer_sample};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Primitives,
    Pane,
    Operate,
    Hex,
    PagedText,
    Drives,
    TypedPath,
    Archive,
    Font,
    Marks,
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
        Self::Archive,
        Self::Font,
        Self::Marks,
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
            Self::Archive => "Zip",
            Self::Font => "Font",
            Self::Marks => "Marks",
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
    archive: Entity<ArchiveView>,
    font: Option<Entity<FontView>>,
    marks: Option<Entity<PagedView>>,
    image: Option<Entity<ImageView>>,
    tab: usize,
    clicks: usize,
    scroll: ListScroll,
    split: Entity<SplitterState>,
}

impl Gallery {
    pub fn new(mode: Mode, cx: &mut App) -> Self {
        Self {
            mode,
            page: Page::Primitives,
            pane: cx.new(PanePage::new),
            operate: cx.new(OperatePage::new),
            hex: viewer_sample::hex_view(cx),
            paged_text: viewer_sample::text_view(cx),
            drives: cx.new(|cx| DrivesPage::new(Shows::Menu, cx)),
            typed_path: cx.new(|cx| DrivesPage::new(Shows::PathError, cx)),
            archive: rich_sample::archive_view(cx),
            font: rich_sample::font_view(cx),
            marks: None,
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

    fn page_switch(&self, cx: &mut Context<Self>) -> impl Iterator<Item = Button> {
        let current = self.page;
        let pages: Vec<Button> = Page::ALL
            .into_iter()
            .filter(|page| !matches!(page, Page::Marks | Page::Archive | Page::Font))
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
            .header(div().flex().gap(px(4.)).children(self.page_switch(cx)))
            .header(toggle)
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
    FKeyBar::new("image-keys", ImageView::KEYS.into_iter().chain(kinds)).sized_to_keys()
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
            .child(self.header(cx));
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
            Page::Primitives => root.child(self.primitives(&t, window, cx)),
        }
    }
}

impl Gallery {
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
