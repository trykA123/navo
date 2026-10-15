use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use crate::format;
use crate::prelude::*;
use crate::tokens::Theme;

pub(super) struct HeaderParts {
    pub title: SharedString,
    pub mode: SharedString,
    pub len: u64,
    pub detail: Option<(&'static str, SharedString)>,
    pub status: Option<(&'static str, SharedString)>,
}

fn small(t: &Theme) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
}

pub(super) fn header(t: &Theme, parts: HeaderParts) -> Div {
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
                .aria_label(parts.title.clone())
                .test_support()
                .flex_none()
                .max_w(t.spacing.rail_width * 2.)
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(parts.title),
        )
        .child(small(t).child(parts.mode))
        .child(small(t).child(format::size(parts.len)))
        .when_some(parts.detail, |row, (id, detail)| {
            row.child(
                div()
                    .id(id)
                    .aria_label(detail.clone())
                    .test_support()
                    .min_w_0()
                    .truncate()
                    .font_family(t.typography.mono_font.clone())
                    .text_size(t.typography.small)
                    .text_color(t.colors.accent_text)
                    .child(detail),
            )
        })
        .child(div().flex_1())
        .when_some(parts.status, |row, (id, status)| {
            row.child(
                small(t)
                    .id(id)
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .test_support()
                    .child(status),
            )
        })
}
