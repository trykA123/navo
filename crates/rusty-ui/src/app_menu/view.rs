use std::rc::Rc;

use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, App, Div, FontWeight, Hsla, InteractiveElement,
    IntoElement, ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled,
    Window, div, img, px,
};

use super::items::QuickSettings;
use super::items::{self, RowSpec, TileFace, Trailing, tile_face};
use super::{AppMenuEvent, Identity, event_for, item_at};
use crate::commands::Keymap;
use crate::primitives::{IconName, icon};
use crate::title_bar::logo;
use crate::tokens::Theme;

const WIDTH: f32 = 380.;
const LOGO: f32 = 36.;
const TILE_HEIGHT: f32 = 70.;
const ROW_HEIGHT: f32 = 30.;
const ICON_COLUMN: f32 = 18.;

type Handler = Rc<dyn Fn(&AppMenuEvent, &mut Window, &mut App)>;

pub(super) struct Look<'a> {
    pub theme: &'a Theme,
    pub keymap: &'a Keymap,
    pub selected: usize,
    pub send: Option<Handler>,
}

pub(super) fn panel(
    t: &Theme,
    root: ObservedElement<Stateful<Div>>,
) -> ObservedElement<Stateful<Div>> {
    root.occlude()
        .flex()
        .flex_col()
        .w(px(WIDTH))
        .p(t.spacing.xl)
        .rounded(t.radii.sheet)
        .border_1()
        .border_color(t.surfaces.tile_edge)
        .bg(t.surfaces.sheet)
        .shadow(t.shadows.menu.clone())
        .text_color(t.colors.text)
        .text_size(t.typography.body)
}

pub(super) fn enter(panel: ObservedElement<Stateful<Div>>, t: &Theme) -> AnyElement {
    let animation = Animation::new(t.motion.fade).with_easing(ease_out_cubic);
    panel
        .with_animation("app-menu-enter", animation, |panel, delta| {
            crate::perf::note(crate::perf::UiFlag::AppMenu);
            crate::perf::animating(crate::perf::Latency::MenuOpen, delta < 1.);
            panel.opacity(delta)
        })
        .into_any_element()
}

pub(super) fn header(t: &Theme, identity: &Identity, scale_factor: f32) -> Div {
    let mark = div()
        .flex_none()
        .size(px(LOGO))
        .p(t.spacing.sm)
        .rounded(t.radii.card)
        .bg(t.surfaces.tile)
        .border_1()
        .border_color(t.surfaces.tile_edge)
        .child(img(logo(scale_factor)).size_full());
    let text = div()
        .id("app-menu-identity")
        .test_support()
        .role(Role::Label)
        .aria_label(format!("{}, {}", identity.title(), identity.subtitle()))
        .flex()
        .flex_col()
        .min_w_0()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(identity.title()),
        )
        .child(
            div()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(identity.subtitle()),
        );
    div()
        .flex()
        .items_center()
        .gap(t.spacing.lg)
        .px(t.spacing.xs)
        .pb(t.spacing.lg)
        .child(mark)
        .child(text)
}

pub(super) fn error_line(t: &Theme, message: SharedString) -> impl IntoElement {
    div()
        .id("app-menu-error")
        .role(Role::Alert)
        .aria_label(message.clone())
        .test_support()
        .px(t.spacing.xs)
        .pb(t.spacing.lg)
        .text_size(t.typography.small)
        .text_color(t.colors.mark)
        .child(message)
}

fn hover_and_click<E: StatefulInteractiveElement>(
    element: E,
    index: usize,
    send: Option<Handler>,
) -> E {
    let Some(send) = send else {
        return element;
    };
    let hover = send.clone();
    element
        .on_hover(move |is_hovered, window, cx| {
            if *is_hovered {
                hover(&AppMenuEvent::Select(index), window, cx);
            }
        })
        .on_click(move |_, window, cx| {
            if let Some(event) = item_at(index).and_then(event_for) {
                send(&event, window, cx);
            }
        })
}

fn tile(look: &Look, face: TileFace, index: usize) -> AnyElement {
    let t = look.theme;
    let (accent, muted) = (t.colors.accent_text, t.colors.muted);
    let tile = div()
        .id(("app-menu-tile", index))
        .test_support()
        .role(Role::MenuItem)
        .aria_label(face.label)
        .aria_value(face.value.clone())
        .aria_selected(index == look.selected)
        .flex()
        .flex_col()
        .items_start()
        .gap(t.spacing.xxs)
        .h(px(TILE_HEIGHT))
        .px(t.spacing.lg)
        .py(t.spacing.md)
        .rounded(t.radii.card)
        .border_1()
        .border_color(t.surfaces.tile_edge)
        .bg(t.surfaces.tile)
        .cursor_pointer()
        .hover(|tile| tile.border_color(t.colors.faint))
        .when(face.is_on, |tile| {
            tile.bg(t.surfaces.layer_tint).border_color(t.colors.accent)
        })
        .when(index == look.selected, |tile| {
            tile.shadow(t.shadows.focus.clone())
        })
        .child(icon(
            face.icon,
            t.spacing.icon,
            if face.is_on { accent } else { muted },
        ))
        .child(
            div()
                .text_size(t.typography.small)
                .text_color(muted)
                .child(face.label),
        )
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .when(face.is_on, |value| value.text_color(accent))
                .child(face.value),
        );
    hover_and_click(tile, index, look.send.clone()).into_any_element()
}

