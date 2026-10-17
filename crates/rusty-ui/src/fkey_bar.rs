use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{
    Animation, AnimationExt, AnyElement, Div, ElementId, FontWeight, InteractiveElement,
    IntoElement, Modifiers, ParentElement, Pixels, RenderOnce, Role, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::FluentBuilder as _;
use crate::primitives::{IconName, Tooltip, icon};
use crate::tokens::{self, Motion, Theme};

const KEY_HEIGHT: f32 = 24.;
const CHIP_HEIGHT: f32 = 18.;
const CHIP_WIDTH: f32 = 24.;
const LABEL_RISE: f32 = 4.;
const PILL_SLOT: f32 = 220.;
const DIMMED: f32 = 0.45;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyLayer {
    Shift,
    Ctrl,
    Alt,
}

impl KeyLayer {
    pub const ALL: [Self; 3] = [Self::Shift, Self::Ctrl, Self::Alt];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Shift => "Shift",
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
        }
    }

    #[must_use]
    pub fn held(modifiers: &Modifiers) -> Option<Self> {
        if modifiers.platform || modifiers.function {
            return None;
        }
        match (modifiers.shift, modifiers.control, modifiers.alt) {
            (true, false, false) => Some(Self::Shift),
            (false, true, false) => Some(Self::Ctrl),
            (false, false, true) => Some(Self::Alt),
            _ => None,
        }
    }
}

#[derive(IntoElement)]
pub struct FKeyBar {
    id: ElementId,
    keys: Vec<(SharedString, SharedString)>,
    layers: Option<Layers>,
    warning: Option<Warning>,
    pill_width: Option<Pixels>,
    left: Option<AnyElement>,
    right: Option<AnyElement>,
}

struct Layers {
    held: Option<KeyLayer>,
    labels: Vec<[SharedString; 4]>,
}

#[derive(Clone)]
struct Warning {
    layer: Option<KeyLayer>,
    index: usize,
    spoken: SharedString,
}

impl FKeyBar {
    pub fn new(
        id: impl Into<ElementId>,
        keys: impl IntoIterator<Item = (impl Into<SharedString>, impl Into<SharedString>)>,
    ) -> Self {
        Self {
            id: id.into(),
            keys: keys
                .into_iter()
                .map(|(key, label)| (key.into(), label.into()))
                .collect(),
            layers: None,
            warning: None,
            pill_width: None,
            left: None,
            right: None,
        }
    }

    #[must_use]
    pub fn layers<L: Into<SharedString>>(
        mut self,
        held: Option<KeyLayer>,
        labels: impl IntoIterator<Item = [L; 4]>,
    ) -> Self {
        let labels = labels.into_iter().map(|key| key.map(Into::into)).collect();
        self.layers = Some(Layers { held, labels });
        self
    }

    #[must_use]
    pub fn warn(self, layer: KeyLayer, index: usize, spoken: impl Into<SharedString>) -> Self {
        self.warn_at(Some(layer), index, spoken)
    }

    #[must_use]
    pub fn warn_at(
        mut self,
        layer: Option<KeyLayer>,
        index: usize,
        spoken: impl Into<SharedString>,
    ) -> Self {
        self.warning = Some(Warning {
            layer,
            index,
            spoken: spoken.into(),
        });
        self
    }

    #[must_use]
    pub fn reserve_pill(self) -> Self {
        self.reserve_pill_width(px(PILL_SLOT))
    }

    #[must_use]
    pub fn reserve_pill_width(mut self, width: Pixels) -> Self {
        self.pill_width = Some(width);
        self
    }

    #[must_use]
    pub fn left_slot(mut self, content: impl IntoElement) -> Self {
        self.left = Some(content.into_any_element());
        self
    }

    #[must_use]
    pub fn right_slot(mut self, content: impl IntoElement) -> Self {
        self.right = Some(content.into_any_element());
        self
    }
}

fn pill(t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .h(px(KEY_HEIGHT))
        .rounded(t.radii.full)
        .bg(t.surfaces.card)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .whitespace_nowrap()
}

