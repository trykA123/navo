use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    App, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::primitives::{IconName, Kbd, icon};
use crate::tokens;

const WIDTH: f32 = 520.;
const HEIGHT: f32 = 32.;

#[derive(IntoElement)]
pub struct HeaderSearch {
    placeholder: SharedString,
    keys: &'static str,
}

impl Default for HeaderSearch {
    fn default() -> Self {
        Self::new()
    }
}

impl HeaderSearch {
    pub fn new() -> Self {
        Self {
            placeholder: "Find anywhere: files, folders, commands".into(),
            keys: "ctrl-k",
        }
    }
}

impl RenderOnce for HeaderSearch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        div()
            .id("title-bar-search")
            .aria_label(self.placeholder.clone())
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.lg)
            .w(px(WIDTH))
            .min_w_0()
            .flex_shrink(1.)
            .h(px(HEIGHT))
            .pl(t.spacing.xl)
            .pr(t.spacing.sm)
            .border_1()
            .border_color(t.surfaces.card_edge)
            .rounded(px(HEIGHT / 2.))
            .bg(t.surfaces.card)
            .text_color(t.colors.muted)
            .child(icon(IconName::Search, t.spacing.icon, t.colors.muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.placeholder),
            )
            .children(Kbd::new("title-bar-search-key", self.keys))
    }
}
