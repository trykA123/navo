//! The Settings nav: ten pages in three clusters, each page's Ctrl+digit while Ctrl is held,
//! and the greyed-out Modules page that waits for the first optional module.

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::SheetEvent;
use super::fields::Section;
use super::steps::digit_of;
use super::view::{DIMMED, Look, section_label, send_on_click};
use crate::prelude::*;
use crate::primitives::{IconName, Kbd, icon};

const NAV_WIDTH: f32 = 200.;
const NAV_ITEM: f32 = 30.;
const MARKER: (f32, f32) = (3., 14.);
/// The nav draws a divider after these pages: the end of each cluster.
const CLUSTER_ENDS: [Section; 2] = [Section::CopyDelete, Section::TrayClipboard];

struct NavItem {
    index: usize,
    page: Section,
    is_on: bool,
    event: Option<SheetEvent>,
}

/// What the nav needs besides the look: the shown page (`None` while the shortcut list
/// has focus, which belongs to Keyboard) and whether Ctrl is held.
pub(super) struct NavState {
    current: Option<Section>,
    shows_digits: bool,
}

impl NavState {
    /// The nav state for the shown page; the digits show while Ctrl is held.
    pub(super) fn new(current: Option<Section>, window: &Window) -> Self {
        Self {
            current,
            shows_digits: window.modifiers().control,
        }
    }
}

fn nav_item(look: &Look, item: NavItem, shows_digits: bool) -> AnyElement {
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
    let digit = digit_of(item.page);
    let cap = Kbd::new(("settings-digit", item.index), &digit.to_string())
        .filter(|_| shows_digits && item.event.is_some());
    let element = div()
        .id(("settings-section", item.index))
        .role(Role::Tab)
        .aria_label(section_label(item.page))
        .aria_selected(item.is_on)
        .aria_keyshortcuts(format!("Control+{digit}"))
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
        .when(item.event.is_none(), |element| element.opacity(DIMMED))
        .hover(|element| element.bg(t.colors.hover))
        .when(item.is_on, |element| {
            element
                .bg(t.colors.selection)
                .text_color(t.colors.on_selection)
                .font_weight(FontWeight::SEMIBOLD)
                .child(marker)
        })
        .child(icon(item.page.icon(), t.spacing.icon, tint))
        .child(div().flex_1().min_w_0().child(section_label(item.page)))
        .children(cap);
    match item.event {
        Some(event) => send_on_click(element, look.send.as_ref(), event).into_any_element(),
        None => element.into_any_element(),
    }
}

/// The page that waits for optional modules: greyed out, never clickable, announced as
/// unavailable.
fn modules_item(look: &Look) -> AnyElement {
    let t = look.theme;
    let label = navo_l10n::tr("settings-modules");
    div()
        .id("settings-modules")
        .role(Role::Tab)
        .aria_label(label.clone())
        .aria_description(navo_l10n::tr("settings-modules-unavailable"))
        .aria_selected(false)
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(NAV_ITEM))
        .px(t.spacing.lg)
        .opacity(DIMMED)
        .text_color(t.colors.faint)
        .child(icon(IconName::Layers, t.spacing.icon, t.colors.faint))
        .child(label)
        .into_any_element()
}

fn divider(look: &Look) -> AnyElement {
    let t = look.theme;
    div()
        .flex_none()
        .h(px(1.))
        .mx(t.spacing.md)
        .my(t.spacing.sm)
        .bg(t.colors.line)
        .into_any_element()
}

/// Builds the nav column: the pages in nav order with cluster dividers, then Modules.
pub(super) fn nav(look: &Look, state: &NavState) -> impl IntoElement + use<> {
    let t = look.theme;
    let shown = state.current.unwrap_or(Section::Keyboard);
    let mut items = Vec::new();
    for (index, page) in Section::ALL.into_iter().enumerate() {
        let item = NavItem {
            index,
            page,
            is_on: shown == page,
            event: look.filter.first_in(page).map(SheetEvent::Select),
        };
        items.push(nav_item(look, item, state.shows_digits));
        if CLUSTER_ENDS.contains(&page) {
            items.push(divider(look));
        }
    }
    items.push(divider(look));
    items.push(modules_item(look));
    div()
        .id("settings-nav")
        .role(Role::TabList)
        .aria_label("Pages")
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
