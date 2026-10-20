use std::sync::LazyLock;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Hsla, Role};

use super::fields::{Choice, Field, Look};
use super::values::{Change, Value};
use super::{Handler, SheetEvent};
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::{Colors, Density, Mode, Surfaces, Theme};

const TOGGLE: (f32, f32) = (34., 18.);
const KNOB: f32 = 10.;
const SEGMENT_HEIGHT: f32 = 24.;
const THUMB_WIDTH: f32 = 96.;
const SWATCH_HEIGHT: f32 = 48.;
const STRIP: f32 = 8.;
const BAR: f32 = 3.;
const VALUE_WIDTH: f32 = 168.;

struct Palette {
    colors: Colors,
    surfaces: Surfaces,
}

static PALETTES: LazyLock<[Palette; 2]> = LazyLock::new(|| {
    [Mode::Light, Mode::Dark].map(|mode| {
        let theme = Theme::new(mode);
        Palette {
            colors: theme.colors,
            surfaces: theme.surfaces,
        }
    })
});

fn pick_on_click<E: StatefulInteractiveElement>(
    element: E,
    send: Option<&Handler>,
    change: Change,
) -> E {
    let Some(send) = send.cloned() else {
        return element;
    };
    element.on_click(move |_, window, cx| send(&SheetEvent::Set(change.clone()), window, cx))
}

pub(super) struct Picks<'a> {
    pub t: &'a Theme,
    pub field: Field,
    pub options: &'static [Choice],
    pub at: usize,
    pub send: Option<&'a Handler>,
}

impl Picks<'_> {
    fn change(&self, index: usize) -> Change {
        Change {
            field: self.field,
            value: Value::Pick(index),
        }
    }
}

pub(super) fn segments(picks: &Picks) -> impl IntoElement + use<> {
    let t = picks.t;
    let buttons = picks.options.iter().enumerate().map(|(index, option)| {
        let is_on = index == picks.at;
        let button = div()
            .id(("settings-segment", index))
            .role(Role::RadioButton)
            .aria_label(option.text())
            .aria_selected(is_on)
            .test_support()
            .flex()
            .items_center()
            .h(px(SEGMENT_HEIGHT))
            .px(t.spacing.lg)
            .rounded(t.radii.sm)
            .whitespace_nowrap()
            .cursor_pointer()
            .text_color(if is_on { t.colors.text } else { t.colors.muted })
            .when(is_on, |button| {
                button
                    .bg(t.surfaces.sheet)
                    .border_1()
                    .border_color(t.surfaces.card_edge)
                    .font_weight(FontWeight::SEMIBOLD)
            })
            .child(option.text());
        pick_on_click(button, picks.send, picks.change(index))
    });
    div()
        .flex()
        .flex_none()
        .gap(t.spacing.xxs)
        .p(t.spacing.xxs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .text_size(t.typography.small)
        .children(buttons)
}

pub(super) fn toggle(t: &Theme, is_on: bool, send: Option<&Handler>, field: Field) -> AnyElement {
    let (track, edge, knob) = if is_on {
        (t.colors.accent, t.colors.accent, t.colors.background)
    } else {
        (t.surfaces.field, t.colors.faint, t.colors.muted)
    };
    let inset = (px(TOGGLE.1) - px(KNOB)) / 2. - px(1.);
    let switch = div()
        .id("settings-toggle")
        .role(Role::Switch)
        .aria_selected(is_on)
        .test_support()
        .relative()
        .flex_none()
        .w(px(TOGGLE.0))
        .h(px(TOGGLE.1))
        .rounded(t.radii.full)
        .border_1()
        .border_color(edge)
        .bg(track)
        .cursor_pointer()
        .child(
            div()
                .absolute()
                .top(inset)
                .when(is_on, |knob| knob.right(inset))
                .when(!is_on, |knob| knob.left(inset))
                .size(px(KNOB))
                .rounded(t.radii.full)
                .bg(knob),
        );
    let change = Change {
        field,
        value: Value::Flag(!is_on),
    };
    let label = div()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(if is_on { "On" } else { "Off" });
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .child(label)
        .child(pick_on_click(switch, send, change))
        .into_any_element()
}

pub(super) fn value_box(t: &Theme, text: SharedString) -> AnyElement {
    let shown = if text.is_empty() {
        SharedString::from("Not set")
    } else {
        text.clone()
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .max_w(px(VALUE_WIDTH))
        .h(px(SEGMENT_HEIGHT + 4.))
        .px(t.spacing.lg)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.surfaces.field)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .text_color(if text.is_empty() {
            t.colors.muted
        } else {
            t.colors.text
        })
        .child(div().min_w_0().truncate().child(shown))
        .child(icon(IconName::Pencil, t.spacing.icon, t.colors.muted))
        .into_any_element()
}

fn swatch_half(t: &Theme, palette: &Palette) -> Div {
    let c = &palette.colors;
    let bar = |color: Hsla, width: f32| {
        div()
            .h(px(BAR))
            .w(relative(width))
            .rounded(t.radii.full)
            .bg(color)
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .h_full()
        .bg(palette.surfaces.card)
        .child(div().h(px(STRIP)).bg(c.rail))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(BAR + 1.))
                .p(t.spacing.md)
                .child(bar(c.faint, 0.9))
                .child(bar(c.accent, 0.6))
                .child(bar(c.faint, 0.8)),
        )
}

