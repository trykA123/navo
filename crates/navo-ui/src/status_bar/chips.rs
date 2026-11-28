use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{
    Animation, AnimationExt, App, ClickEvent, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, Role, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::StatusNotice;
use crate::primitives::{IconName, icon};
use crate::tokens;

/// Icon size inside the clickable status-bar find chip.
const CHIP_ICON_SIZE: f32 = 12.;

/// Click callback for a status-bar chip.
pub(crate) type ChipClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Clickable status-bar chip that opens or focuses the find-results window.
pub(crate) struct FindChip {
    pub label: SharedString,
    pub running: bool,
    pub on_click: ChipClick,
}

/// Renders the clickable status-bar find chip using theme tokens.
pub(super) fn find_chip(t: &tokens::Theme, chip: FindChip) -> impl IntoElement {
    let hover = t.colors.hover;
    let fg = if chip.running {
        t.colors.text
    } else {
        t.colors.muted
    };
    let (icon_id, glyph, icon_color) = if chip.running {
        (
            "status-find-chip-running",
            IconName::LoaderCircle,
            t.colors.accent,
        )
    } else {
        ("status-find-chip-icon", IconName::Search, t.colors.muted)
    };
    let on_click = chip.on_click;
    let base = div()
        .id("status-find-chip")
        .role(Role::Button)
        .aria_label(chip.label.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .px(t.spacing.sm)
        .rounded(t.radii.sm)
        .text_size(t.typography.small)
        .text_color(fg)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(move |event, window, cx| on_click(event, window, cx))
        .child(
            div()
                .id(icon_id)
                .test_support()
                .flex()
                .flex_none()
                .items_center()
                .child(icon(glyph, px(CHIP_ICON_SIZE), icon_color)),
        )
        .child(chip.label);
    if chip.running {
        base.bg(t.colors.hover)
    } else {
        base
    }
}

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