fn chip(t: &Theme, key: SharedString) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(CHIP_HEIGHT))
        .min_w(px(CHIP_WIDTH))
        .px(t.spacing.xs)
        .rounded(t.radii.full)
        .bg(t.surfaces.field)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.accent_text)
        .child(key)
}

fn modifier_pills(t: &Theme, held: Option<KeyLayer>) -> Vec<AnyElement> {
    let s = &t.surfaces;
    KeyLayer::ALL
        .into_iter()
        .enumerate()
        .map(|(index, layer)| {
            let is_lit = held == Some(layer);
            pill(t)
                .id(("fkey-mod", index))
                .role(Role::Label)
                .aria_label(layer.name())
                .aria_selected(is_lit)
                .test_support()
                .flex_none()
                .px(t.spacing.md)
                .text_size(t.typography.small)
                .when(is_lit, |pill| {
                    pill.bg(s.keycap_lit)
                        .border_color(s.keycap_lit)
                        .text_color(s.keycap_lit_label)
                })
                .child(layer.name())
                .into_any_element()
        })
        .collect()
}

struct Cell {
    index: usize,
    key: SharedString,
    label: SharedString,
    fallback: Option<SharedString>,
    warning: Option<SharedString>,
    sizers: Option<Sizers>,
    motion: Option<LabelMotion>,
}

#[derive(Clone)]
struct Sizers {
    labels: [SharedString; 4],
    icon_in: Option<usize>,
}

#[derive(Clone, Copy)]
struct LabelMotion {
    layer: Option<KeyLayer>,
    rise: f32,
}

fn warning_icon(t: &Theme) -> impl IntoElement {
    icon(IconName::Trash, t.spacing.icon - px(2.), t.colors.mark)
}

fn label_text(t: &Theme, text: SharedString, has_icon: bool) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .min_w_0()
        .child(div().min_w_0().truncate().child(text))
        .when(has_icon, |d| d.child(warning_icon(t)))
}

fn sizer(t: &Theme, sizers: Sizers) -> Div {
    let Sizers { labels, icon_in } = sizers;
    div()
        .flex()
        .flex_col()
        .h(px(0.))
        .overflow_hidden()
        .children(
            labels
                .into_iter()
                .enumerate()
                .map(|(layer, text)| label_text(t, text, icon_in == Some(layer)).flex_none()),
        )
}

fn animated(text: Div, index: usize, motion: Option<LabelMotion>, t: &Theme) -> AnyElement {
    let Some(LabelMotion { layer, rise }) = motion else {
        return text.into_any_element();
    };
    let id = SharedString::from(format!(
        "fkey-label-{}-{index}",
        layer.map_or("base", KeyLayer::name)
    ));
    let animation = Animation::new(t.motion.fast).with_easing(ease_out_cubic);
    text.with_animation(id, animation, move |text, delta| {
        text.top(px(rise * (1. - delta))).opacity(delta)
    })
    .into_any_element()
}

fn label_area(t: &Theme, cell: &Cell, shown: Div) -> AnyElement {
    let Some(sizers) = cell.sizers.clone() else {
        return animated(shown, cell.index, cell.motion, t);
    };
    let overlay = div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .flex()
        .items_center()
        .child(shown);
    div()
        .relative()
        .h_full()
        .min_w_0()
        .overflow_hidden()
        .child(sizer(t, sizers))
        .child(animated(overlay, cell.index, cell.motion, t))
        .into_any_element()
}

