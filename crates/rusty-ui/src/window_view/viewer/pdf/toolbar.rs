use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::PdfView;
use super::layout::Zoom;
use crate::format;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconButton, IconName, TextField, icon};
use crate::side_sheet::Side;
use crate::tokens::Theme;
use crate::window_view::viewer::search::{FIND_CONTEXT, GO_TO_CONTEXT};

const PAGE_BOX: f32 = 56.;
const FIND_BOX: f32 = 220.;

fn small(t: &Theme) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
}

fn toggle(id: &'static str, label: &'static str, is_on: bool, tooltip: &'static str) -> Button {
    let variant = if is_on {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Ghost
    };
    Button::new(id, label).variant(variant).tooltip(tooltip)
}

impl PdfView {
    fn page_group(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let count = self.doc().map_or(0, super::doc::Doc::count);
        let is_open = self.is_open(Side::Left, cx);
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .child(
                toggle(
                    "pdf-pages-button",
                    "Pages",
                    is_open,
                    "Pages and outline (Ctrl+B)",
                )
                .icon(IconName::PanelLeft)
                .on_click(cx.listener(|view, _, window, cx| view.toggle_left(window, cx))),
            )
            .child(small(t).child("Page"))
            .child(
                div()
                    .id("pdf-page-box")
                    .key_context(GO_TO_CONTEXT)
                    .w(px(PAGE_BOX))
                    .child(TextField::new("pdf-page-input", &self.inputs.page, "Page")),
            )
            .child(small(t).child(format!("of {}", format::bytes(u64::from(count)))))
    }

    fn find_group(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.count_label();
        div()
            .id("pdf-find")
            .key_context(FIND_CONTEXT)
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .pl(t.spacing.md)
            .pr(t.spacing.xs)
            .rounded(t.radii.md)
            .border_1()
            .border_color(if self.is_finding {
                t.colors.accent
            } else {
                t.colors.line
            })
            .bg(t.surfaces.field)
            .child(icon(IconName::Search, t.spacing.icon, t.colors.muted))
            .child(small(t).child("Find"))
            .child(div().w(px(FIND_BOX)).child(TextField::new(
                "pdf-find-input",
                &self.inputs.find,
                "Find",
            )))
            .children(count.map(|count| {
                small(t)
                    .id("pdf-find-count")
                    .role(Role::Status)
                    .aria_label(count.clone())
                    .test_support()
                    .font_family(t.typography.numbers_font.clone())
                    .child(count)
            }))
            .child(self.find_buttons(t, cx))
    }

    fn find_buttons(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let hits_open = self.is_open(Side::Right, cx);
        div()
            .flex()
            .items_center()
            .gap(t.spacing.xs)
            .child(
                toggle(
                    "pdf-match-case",
                    "Aa",
                    self.query.match_case,
                    "Match case (Alt+C)",
                )
                .on_click(cx.listener(|view, _, _, cx| view.toggle_case(cx))),
            )
            .child(
                toggle(
                    "pdf-whole-word",
                    "ab",
                    self.query.whole_word,
                    "Whole word (Alt+W)",
                )
                .on_click(cx.listener(|view, _, _, cx| view.toggle_word(cx))),
            )
            .child(
                IconButton::new(
                    "pdf-previous-hit",
                    IconName::ChevronUp,
                    "Previous hit (Shift+Enter)",
                )
                .on_click(cx.listener(|view, _, window, cx| view.step(true, window, cx))),
            )
            .child(
                IconButton::new("pdf-next-hit", IconName::ChevronDown, "Next hit (Enter)")
                    .on_click(cx.listener(|view, _, window, cx| view.step(false, window, cx))),
            )
            .child(
                IconButton::new("pdf-all-hits", IconName::PanelRight, "All hits (Ctrl+I)")
                    .selected(hits_open)
                    .on_click(cx.listener(|view, _, window, cx| view.toggle_hits(window, cx))),
            )
    }

    fn zoom_group(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let (percent, is_fit) = self.doc().map_or((100, true), |doc| {
            (doc.camera.percent(&doc.pages), doc.camera.zoom == Zoom::Fit)
        });
        let label: SharedString = format!("{percent} %").into();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xs)
            .child(
                IconButton::new("pdf-zoom-out", IconName::Minus, "Zoom out (Ctrl+−)")
                    .on_click(cx.listener(|view, _, _, cx| view.zoom_step(false, None, cx))),
            )
            .child(
                small(t)
                    .id("pdf-zoom")
                    .role(Role::Status)
                    .aria_label(label.clone())
                    .test_support()
                    .font_family(t.typography.numbers_font.clone())
                    .child(label),
            )
            .child(
                IconButton::new("pdf-zoom-in", IconName::Plus, "Zoom in (Ctrl+=)")
                    .on_click(cx.listener(|view, _, _, cx| view.zoom_step(true, None, cx))),
            )
            .child(
                toggle("pdf-fit", "Fit", is_fit, "Fit to width (Ctrl+0)")
                    .on_click(cx.listener(|view, _, _, cx| view.set_zoom(Zoom::Fit, cx))),
            )
            .child(
                Button::new("pdf-actual", "100 %")
                    .variant(ButtonVariant::Ghost)
                    .tooltip("Actual size (Ctrl+1)")
                    .on_click(cx.listener(|view, _, _, cx| view.actual_size(cx))),
            )
    }

    pub(super) fn tool_row(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.lg)
            .px(t.spacing.xl)
            .py(t.spacing.sm)
            .border_b_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .child(self.page_group(t, cx))
            .child(self.find_group(t, cx))
            .child(div().flex_1())
            .child(self.zoom_group(t, cx))
    }
}
