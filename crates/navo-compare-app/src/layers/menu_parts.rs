//! Pieces every topbar popover draws with: the frame look, headings, marks and the shadow.

use gpui_kit::assets::IconName;
use gpui_kit::{BoxShadow, Toggled};
use sidewise_kit::prelude::*;
use sidewise_kit::primitives::icon;

use crate::theme_data::palettes::ModeTokens;

/// The colours and ring a menu frame is drawn with.
pub(super) struct Look {
    pub(super) m: ModeTokens,
    pub(super) ring_w: Pixels,
    pub(super) focused: bool,
}

/// A small muted heading (`View`).
pub(super) fn heading(text: &'static str, m: &ModeTokens) -> Div {
    div()
        .px_3()
        .pt_2()
        .pb_1()
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(m.muted)
        .child(text.to_uppercase())
}

/// The mark column of a row: nothing, a check (toggle) or a dot (choice).
pub(super) fn mark(on: bool, toggle: bool, m: &ModeTokens) -> AnyElement {
    let slot = div().w(px(16.)).flex().flex_none().justify_center();
    match (on, toggle) {
        (false, _) => slot.into_any_element(),
        (true, true) => slot
            .child(icon(IconName::Check, px(12.), m.accent))
            .into_any_element(),
        (true, false) => slot
            .child(div().size(px(6.)).rounded_full().bg(m.accent))
            .into_any_element(),
    }
}

/// The popover's drop shadow: the palette's middle shadow, 8 px down and 24 px soft.
pub(super) fn card_shadow(m: &ModeTokens) -> BoxShadow {
    BoxShadow {
        color: m.shadow[1],
        offset: point(px(0.), px(8.)),
        blur_radius: px(24.),
        spread_radius: px(0.),
        inset: false,
    }
}

/// `Toggled` for an accessibility state.
pub(super) fn toggled(on: bool) -> Toggled {
    if on { Toggled::True } else { Toggled::False }
}