fn cell(t: &Theme, cell: Cell) -> impl IntoElement {
    let is_dimmed = cell.label.is_empty();
    let key = cell.key.clone();
    let spoken: SharedString = match (&cell.warning, is_dimmed) {
        (Some(warning), _) => format!("{key} {warning}").into(),
        (None, true) => key.clone(),
        (None, false) => format!("{key} {}", cell.label).into(),
    };
    let text = if is_dimmed {
        cell.fallback.clone().unwrap_or_default()
    } else {
        cell.label.clone()
    };
    let shown = label_text(t, text, cell.warning.is_some());
    let area = label_area(t, &cell, shown);
    pill(t)
        .id(("fkey", cell.index))
        .role(Role::Label)
        .aria_label(spoken)
        .test_support()
        .flex_initial()
        .min_w_0()
        .gap(t.spacing.sm)
        .pl(t.spacing.xxs)
        .pr(t.spacing.sm)
        .when(is_dimmed, |pill| pill.opacity(DIMMED))
        .when_some(cell.warning, |pill, warning| {
            pill.tooltip(move |window, cx| Tooltip::new(warning.clone()).build(window, cx))
        })
        .child(chip(t, key))
        .child(area)
}

fn pill_area(width: Pixels) -> ObservedElement<Stateful<Div>> {
    div()
        .id("fkey-pill-slot")
        .test_support()
        .flex_none()
        .w(width)
        .h(px(KEY_HEIGHT))
}

fn side_slot(
    id: &'static str,
    content: Option<AnyElement>,
    min_width: Option<Pixels>,
) -> impl IntoElement {
    let slot = div()
        .id(id)
        .test_support()
        .flex()
        .flex_1()
        .flex_basis(px(0.))
        .min_w_0()
        .items_center()
        .overflow_hidden();
    match min_width {
        Some(width) => slot.min_w(width).child(pill_area(width).children(content)),
        None => slot.children(content),
    }
}

struct Frame {
    held: Option<KeyLayer>,
    labels: Option<Vec<[SharedString; 4]>>,
    warning: Option<Warning>,
    rise: f32,
}

impl Frame {
    fn cell(&self, index: usize, key: SharedString, label: SharedString) -> Cell {
        let held = self.held;
        let labels = self.labels.as_ref().and_then(|all| all.get(index)).cloned();
        let warning = self.warning.clone().filter(|w| w.index == index);
        Cell {
            index,
            key,
            label,
            fallback: labels
                .clone()
                .map(|[base, ..]| base)
                .filter(|_| held.is_some()),
            sizers: labels.map(|labels| Sizers {
                labels,
                icon_in: warning.as_ref().map(|w| layer_slot(w.layer)),
            }),
            warning: warning.filter(|w| held == w.layer).map(|w| w.spoken),
            motion: self.labels.is_some().then_some(LabelMotion {
                layer: held,
                rise: self.rise,
            }),
        }
    }
}

fn layer_slot(layer: Option<KeyLayer>) -> usize {
    match layer {
        None => 0,
        Some(KeyLayer::Shift) => 1,
        Some(KeyLayer::Ctrl) => 2,
        Some(KeyLayer::Alt) => 3,
    }
}

impl RenderOnce for FKeyBar {
    fn render(self, _: &mut Window, cx: &mut gpui_kit::App) -> impl IntoElement {
        let t = tokens::current(cx);
        let is_layered = self.layers.is_some();
        let held = self.layers.as_ref().and_then(|layers| layers.held);
        let frame = Frame {
            held,
            labels: self.layers.map(|layers| layers.labels),
            warning: self.warning,
            rise: Motion::travel(px(LABEL_RISE), cx.reduce_motion()) / px(1.),
        };
        let cells = self
            .keys
            .into_iter()
            .enumerate()
            .map(|(index, (key, label))| cell(t, frame.cell(index, key, label)));
        let group = div()
            .flex()
            .flex_shrink(1.)
            .min_w_0()
            .items_center()
            .gap(t.spacing.xs)
            .when(is_layered, |group| group.children(modifier_pills(t, held)))
            .children(cells);
        div()
            .id(self.id)
            .role(Role::Group)
            .aria_label("Function keys")
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .overflow_hidden()
            .h(t.spacing.fkey_height)
            .px(t.spacing.md)
            .text_size(t.typography.body)
            .text_color(t.colors.text)
            .child(side_slot("fkey-slot-left", self.left, None))
            .child(group)
            .child(side_slot("fkey-slot-right", self.right, self.pill_width))
    }
}

#[cfg(test)]
mod tests;
