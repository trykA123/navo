//! The modifier ribbon card: groups of one key cap and a short label each, flowed into
//! columns of a row count shared by Ctrl, Shift and Alt, and the F-row while the F-key bar is hidden.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt, ElementId, RenderOnce, Role};

use super::drawer_header::{HEADER_HEIGHT, TurnOffParts, header};
use super::layer::{Entry, Group, Layer, Shape};
use crate::prelude::*;
use crate::primitives::{IconName, Tooltip, icon};
use crate::tokens::{self, Theme};

const CAP_HEIGHT: f32 = 20.;
const CAP_MIN_WIDTH: f32 = 22.;
/// Width of one mono character in a key cap, and the cap's side padding, in px (estimate).
const CAP_CHAR: f32 = 7.6;
const CAP_PAD: f32 = 12.;
/// The label column; it sets how many rows a layer needs (design `RB_LABEL_W`).
const LABEL_WIDTH: f32 = 116.;
const GROUP_GAP: f32 = 28.;
const TITLE_HEIGHT: f32 = 18.;
const FKEY_CAP_WIDTH: f32 = 28.;
const FROW_HEIGHT: f32 = 22.;
const FEWEST_ROWS: usize = 4;
const MOST_ROWS: usize = 7;
const FKEY_COUNT: u8 = 10;

pub type TurnOff = Rc<dyn Fn(&mut Window, &mut App)>;

/// Where the card sits: its inner width and whether the F-key bar is hidden, so the
/// card shows the F-row itself.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fit {
    pub width: Pixels,
    pub has_frow: bool,
}

impl Fit {
    /// The fit of a card that spans the window between the side margins.
    #[must_use]
    pub fn in_window(t: &Theme, viewport_width: Pixels, has_fkey_bar: bool) -> Self {
        Self {
            width: viewport_width - (t.spacing.md + t.spacing.xxl) * 2.,
            has_frow: !has_fkey_bar,
        }
    }
}

#[derive(IntoElement)]
pub struct HintsDrawer {
    layer: Layer,
    fit: Fit,
    turn_off: TurnOffParts,
}

/// Estimated width of a key cap for its text, in px.
fn cap_width(text: &str) -> f32 {
    let chars = text.chars().count() as f32;
    (CAP_PAD + CAP_CHAR * chars).max(CAP_MIN_WIDTH)
}

/// Width of a layer's groups at `rows` rows per column, in px.
fn band_width(t: &Theme, shape: &[Shape], rows: usize) -> f32 {
    let column_gap = f32::from(t.spacing.xxl);
    let gap = f32::from(t.spacing.md);
    let groups: f32 = shape
        .iter()
        .map(|group| {
            let columns = group.entries.div_ceil(rows) as f32;
            let width = cap_width(&"M".repeat(group.cap_chars)) + gap + LABEL_WIDTH;
            columns * width + (columns - 1.).max(0.) * column_gap
        })
        .sum();
    groups + GROUP_GAP * shape.len().saturating_sub(1) as f32
}

/// The fewest rows (4 to 7) at which every peer layer fits the width; 7 when none does.
#[must_use]
pub fn rows_to_fit(t: &Theme, layer: &Layer, width: Pixels) -> usize {
    let width = f32::from(width);
    (FEWEST_ROWS..=MOST_ROWS)
        .find(|&rows| {
            layer
                .shapes
                .iter()
                .all(|shape| band_width(t, shape, rows) <= width)
        })
        .unwrap_or(MOST_ROWS)
}

/// True when the card shows the F-row: the bar is hidden and one modifier is held.
fn shows_frow(layer: &Layer, fit: Fit) -> bool {
    fit.has_frow && !layer.combo.name().contains('+')
}

