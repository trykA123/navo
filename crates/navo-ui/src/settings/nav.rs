use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::SheetEvent;
use super::fields::Section;
use super::steps::first_row;
use super::view::{DIMMED, Look, section_label, send_on_click};
use crate::prelude::*;
use crate::primitives::{IconName, icon};

const NAV_WIDTH: f32 = 148.;
const NAV_ITEM: f32 = 30.;
const MARKER: (f32, f32) = (3., 14.);
const SHORTCUTS: &str = "Shortcuts";

struct NavItem {
    index: usize,
    label: SharedString,
    glyph: IconName,
    is_on: bool,
    is_empty: bool,
    event: SheetEvent,
}

fn nav_item(look: &Look, item: NavItem) -> AnyElement {
    let theme = look.theme.selected_content(item.is_on);
    let t = &theme;
    let marker = div()
        .absolute()
        .left_0()
        .top((px(NAV_ITEM) - px(MARKER.1)) / 2.)
        .w(px(MARKER.0))
        .h(px(MARKER.1))
        .rounded(t.radii.full)
        .bg(t.colors.accent);
    let tint = if item.is_on {
        t.colors.accent_text
    } else {
        t.colors.muted
    };
    let element = div()
        .id(("settings-section", item.index))
        .role(Role::Tab)
        .aria_label(item.label.clone())
        .aria_selected(item.is_on)
        .test_support()
        .relative()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(NAV_ITEM))
        .px(t.spacing.lg)
        .rounded(t.radii.md)
        .cursor_pointer()
        .whitespace_nowrap()
        .when(item.is_empty, |element| element.opacity(DIMMED))
        .hover(|element| element.bg(t.colors.hover))
        .when(item.is_on, |element| {
            element
                .bg(t.colors.selection)
                .text_color(t.colors.on_selection)
                .font_weight(FontWeight::SEMIBOLD)
                .child(marker)
        })
        .child(icon(item.glyph, t.spacing.icon, tint))
        .child(item.label);
    let send = look.send.as_ref().filter(|_| !item.is_empty);
    send_on_click(element, send, item.event).into_any_element()
}

pub(super) fn nav(look: &Look, current: Option<Section>) -> impl IntoElement + use<> {
    let t = look.theme;
    let sections = Section::ALL
        .iter()
        .enumerate()
        .filter(|(_, section)| section.is_shown())
        .map(|(index, section)| NavItem {
            index,
            label: section_label(*section),
            glyph: section.icon(),
            is_on: current == Some(*section),
            is_empty: look.filter.first_in(*section).is_none(),
            event: SheetEvent::Select(
                look.filter
                    .first_in(*section)
                    .unwrap_or(first_row(*section)),
            ),
        });
    let shortcuts = NavItem {
        index: Section::ALL.len(),
        label: SHORTCUTS.into(),
        glyph: IconName::Keyboard,
        is_on: current.is_none(),
        is_empty: false,
        event: SheetEvent::Shortcuts,
    };
    let items = sections.chain([shortcuts]).map(|item| nav_item(look, item));
    div()
        .id("settings-nav")
        .role(Role::TabList)
        .aria_label("Sections")
        .test_support()
        .flex()
        .flex_col()
        .flex_none()
        .gap(t.spacing.xxs)
        .w(px(NAV_WIDTH))
        .p(t.spacing.md)
        .border_r_1()
        .border_color(t.surfaces.card_edge)
        .children(items)
}
