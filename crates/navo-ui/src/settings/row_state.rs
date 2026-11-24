//! Row state of the Settings sheet: the changed dot, Reset, the footer, the Live chip and
//! the notes for refused keys.

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::fields::Field;
use super::values::{Change, Values};
use super::{Handler, SheetEvent};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, icon};
use crate::tokens::Theme;

const DOT: f32 = 6.;
/// Width of the left gutter that holds a row's changed dot (px).
pub(super) const GUTTER: f32 = 14.;
const DEFAULT: &str = "Default";
const CHANGED: &str = "Changed";
const LIVE: &str = "Live";

/// The accent dot that marks a changed setting.
pub(super) fn dot(t: &Theme) -> Div {
    div()
        .flex_none()
        .size(px(DOT))
        .rounded(t.radii.full)
        .bg(t.colors.accent)
}

/// The row's left gutter: the changed dot, or nothing; it names the state for assistive tech.
pub(super) fn gutter(t: &Theme, index: usize, is_changed: bool) -> impl IntoElement + use<> {
    let text = if is_changed { CHANGED } else { DEFAULT };
    div()
        .id(("settings-state", index))
        .role(Role::Note)
        .aria_label(text)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(GUTTER))
        .when(is_changed, |gutter| {
            gutter.child(dot(t).id(("settings-dot", index)).test_support())
        })
}

/// Reset to default for `field`; disabled while the value is already the default. It sends
/// the same reset change as F8.
pub(super) fn reset(
    index: usize,
    field: Field,
    send: Option<&Handler>,
    is_changed: bool,
) -> impl IntoElement {
    let send = send.cloned().filter(|_| is_changed);
    Button::new(
        ("settings-reset", index),
        navo_l10n::tr("settings-reset-default"),
    )
    .variant(ButtonVariant::Secondary)
    .icon(IconName::RotateCcw)
    .disabled(!is_changed)
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

/// The element id and string of the note for a key the OS refused; only the field whose key
/// was refused gets one.
fn taken_note(
    field: Field,
    (summon, clipboard): (bool, bool),
) -> Option<(&'static str, &'static str)> {
    match field {
        Field::SummonKey if summon => Some(("settings-summon-taken", "settings-summon-key-taken")),
        Field::ClipboardKey if clipboard => {
            Some(("settings-clipboard-taken", "settings-clipboard-key-taken"))
        }
        _ => None,
    }
}

/// `taken` says whether the summon key and the clipboard key were refused.
pub(super) fn notes(
    t: &Theme,
    field: Field,
    taken: (bool, bool),
) -> impl Iterator<Item = AnyElement> + use<> {
    taken_note(field, taken)
        .map(|(id, key)| note(t, id, navo_l10n::tr(key).into(), t.colors.mark))
        .into_iter()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_key_notes_only_its_own_field() {
        let clipboard = Some(("settings-clipboard-taken", "settings-clipboard-key-taken"));
        let summon = Some(("settings-summon-taken", "settings-summon-key-taken"));
        assert_eq!(taken_note(Field::ClipboardKey, (false, true)), clipboard);
        assert_eq!(taken_note(Field::SummonKey, (true, false)), summon);
        assert_eq!(taken_note(Field::ClipboardKey, (true, false)), None);
        assert_eq!(taken_note(Field::SummonKey, (false, true)), None);
        assert_eq!(taken_note(Field::ClipboardKey, (false, false)), None);
        assert_eq!(taken_note(Field::Font, (true, true)), None);
    }
}