/// Height of the card in px; the same for Ctrl, Shift and Alt at one fit.
#[must_use]
pub fn drawer_height(t: &Theme, layer: &Layer, fit: Fit) -> Pixels {
    let row = t.spacing.row_height;
    let body = if layer.is_empty() {
        row
    } else {
        px(TITLE_HEIGHT) + t.spacing.xs + row * rows_to_fit(t, layer, fit.width) as f32
    };
    let frow = if shows_frow(layer, fit) {
        t.spacing.lg + t.spacing.md + px(1. + FROW_HEIGHT)
    } else {
        px(0.)
    };
    t.spacing.lg + px(HEADER_HEIGHT) + t.spacing.md + body + frow + t.spacing.xl
}

impl HintsDrawer {
    #[must_use]
    pub fn new(layer: Layer, fit: Fit) -> Self {
        Self {
            layer,
            fit,
            turn_off: TurnOffParts {
                key: None,
                on_click: None,
                on_press: None,
            },
        }
    }

    #[must_use]
    pub fn settings_key(mut self, key: Option<&str>) -> Self {
        self.turn_off.key = key.map(|key| key.to_owned().into());
        self
    }

    #[must_use]
    pub fn on_turn_off(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.turn_off.on_click = Some(Rc::new(handler));
        self
    }

    /// Sets the handler for a left press on Turn off, before its click; the click may still be cancelled.
    #[must_use]
    pub fn on_turn_off_press(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.turn_off.on_press = Some(Rc::new(handler));
        self
    }
}

/// A key cap: mono text on the field surface, at least 22 px wide.
pub(super) fn cap(t: &Theme, key: SharedString) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(CAP_HEIGHT))
        .min_w(px(CAP_MIN_WIDTH))
        .px(t.spacing.xs)
        .rounded(t.radii.sm)
        .bg(t.surfaces.field)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .shadow(t.shadows.keycap.clone())
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text)
        .child(key)
}

/// The label colour: the mark colour for an entry that deletes for good.
pub(super) fn entry_color(t: &Theme, entry: &Entry) -> Hsla {
    if entry.is_danger {
        t.colors.mark
    } else {
        t.colors.text
    }
}

/// The short label, in the mark colour with a trash icon for a danger entry.
fn label(t: &Theme, entry: &Entry) -> Div {
    let color = entry_color(t, entry);
    let trash = entry
        .is_danger
        .then(|| icon(IconName::Trash, t.spacing.icon - px(3.), t.colors.mark));
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .min_w_0()
        .text_color(color)
        .child(div().min_w_0().truncate().child(entry.short.clone()))
        .children(trash)
}

/// The accessible name: "Ctrl+C or Ctrl+Insert Copy to clipboard".
fn spoken(layer: &Layer, entry: &Entry) -> String {
    let combo = layer.combo.name();
    let keys: Vec<String> = entry
        .keys
        .iter()
        .map(|key| format!("{combo}+{key}"))
        .collect();
    format!("{} {}", keys.join(" or "), entry.label)
}

fn entry(t: &Theme, layer: &Layer, entry: &Entry, cap_width: f32) -> impl IntoElement + use<> {
    let full = entry.label.clone();
    let key = entry.keys.first().cloned().unwrap_or_default();
    div()
        .id(SharedString::from(format!("hint-{}", entry.command)))
        .role(Role::Label)
        .aria_label(spoken(layer, entry))
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(t.spacing.row_height)
        .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
        .child(
            div()
                .flex()
                .flex_none()
                .justify_end()
                .w(px(cap_width))
                .child(cap(t, key)),
        )
        .child(label(t, entry).w(px(LABEL_WIDTH)))
}

fn group(t: &Theme, layer: &Layer, group: &Group, rows: usize) -> impl IntoElement + use<> {
    let widest = group
        .entries
        .iter()
        .map(|entry| cap_width(entry.keys.first().map_or("", |key| key.as_ref())));
    let cap_width = widest.fold(CAP_MIN_WIDTH, f32::max);
    let columns = group.entries.chunks(rows.max(1)).map(|chunk| {
        div()
            .flex()
            .flex_col()
            .flex_none()
            .children(chunk.iter().map(|shown| entry(t, layer, shown, cap_width)))
    });
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(
            div()
                .h(px(TITLE_HEIGHT))
                .mb(t.spacing.xs)
                .text_size(t.typography.tiny)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.muted)
                .child(group.name.clone()),
        )
        .child(div().flex().gap(t.spacing.xxl).children(columns))
}

