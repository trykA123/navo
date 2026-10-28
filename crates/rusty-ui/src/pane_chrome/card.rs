use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, BoxShadow, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled, div, px,
};

use crate::tokens::{self, Theme};

const HEADER_HEIGHT: f32 = 42.;

#[must_use]
pub fn card_shadow(t: &Theme, is_active: bool) -> Vec<BoxShadow> {
    if is_active {
        t.shadows.card_active.clone()
    } else {
        t.shadows.card.clone()
    }
}

pub fn pane_card(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    is_active: bool,
    cx: &App,
) -> ObservedElement<Stateful<Div>> {
    let t = tokens::current(cx);
    div()
        .id(id)
        .role(Role::Group)
        .aria_label(label.into())
        .aria_selected(is_active)
        .test_support()
        .flex()
        .flex_col()
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .rounded(t.radii.card)
        .bg(t.surfaces.card)
        .shadow(card_shadow(t, is_active))
}

#[must_use]
pub fn card_header(t: &Theme) -> Div {
    div()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(HEADER_HEIGHT))
        .px(t.spacing.md)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaneMarks {
    pub number: Option<usize>,
    pub is_filled: bool,
    pub is_target: bool,
}

impl PaneMarks {
    #[must_use]
    pub fn new(shown: usize, number: usize, is_active: bool, is_target: bool) -> Self {
        let is_tiled = shown >= 3;
        Self {
            number: is_tiled.then_some(number),
            is_filled: is_tiled && is_active,
            is_target: shown >= 2 && is_target,
        }
    }

    #[must_use]
    pub fn has_target_tag(self) -> bool {
        self.is_target && self.number.is_some()
    }
}

#[must_use]
pub fn on_accent(t: &Theme) -> Theme {
    let mut theme = t.clone();
    let ink = t.colors.background;
    let c = &mut theme.colors;
    c.text = ink;
    c.muted = ink;
    c.accent = ink;
    c.accent_text = ink;
    c.hover = ink.opacity(0.2);
    theme
}

#[must_use]
pub fn marked_header(t: &Theme, id: u64, marks: PaneMarks) -> Div {
    let header = card_header(t).when(marks.is_filled, |header| {
        header.bg(t.colors.accent).text_color(t.colors.background)
    });
    match marks.number {
        Some(number) => header.child(number_badge(t, id, number, marks.is_filled)),
        None => header,
    }
}

fn marker_box(t: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(t.spacing.icon + t.spacing.xxs)
        .rounded(t.radii.sm)
        .text_size(t.typography.tiny)
        .line_height(t.spacing.icon)
        .font_weight(FontWeight::SEMIBOLD)
}

fn number_badge(t: &Theme, id: u64, number: usize, is_filled: bool) -> impl IntoElement {
    let (ink, edge) = match is_filled {
        true => (t.colors.background, t.colors.background),
        false => (t.colors.muted, t.colors.line),
    };
    marker_box(t)
        .id(("pane-number", id))
        .test_support()
        .w(t.spacing.icon + t.spacing.xxs)
        .border_1()
        .border_color(edge)
        .when(!is_filled, |badge| badge.bg(t.colors.rail))
        .font_family(t.typography.numbers_font.clone())
        .text_color(ink)
        .child(number.to_string())
}

#[must_use]
pub fn marker_tag(t: &Theme, id: u64, marks: PaneMarks) -> Option<AnyElement> {
    let tag = if marks.is_filled {
        marker_box(t)
            .id(("pane-tag-active", id))
            .bg(t.colors.background)
            .text_color(t.colors.accent_text)
            .child(rusty_l10n::tr("pane-strip-active"))
    } else if marks.has_target_tag() {
        marker_box(t)
            .id(("pane-tag-target", id))
            .border_1()
            .border_dashed()
            .border_color(t.colors.faint)
            .text_color(t.colors.text)
            .child(rusty_l10n::tr("pane-target-here"))
    } else {
        return None;
    };
    Some(tag.test_support().px(t.spacing.sm).into_any_element())
}

#[must_use]
pub fn target_outline(t: &Theme, id: u64) -> impl IntoElement {
    dashed_outline(t)
        .id(("pane-target-outline", id))
        .test_support()
}

pub(super) fn dashed_outline(t: &Theme) -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded(t.radii.card)
        .border_1()
        .border_dashed()
        .border_color(t.colors.faint)
}
