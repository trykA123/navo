//! The ribbon header: the held keys, "N keys in the file list", the combo hints
//! ("+ Shift 8 more") and the quiet Turn off button.

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::drawer::{TurnOff, cap};
use super::layer::Layer;
use crate::prelude::*;
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens::Theme;

pub(super) const HEADER_HEIGHT: f32 = 24.;
const CHIP_HEIGHT: f32 = 22.;
const SEPARATOR_HEIGHT: f32 = 16.;

/// Turn off's shortcut (for its accessible name) and its click and press handlers.
pub(super) struct TurnOffParts {
    pub key: Option<SharedString>,
    pub on_click: Option<TurnOff>,
    pub on_press: Option<TurnOff>,
}

/// The header text before the combo hints: "25 keys in the file list".
#[must_use]
pub(super) fn summary(layer: &Layer) -> String {
    let count = layer.count();
    let noun = if count == 1 { "key" } else { "keys" };
    format!("{count} {noun} in the file list")
}

/// One lit cap per held modifier, joined by "+".
fn held_keys(t: &Theme, layer: &Layer) -> impl IntoElement + use<> {
    let name = layer.combo.name();
    let chips: Vec<AnyElement> = name
        .split('+')
        .enumerate()
        .flat_map(|(index, key)| {
            let joiner = (index > 0).then(|| div().child("+").into_any_element());
            let chip = div()
                .flex()
                .items_center()
                .h(px(CHIP_HEIGHT))
                .px(t.spacing.md)
                .rounded(t.radii.keycap)
                .bg(t.surfaces.keycap_lit)
                .text_color(t.surfaces.keycap_lit_label)
                .font_family(t.typography.mono_font.clone())
                .font_weight(FontWeight::SEMIBOLD)
                .child(key.to_owned())
                .into_any_element();
            joiner.into_iter().chain([chip])
        })
        .collect();
    div()
        .id("modifier-hints-combo")
        .role(Role::Label)
        .aria_label(SharedString::from(name))
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .children(chips)
}

/// "+ [Shift] 8 more" for each layer one modifier away.
fn combo_hints(t: &Theme, layer: &Layer) -> Vec<AnyElement> {
    layer
        .more
        .iter()
        .map(|more| {
            let spoken = format!("{} more with {}", more.count, more.modifier);
            div()
                .id(SharedString::from(format!(
                    "modifier-hints-more-{}",
                    more.modifier
                )))
                .role(Role::Label)
                .aria_label(spoken)
                .test_support()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.spacing.sm)
                .child("+")
                .child(cap(t, more.modifier.into()).h(px(18.)))
                .child(format!("{} more", more.count))
                .into_any_element()
        })
        .collect()
}

/// Builds the quiet Turn off button: a press stays on it (rows below never see it), a release on it clicks.
fn turn_off(t: &Theme, parts: TurnOffParts) -> impl IntoElement {
    let shortcut = parts
        .key
        .and_then(|key| Kbd::new("modifier-hints-off-key", &key))
        .map(|kbd| kbd.text());
    let hover = (t.colors.hover, t.colors.text);
    div()
        .id("modifier-hints-off")
        .role(Role::Button)
        .aria_label("Turn off modifier hints")
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(HEADER_HEIGHT))
        .px(t.spacing.md)
        .rounded(t.radii.full)
        .text_color(t.colors.muted)
        .cursor_pointer()
        .hover(move |button| button.bg(hover.0).text_color(hover.1))
        .occlude()
        .when_some(parts.on_press, |button, press| {
            button.on_mouse_down(MouseButton::Left, move |_, window, cx| press(window, cx))
        })
        .when_some(parts.on_click, |button, handler| {
            button.on_click(move |_, window, cx| handler(window, cx))
        })
        .child(icon(
            IconName::EyeOff,
            t.spacing.icon - px(3.),
            t.colors.muted,
        ))
        .child("Turn off")
        .when_some(shortcut, |button, keys| button.aria_keyshortcuts(keys))
}

/// The header row: held keys, count, combo hints, then a separator and Turn off at the right.
pub(super) fn header(t: &Theme, layer: &Layer, parts: TurnOffParts) -> Div {
    let text = (!layer.is_empty()).then(|| {
        let summary = summary(layer);
        div()
            .id("modifier-hints-count")
            .role(Role::Label)
            .aria_label(summary.clone())
            .test_support()
            .flex_none()
            .child(summary)
    });
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(HEADER_HEIGHT))
        .mb(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(held_keys(t, layer))
        .children(text)
        .children(combo_hints(t, layer))
        .child(div().flex_1())
        .child(div().w(px(1.)).h(px(SEPARATOR_HEIGHT)).bg(t.colors.line))
        .child(turn_off(t, parts))
}