pub(super) fn tiles(look: &Look, settings: QuickSettings) -> Div {
    let tiles = items::TILES
        .iter()
        .enumerate()
        .map(|(index, setting)| tile(look, tile_face(*setting, settings), index));
    div()
        .grid()
        .grid_cols(items::COLUMNS as u16)
        .gap(look.theme.spacing.md)
        .mb(look.theme.spacing.lg)
        .children(tiles)
}

struct RowColors {
    text: Hsla,
    icon: Hsla,
    trailing: Hsla,
}

fn row_colors(t: &Theme, spec: &RowSpec, is_selected: bool) -> RowColors {
    let c = &t.colors;
    if t.is_contrast && is_selected {
        return RowColors {
            text: c.on_selection,
            icon: c.on_selection,
            trailing: c.on_selection,
        };
    }
    if t.is_contrast && spec.command.is_primary() && spec.command.is_available() {
        return RowColors {
            text: c.on_button,
            icon: c.on_button,
            trailing: c.on_button,
        };
    }
    match (spec.command.is_available(), spec.command.is_primary()) {
        (false, _) => RowColors {
            text: c.faint,
            icon: c.faint,
            trailing: c.faint,
        },
        (true, true) => RowColors {
            text: c.background,
            icon: c.background,
            trailing: c.background,
        },
        (true, false) => RowColors {
            text: c.text,
            icon: if is_selected { c.accent_text } else { c.muted },
            trailing: c.muted,
        },
    }
}

fn row_fill(t: &Theme, spec: &RowSpec, is_selected: bool) -> Option<Hsla> {
    let c = &t.colors;
    if t.is_contrast && is_selected {
        return Some(c.selection);
    }
    if t.is_contrast && spec.command.is_primary() && spec.command.is_available() {
        return Some(t.surfaces.keycap_face);
    }
    match (spec.command.is_primary(), spec.command.is_available()) {
        (true, true) => Some(c.accent),
        (true, false) if !is_selected => Some(t.surfaces.layer_tint),
        _ => is_selected.then_some(c.selection),
    }
}

fn trailing(t: &Theme, spec: &RowSpec, keys: Option<String>, color: Hsla) -> AnyElement {
    match (&spec.trailing, keys) {
        (Trailing::Keys, Some(keys)) => div()
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.small)
            .text_color(color)
            .child(keys)
            .into_any_element(),
        (Trailing::Keys, None) => div().into_any_element(),
        (Trailing::Submenu, _) => {
            icon(IconName::ChevronRight, t.spacing.icon, color).into_any_element()
        }
    }
}

fn row(look: &Look, spec: &RowSpec, index: usize) -> AnyElement {
    let t = look.theme;
    let is_selected = index == look.selected;
    let is_available = spec.command.is_available();
    let colors = row_colors(t, spec, is_selected);
    let keys = match spec.trailing {
        Trailing::Keys => items::row_keys(look.keymap, spec.command),
        Trailing::Submenu => None,
    };
    let element = div()
        .id(("app-menu-row", index))
        .test_support()
        .role(Role::MenuItem)
        .aria_label(spec.label)
        .aria_selected(is_selected)
        .when_some(keys.clone(), |row, keys| row.aria_keyshortcuts(keys))
        .when(!is_available, |row| row.aria_value("Not available yet"))
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .h(px(ROW_HEIGHT))
        .px(t.spacing.md)
        .rounded(t.radii.sm)
        .text_color(colors.text)
        .when(spec.command.is_primary(), |row| {
            row.mt(t.spacing.xs).font_weight(FontWeight::SEMIBOLD)
        })
        .when(is_available, |row| row.cursor_pointer())
        .when_some(row_fill(t, spec, is_selected), |row, fill| row.bg(fill))
        .child(div().w(px(ICON_COLUMN)).flex_none().child(icon(
            spec.icon,
            t.spacing.icon,
            colors.icon,
        )))
        .child(div().flex_1().min_w_0().truncate().child(spec.label))
        .child(trailing(t, spec, keys, colors.trailing));
    hover_and_click(element, index, look.send.clone()).into_any_element()
}

pub(super) fn rows(look: &Look) -> Vec<AnyElement> {
    let first = items::TILES.len();
    items::ROWS
        .iter()
        .enumerate()
        .map(|(offset, spec)| row(look, spec, first + offset))
        .collect()
}