fn body(t: &Theme, layer: &Layer, rows: usize, is_calm: bool) -> AnyElement {
    if layer.is_empty() {
        let text = format!("No shortcuts use {} in the file list.", layer.combo.name());
        return div()
            .flex()
            .items_center()
            .h(t.spacing.row_height)
            .text_color(t.colors.muted)
            .child(text)
            .into_any_element();
    }
    let groups = layer
        .groups
        .iter()
        .map(|shown| group(t, layer, shown, rows));
    let swap = SharedString::from(format!("modifier-hints-swap-{}", layer.combo.name()));
    let band = div()
        .id(SharedString::from(format!(
            "modifier-hints-{}",
            layer.combo.name()
        )))
        .flex()
        .gap(px(GROUP_GAP))
        .h(px(TITLE_HEIGHT) + t.spacing.xs + t.spacing.row_height * rows as f32)
        .children(groups);
    fade(t, band, swap, is_calm)
}

/// One F-row cell: the F-key cap and its label, faint when the key is unused here.
fn fkey(t: &Theme, layer: &Layer, number: u8) -> impl IntoElement + use<> {
    let name = format!("F{number}");
    let shown = layer
        .fkeys
        .iter()
        .find(|entry| entry.keys.first().is_some_and(|key| *key == name));
    let (cap_color, spoken) = match shown {
        Some(entry) => (t.colors.accent_text, spoken(layer, entry)),
        None => (
            t.colors.faint,
            format!("{}+{name} unused", layer.combo.name()),
        ),
    };
    div()
        .id(SharedString::from(format!("hint-{name}")))
        .role(Role::Label)
        .aria_label(spoken)
        .test_support()
        .flex()
        .flex_1()
        .flex_basis(px(0.))
        .items_center()
        .gap(t.spacing.sm)
        .min_w_0()
        .h(px(FROW_HEIGHT))
        .child(
            cap(t, name.into())
                .min_w(px(FKEY_CAP_WIDTH))
                .text_color(cap_color),
        )
        .children(shown.map(|entry| label(t, entry)))
}

fn frow(t: &Theme, layer: &Layer) -> impl IntoElement + use<> {
    div()
        .flex()
        .gap(t.spacing.sm)
        .mt(t.spacing.lg)
        .pt(t.spacing.md)
        .border_t_1()
        .border_color(t.colors.line)
        .children((1..=FKEY_COUNT).map(|number| fkey(t, layer, number)))
}

/// Fades an element in over `motion.fade`; no motion when the system asks for less.
fn fade<E: IntoElement + Styled + 'static>(
    t: &Theme,
    element: E,
    id: impl Into<ElementId>,
    is_calm: bool,
) -> AnyElement {
    if is_calm {
        return element.into_any_element();
    }
    let enter = Animation::new(t.motion.fade).with_easing(ease_out_cubic);
    element
        .with_animation(id, enter, |element, delta| element.opacity(delta))
        .into_any_element()
}

impl RenderOnce for HintsDrawer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let is_calm = cx.reduce_motion();
        let rows = rows_to_fit(&t, &self.layer, self.fit.width);
        let label = format!("{} shortcuts", self.layer.combo.name());
        let frow = shows_frow(&self.layer, self.fit).then(|| frow(&t, &self.layer));
        let card = div()
            .id("modifier-hints")
            .role(Role::Group)
            .aria_label(label)
            .test_support()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .pt(t.spacing.lg)
            .pb(t.spacing.xl)
            .px(t.spacing.xxl)
            .rounded_t(t.radii.sheet)
            .rounded_b(t.radii.card)
            .bg(t.surfaces.sheet)
            .shadow(t.shadows.up.clone())
            .text_size(t.typography.body)
            .text_color(t.colors.text)
            .child(header(&t, &self.layer, self.turn_off))
            .child(body(&t, &self.layer, rows, is_calm))
            .children(frow);
        fade(&t, card, "modifier-hints-enter", is_calm)
    }
}
