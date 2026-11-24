//! The Shift, Ctrl and Alt pills at the start of the F-key bar. A pill lights while its
//! modifier is held or its layer is pinned; a click pins or unpins that layer.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, Role,
    StatefulInteractiveElement, Styled, Toggled, Window,
};

use super::{KeyLayer, pill};
use crate::FluentBuilder as _;
use crate::tokens::Theme;

/// Called with the pill's layer when the pill is clicked.
pub type OnPin = Rc<dyn Fn(KeyLayer, &mut Window, &mut App)>;

/// The pills' state: the held layer, the pinned layer and the click handler, if any.
#[derive(Clone, Default)]
pub(super) struct Pins {
    pub(super) pinned: Option<KeyLayer>,
    pub(super) on_pin: Option<OnPin>,
}

/// One pill per modifier, lit for the held or pinned layer; a button when `pins` has a handler.
pub(super) fn modifier_pills(t: &Theme, held: Option<KeyLayer>, pins: &Pins) -> Vec<AnyElement> {
    let s = &t.surfaces;
    KeyLayer::ALL
        .into_iter()
        .enumerate()
        .map(|(index, layer)| {
            let is_pinned = pins.pinned == Some(layer);
            let is_lit = held == Some(layer) || is_pinned;
            let on_pin = pins.on_pin.clone();
            let role = if on_pin.is_some() {
                Role::Button
            } else {
                Role::Label
            };
            pill(t)
                .id(("fkey-mod", index))
                .role(role)
                .aria_label(layer.name())
                .aria_selected(is_lit)
                .when(on_pin.is_some(), |pill| {
                    let toggled = if is_pinned {
                        Toggled::True
                    } else {
                        Toggled::False
                    };
                    pill.aria_toggled(toggled).cursor_pointer()
                })
                .when(pins.on_pin.is_some() && !is_lit, |pill| {
                    pill.hover(|style| style.bg(t.colors.hover))
                })
                .test_support()
                .flex_none()
                .px(t.spacing.md)
                .text_size(t.typography.small)
                .when(is_lit, |pill| {
                    pill.bg(s.keycap_lit)
                        .border_color(s.keycap_lit)
                        .text_color(s.keycap_lit_label)
                })
                .when_some(on_pin, |pill, on_pin| {
                    pill.on_click(move |_, window, cx| on_pin(layer, window, cx))
                })
                .child(layer.name())
                .into_any_element()
        })
        .collect()
}
