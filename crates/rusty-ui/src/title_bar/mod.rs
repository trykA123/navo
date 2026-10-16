mod brand;
mod search;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::TitleBar;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled, TitlebarOptions, Window, WindowOptions, div, transparent_black,
};

use crate::tokens::{self, Theme};

pub use search::HeaderSearch;

type CloseHandler = Rc<dyn Fn(&mut Window, &mut App)>;

pub fn window_options(title: impl Into<SharedString>) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            ..TitleBar::title_bar_options()
        }),
        ..TitleBar::window_options()
    }
}

#[derive(IntoElement)]
pub struct FerriteTitleBar {
    title: SharedString,
    header: Vec<AnyElement>,
    is_on_mica: bool,
    on_close: Option<CloseHandler>,
}

impl FerriteTitleBar {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            header: Vec::new(),
            is_on_mica: false,
            on_close: None,
        }
    }

    pub fn header(mut self, content: impl IntoElement) -> Self {
        self.header.push(content.into_any_element());
        self
    }

    #[must_use]
    pub fn on_mica(mut self) -> Self {
        self.is_on_mica = true;
        self
    }

    pub fn on_close(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }
}

fn header_area(t: &Theme, header: Vec<AnyElement>) -> impl IntoElement {
    div()
        .id("title-bar-header")
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .min_w_0()
        .flex_shrink(1.)
        .mr(t.spacing.md)
        .occlude()
        .test_support()
        .children(header)
}

impl RenderOnce for FerriteTitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        TitleBar::new()
            .h(t.spacing.header_height)
            .pl(t.spacing.xl)
            .bg(t.colors.background)
            .border_color(t.colors.line)
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .when(self.is_on_mica, |bar| {
                bar.bg(transparent_black())
                    .border_color(transparent_black())
            })
            .child(brand::brand(t, self.title, window.scale_factor()))
            .when(!self.header.is_empty(), |bar| {
                bar.child(header_area(t, self.header))
            })
            .when_some(self.on_close, |bar, handler| {
                bar.on_close_window(move |_, window, cx| {
                    handler(window, cx);
                    window.remove_window();
                })
            })
    }
}
