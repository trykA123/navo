use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{
    Animation, AnimationExt, FontWeight, Hsla, IntoElement, ParentElement, SharedString, Styled,
    div,
};

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

/// Muted text that fades in over the theme's fade time, or appears at once with reduced motion.
pub(super) fn quiet_note(t: &tokens::Theme, text: SharedString, still: bool) -> impl IntoElement {
    let note = div()
        .flex_shrink(1.)
        .min_w_0()
        .truncate()
        .text_color(t.colors.muted)
        .child(text);
    let enter = Animation::new(t.motion.fade).with_easing(ease_out_cubic);
    note.with_animation("status-note-fade", enter, move |note, delta| {
        note.opacity(if still { 1. } else { delta })
    })
}
