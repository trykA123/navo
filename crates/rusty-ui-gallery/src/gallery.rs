use rusty_ui::prelude::*;
use rusty_ui::primitives::{IconButton, IconName, ListScroll, SplitterState, icon};
use rusty_ui::tokens::{self, Mode, Theme};

use crate::sections;

pub struct Gallery {
    mode: Mode,
    tab: usize,
    clicks: usize,
    scroll: ListScroll,
    split: Entity<SplitterState>,
}

impl Gallery {
    pub fn new(mode: Mode, cx: &mut App) -> Self {
        Self {
            mode,
            tab: 1,
            clicks: 0,
            scroll: ListScroll::new(),
            split: cx.new(|_| SplitterState::new(0.4)),
        }
    }

    pub fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.mode = mode;
        tokens::apply(mode, cx);
        cx.notify();
    }

    fn header(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let (glyph, label) = match self.mode {
            Mode::Light => (IconName::Moon, "Switch to the dark theme"),
            Mode::Dark => (IconName::Sun, "Switch to the light theme"),
        };
        let toggle = IconButton::new("theme-toggle", glyph, label).on_click(
            cx.listener(|this: &mut Self, _, _, cx| this.set_mode(this.mode.toggled(), cx)),
        );
        div()
            .flex()
            .items_center()
            .gap(t.spacing.md)
            .flex_none()
            .h(t.spacing.header_height)
            .px(t.spacing.xl)
            .border_b_1()
            .border_color(t.colors.line)
            .child(icon(IconName::Layers, t.spacing.xxl, t.colors.accent))
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::BOLD)
                    .child("Ferrite components"),
            )
            .child(toggle)
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
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.colors.background)
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .line_height(t.typography.line_height)
            .child(self.header(&t, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap(t.spacing.xxl * 2.)
                    .p(t.spacing.xxl)
                    .child(self.left_column(&t, window, cx))
                    .child(self.right_column(&t)),
            )
    }
}
