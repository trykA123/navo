use gpui_kit::{FontWeight, Hsla, IntoElement, ParentElement, Styled, div};

use super::StatusNotice;
use crate::tokens;

pub(super) fn marked_part(marked: String, more: bool) -> String {
    if more { format!("{marked} ·") } else { marked }
}

pub(super) fn marked_chip(color: Hsla, marked: String, more: bool) -> impl IntoElement {
    div()
        .flex_none()
        .text_color(color)
        .font_weight(FontWeight::SEMIBOLD)
        .child(marked_part(marked, more))
}

pub(super) fn notice_text(t: &tokens::Theme, notice: StatusNotice) -> impl IntoElement {
    let color = if notice.is_error {
        t.colors.mark
    } else {
        t.colors.warn
    };
    div()
        .flex_shrink(1.)
        .min_w_0()
        .truncate()
        .text_color(color)
        .child(notice.text)
}