fn theme_swatch(t: &Theme, index: usize) -> Div {
    let [light, dark] = &*PALETTES;
    let row = div().flex().size_full();
    match index {
        1 => row.child(swatch_half(t, light)),
        2 => row.child(swatch_half(t, dark)),
        _ => row.child(swatch_half(t, light)).child(swatch_half(t, dark)),
    }
}

fn density_swatch(t: &Theme, index: usize) -> Div {
    let order = [Density::Compact, Density::Normal, Density::Comfortable];
    let density = order.get(index).copied().unwrap_or_default();
    let gap = Theme::with_density(t.mode, density).rows.height / 4.;
    let bars = (0..4).map(|_| {
        div()
            .h(px(BAR))
            .w_full()
            .rounded(t.radii.full)
            .bg(t.colors.faint)
    });
    div()
        .flex()
        .flex_col()
        .size_full()
        .gap(gap)
        .p(t.spacing.md)
        .bg(t.colors.background)
        .children(bars)
}

pub(super) fn pictures(picks: &Picks, look: Look) -> impl IntoElement + use<> {
    let t = picks.t;
    let thumbs = picks.options.iter().enumerate().map(|(index, option)| {
        let is_on = index == picks.at;
        let swatch = match look {
            Look::DensityPictures => density_swatch(t, index),
            _ => theme_swatch(t, index),
        };
        let name = match look {
            Look::DensityPictures => "settings-density",
            _ => "settings-theme",
        };
        let thumb = div()
            .id((name, index))
            .role(Role::RadioButton)
            .aria_label(option.text())
            .aria_selected(is_on)
            .test_support()
            .flex()
            .flex_col()
            .gap(t.spacing.sm)
            .w(px(THUMB_WIDTH))
            .cursor_pointer()
            .text_size(t.typography.small)
            .text_color(if is_on { t.colors.text } else { t.colors.muted })
            .when(is_on, |thumb| thumb.font_weight(FontWeight::SEMIBOLD))
            .child(
                div()
                    .h(px(SWATCH_HEIGHT))
                    .rounded(t.radii.md)
                    .overflow_hidden()
                    .border_2()
                    .border_color(if is_on {
                        t.colors.accent
                    } else {
                        t.colors.line
                    })
                    .child(swatch),
            )
            .child(option.text());
        pick_on_click(thumb, picks.send, picks.change(index))
    });
    div()
        .flex()
        .gap(t.spacing.lg)
        .pt(t.spacing.sm)
        .children(thumbs)
}
