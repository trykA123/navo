use gpui_kit::base::TestSupportExt;
use gpui_kit::{Role, Stateful};

use super::{FontView, LINE_STEP};
use crate::format;
use crate::prelude::*;
use crate::tokens::{self, Theme};
use crate::window_view::viewer::{GoEnd, GoStart, PageDown, PageUp, RowDown, RowUp};

pub const SAMPLE: &str = "The quick brown fox jumps over the lazy dog. 0123456789";
pub const SIZES: [f32; 6] = [12., 16., 24., 36., 48., 72.];
const GLYPH_CELL: f32 = 56.;
const GLYPH_SIZE: f32 = 26.;
const LINE_RATIO: f32 = 1.3;

fn small(t: &Theme) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
}

fn heading(t: &Theme, text: &'static str) -> Div {
    div()
        .text_size(t.typography.small)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.faint)
        .child(text)
}

impl FontView {
    fn styled(&self, element: Div, family: &SharedString) -> Div {
        element
            .font_family(family.clone())
            .font_weight(FontWeight(f32::from(self.face.weight)))
            .when(self.face.is_italic, Styled::italic)
    }

    fn header(&self, t: &Theme) -> Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .px(t.spacing.xl)
            .h(t.spacing.path_height)
            .border_b_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .child(
                div()
                    .id("viewer-title")
                    .test_support()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .child(small(t).child(self.face.format.label()))
            .children(
                self.face
                    .version
                    .clone()
                    .map(|version| small(t).child(version)),
            )
            .child(div().flex_1())
            .children(self.note.clone().map(|note| {
                small(t)
                    .id("font-note")
                    .role(Role::Status)
                    .aria_label(note.clone())
                    .test_support()
                    .child(note)
            }))
    }

    fn facts(&self, t: &Theme) -> Div {
        let glyphs = format::count(u64::from(self.face.glyphs), "glyph", "glyphs");
        div()
            .flex()
            .items_baseline()
            .gap(t.spacing.lg)
            .child(
                div()
                    .id("font-family")
                    .aria_label(SharedString::from(self.face.family.clone()))
                    .test_support()
                    .text_size(px(28.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.colors.text)
                    .child(self.face.family.clone()),
            )
            .child(small(t).child(format!("{} · {glyphs}", self.face.style)))
    }

    fn samples(&self, t: &Theme, family: &SharedString) -> Div {
        let lines = SIZES.iter().enumerate().map(|(index, size)| {
            let sample = self
                .styled(div(), family)
                .text_size(px(*size))
                .line_height(px(size * LINE_RATIO))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_color(t.colors.text)
                .child(SAMPLE);
            div()
                .flex()
                .items_center()
                .gap(t.spacing.lg)
                .child(small(t).w(px(48.)).child(format!("{size} px")))
                .child(
                    div()
                        .id(("font-sample", index))
                        .aria_label(SAMPLE)
                        .test_support()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .child(sample),
                )
        });
        div().flex().flex_col().gap(t.spacing.md).children(lines)
    }

    fn grid(&self, t: &Theme, family: &SharedString) -> impl IntoElement {
        let cells = self.face.chars.iter().map(|ch| {
            self.styled(div(), family)
                .flex()
                .items_center()
                .justify_center()
                .size(px(GLYPH_CELL))
                .rounded(t.radii.sm)
                .border_1()
                .border_color(t.colors.line)
                .bg(t.colors.background)
                .text_size(px(GLYPH_SIZE))
                .text_color(t.colors.text)
                .child(ch.to_string())
        });
        let shown = format::count(self.face.chars.len() as u64, "character", "characters");
        div()
            .id("font-glyphs")
            .aria_label(SharedString::from(shown))
            .test_support()
            .flex()
            .flex_wrap()
            .gap(t.spacing.xs)
            .children(cells)
    }

    fn moves(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let step = px(LINE_STEP);
        root.on_action(cx.listener(move |view, _: &RowUp, _, cx| view.scroll_by(-step, cx)))
            .on_action(cx.listener(move |view, _: &RowDown, _, cx| view.scroll_by(step, cx)))
            .on_action(cx.listener(|view, _: &PageUp, _, cx| view.scroll_by(-view.page(), cx)))
            .on_action(cx.listener(|view, _: &PageDown, _, cx| view.scroll_by(view.page(), cx)))
            .on_action(cx.listener(|view, _: &GoStart, _, cx| {
                view.scroll_by(px(-f32::MAX), cx);
            }))
            .on_action(cx.listener(|view, _: &GoEnd, _, cx| view.scroll_by(px(f32::MAX), cx)))
    }
}

impl Render for FontView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let body = div()
            .id("font-body")
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .p(t.spacing.xxl)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.xxl)
                    .child(self.facts(&t))
                    .when_some(self.family.as_ref(), |d, family| {
                        d.child(heading(&t, "Sample"))
                            .child(self.samples(&t, family))
                            .child(heading(&t, "Characters"))
                            .child(self.grid(&t, family))
                    }),
            );
        let root = div().id("font").track_focus(&self.focus);
        Self::moves(root, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .child(self.header(&t))
            .child(body)
    }
}
