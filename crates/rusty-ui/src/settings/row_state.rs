use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::fields::Field;
use super::values::{Change, Values};
use super::{Handler, SheetEvent};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, icon};
use crate::tokens::Theme;

const DOT: f32 = 6.;
const DOT_LIFT: f32 = 5.;
const DEFAULT: &str = "Default";
const CHANGED: &str = "Changed";
const LIVE: &str = "Live";
const NOT_APPLIED: &str = "Not applied yet";

pub(super) fn tag(t: &Theme, index: usize, is_changed: bool) -> impl IntoElement + use<> {
    let (text, color) = if is_changed {
        (CHANGED, t.colors.accent_text)
    } else {
        (DEFAULT, t.colors.muted)
    };
    let dot = div()
        .id(("settings-dot", index))
        .test_support()
        .flex_none()
        .size(px(DOT))
        .relative()
        .top(px(-DOT_LIFT))
        .rounded(t.radii.full)
        .bg(t.colors.accent);
    div()
        .id(("settings-state", index))
        .role(Role::Note)
        .aria_label(text)
        .test_support()
        .flex()
        .flex_none()
        .items_baseline()
        .gap(t.spacing.xs)
        .text_size(t.typography.small)
        .text_color(color)
        .when(is_changed, |tag| tag.child(dot))
        .child(text)
}

pub(super) fn reset(index: usize, field: Field, send: Option<&Handler>) -> impl IntoElement {
    let send = send.cloned();
    Button::new(("settings-reset", index), "Reset")
        .variant(ButtonVariant::Ghost)
        .icon(IconName::RotateCcw)
        .tooltip("Write the default to navo.toml")
        .when_some(send, |button, send| {
            let event = SheetEvent::Set(Change::reset(field));
            button.on_click(move |_, window, cx| send(&event, window, cx))
        })
}

pub(super) fn footer(t: &Theme, values: &Values) -> impl IntoElement + use<> {
    let text = format!(
        "Applied live · Saved to navo.toml · {} changed",
        values.changed_count()
    );
    div()
        .id("settings-footer")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .px(t.spacing.xl)
        .py(t.spacing.sm)
        .border_t_1()
        .border_color(t.surfaces.card_edge)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(icon(IconName::Check, t.spacing.icon, t.colors.ok))
        .child(text)
}

pub(super) fn live_chip(t: &Theme) -> impl IntoElement + use<> {
    div()
        .id("settings-live")
        .role(Role::Note)
        .aria_label(LIVE)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .px(t.spacing.md)
        .rounded(t.radii.pill)
        .bg(t.surfaces.field)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(
            div()
                .flex_none()
                .size(px(DOT))
                .rounded(t.radii.full)
                .bg(t.colors.ok),
        )
        .child(LIVE)
}

pub(super) fn notes(
    t: &Theme,
    field: Field,
    key_taken: bool,
) -> impl Iterator<Item = AnyElement> + use<> {
    let pending = (!field.is_applied()).then(|| {
        note(
            t,
            "settings-not-applied",
            NOT_APPLIED.into(),
            t.colors.muted,
        )
    });
    let taken = (field == Field::SummonKey && key_taken).then(|| {
        let text = rusty_l10n::tr("settings-summon-key-taken").into();
        note(t, "settings-summon-taken", text, t.colors.mark)
    });
    [pending, taken].into_iter().flatten()
}

fn note(t: &Theme, id: &'static str, text: SharedString, color: Hsla) -> AnyElement {
    div()
        .id(id)
        .role(Role::Note)
        .aria_label(text.clone())
        .test_support()
        .text_size(t.typography.small)
        .text_color(color)
        .child(text)
        .into_any_element()
}
