use std::rc::Rc;

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use rusty_l10n::tr;

use super::stops::{Act, Stop};
use super::{StopHandler, key_of};
use crate::activity::ActivityItem;
use crate::prelude::*;
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens::Theme;

pub(super) const BUTTON_HEIGHT: f32 = 24.;

pub(super) struct Face {
    pub id: ElementId,
    pub label: SharedString,
    pub glyph: Option<IconName>,
    pub key: Option<SharedString>,
    pub is_selected: bool,
    pub is_chip: bool,
}

fn act_face(act: Act) -> (&'static str, Option<IconName>, Option<&'static str>) {
    match act {
        Act::Pause => ("activity-pause", Some(IconName::Pause), None),
        Act::Resume => ("activity-resume", Some(IconName::Play), None),
        Act::Cancel => ("activity-cancel", Some(IconName::X), None),
        Act::Show => ("activity-show", None, Some("enter")),
        Act::Dismiss => ("activity-dismiss", None, Some("delete")),
    }
}

pub(super) fn act_button(
    item: &ActivityItem,
    act: Act,
    selected: Option<Stop>,
    shows_keys: bool,
) -> Face {
    let (name, glyph, key) = act_face(act);
    let key = key.filter(|_| shows_keys);
    Face {
        id: (name, key_of(item.id)).into(),
        label: tr(name).into(),
        glyph,
        key: key.map(SharedString::from),
        is_selected: selected == Some(Stop::Button(item.id, act)),
        is_chip: false,
    }
}

pub(super) fn chip(item: &ActivityItem, selected: Option<Stop>) -> Face {
    Face {
        id: ("activity-chip", key_of(item.id)).into(),
        label: item.chip.clone(),
        glyph: None,
        key: None,
        is_selected: selected == Some(Stop::Chip(item.id)),
        is_chip: true,
    }
}

pub(super) fn ring<E: Styled>(t: &Theme, element: E, is_selected: bool) -> E {
    if is_selected {
        element.border_color(t.colors.accent)
    } else {
        element
    }
}

pub(super) fn render(
    t: &Theme,
    face: Face,
    stop: Stop,
    on_stop: Option<&StopHandler>,
) -> impl IntoElement + use<> {
    let kbd = face.key.and_then(|key| Kbd::new("activity-key", &key));
    let spoken = match kbd.as_ref().map(Kbd::text) {
        Some(keys) => format!("{} {keys}", face.label),
        None => face.label.to_string(),
    };
    let (fill, text) = if face.is_chip {
        (t.surfaces.field, t.colors.muted)
    } else {
        (t.surfaces.card, t.colors.text)
    };
    let hover = t.colors.hover;
    let handler = on_stop.map(Rc::clone);
    let button = div()
        .id(face.id)
        .role(Role::Button)
        .aria_label(spoken)
        .aria_selected(face.is_selected)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(BUTTON_HEIGHT))
        .px(t.spacing.md)
        .rounded(t.radii.full)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .bg(fill)
        .text_size(t.typography.small)
        .text_color(text)
        .whitespace_nowrap()
        .when(face.is_chip, |chip| {
            chip.flex_shrink(1.).min_w_0().truncate()
        })
        .cursor_pointer()
        .hover(move |button| button.bg(hover))
        .children(
            face.glyph
                .map(|glyph| icon(glyph, t.spacing.icon - px(2.), text)),
        )
        .child(face.label)
        .children(kbd)
        .when_some(handler, |button, handler| {
            button.on_click(move |_, window, cx| handler(stop, window, cx))
        });
    ring(t, button, face.is_selected)
}
