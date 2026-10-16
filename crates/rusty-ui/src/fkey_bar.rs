use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, Div, ElementId, FontWeight, InteractiveElement,
    IntoElement, Modifiers, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::FluentBuilder as _;
use crate::primitives::{IconName, Tooltip, icon};
use crate::tokens::{self, Motion, Theme};

const KEY_HEIGHT: f32 = 30.;
const CHIP_HEIGHT: f32 = 24.;
const CHIP_WIDTH: f32 = 28.;
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
    warning: Option<(usize, SharedString)>,
    is_pill_reserved: bool,
}

struct Layers {
    held: Option<KeyLayer>,
    base: Vec<SharedString>,
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
            is_pill_reserved: false,
        }
    }

    #[must_use]
    pub fn layers<L: Into<SharedString>>(
        mut self,
        held: Option<KeyLayer>,
        labels: impl IntoIterator<Item = [L; 4]>,
    ) -> Self {
        let base = labels.into_iter().map(|[base, ..]| base.into()).collect();
        self.layers = Some(Layers { held, base });
        self
    }

    #[must_use]
    pub fn warn(mut self, index: usize, spoken: impl Into<SharedString>) -> Self {
        self.warning = Some((index, spoken.into()));
        self
    }

    #[must_use]
    pub fn reserve_pill(mut self) -> Self {
        self.is_pill_reserved = true;
        self
    }
}

fn pill(t: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(KEY_HEIGHT))
        .rounded(t.radii.pill)
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
        .rounded(px(CHIP_HEIGHT / 2.))
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
                .px(t.spacing.lg)
                .when(is_lit, |pill| {
                    pill.bg(s.keycap_lit)
                        .border_color(s.keycap_lit)
                        .text_color(s.keycap_lit_label)
                        .font_weight(FontWeight::SEMIBOLD)
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
    motion: Option<LabelMotion>,
}

#[derive(Clone, Copy)]
struct LabelMotion {
    layer: Option<KeyLayer>,
    rise: f32,
}

fn label_view(text: Div, index: usize, motion: Option<LabelMotion>, t: &Theme) -> AnyElement {
    let Some(LabelMotion { layer, rise }) = motion else {
        return text.into_any_element();
    };
    let id = SharedString::from(format!(
        "fkey-label-{}-{index}",
        layer.map_or("base", KeyLayer::name)
    ));
    let animation = Animation::new(t.motion.fast).with_easing(ease_out_cubic);
    text.relative()
        .with_animation(id, animation, move |text, delta| {
            text.top(px(rise * (1. - delta))).opacity(delta)
        })
        .into_any_element()
}

fn cell(t: &Theme, cell: Cell) -> impl IntoElement {
    let Cell {
        index,
        key,
        label,
        fallback,
        warning,
        motion,
    } = cell;
    let is_dimmed = label.is_empty();
    let spoken: SharedString = match (&warning, is_dimmed) {
        (Some(warning), _) => format!("{key} {warning}").into(),
        (None, true) => key.clone(),
        (None, false) => format!("{key} {label}").into(),
    };
    let shown = if is_dimmed {
        fallback.unwrap_or_default()
    } else {
        label
    };
    let text = div().flex().items_center().gap(t.spacing.xs).child(shown);
    let text = match &warning {
        Some(_) => text.child(icon(
            IconName::Trash,
            t.spacing.icon - px(2.),
            t.colors.mark,
        )),
        None => text,
    };
    pill(t)
        .id(("fkey", index))
        .role(Role::Label)
        .aria_label(spoken)
        .test_support()
        .gap(t.spacing.sm)
        .pl(t.spacing.xxs)
        .pr(t.spacing.md)
        .when(is_dimmed, |pill| pill.opacity(DIMMED))
        .when_some(warning, |pill, warning| {
            pill.tooltip(move |window, cx| Tooltip::new(warning.clone()).build(window, cx))
        })
        .child(chip(t, key))
        .child(label_view(text, index, motion, t))
}

fn pill_slot() -> impl IntoElement {
    div()
        .id("fkey-pill-slot")
        .test_support()
        .flex_none()
        .w(px(PILL_SLOT))
        .h(px(KEY_HEIGHT))
}

impl RenderOnce for FKeyBar {
    fn render(self, _: &mut Window, cx: &mut gpui_kit::App) -> impl IntoElement {
        let t = tokens::current(cx);
        let rise = Motion::travel(px(LABEL_RISE), cx.reduce_motion()) / px(1.);
        let held = self.layers.as_ref().and_then(|layers| layers.held);
        let is_layered = self.layers.is_some();
        let base = self.layers.map(|layers| layers.base).unwrap_or_default();
        let warning = self.warning;
        let cells = self
            .keys
            .into_iter()
            .enumerate()
            .map(|(index, (key, label))| {
                let props = Cell {
                    index,
                    key,
                    label,
                    fallback: base.get(index).cloned().filter(|_| held.is_some()),
                    warning: warning
                        .clone()
                        .filter(|(at, _)| *at == index)
                        .map(|(_, spoken)| spoken),
                    motion: is_layered.then_some(LabelMotion { layer: held, rise }),
                };
                cell(t, props)
            });
        div()
            .id(self.id)
            .role(Role::Group)
            .aria_label("Function keys")
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(t.spacing.sm)
            .h(t.spacing.fkey_height)
            .px(t.spacing.md)
            .text_size(t.typography.body)
            .text_color(t.colors.text)
            .when(is_layered, |bar| bar.children(modifier_pills(t, held)))
            .children(cells)
            .when(self.is_pill_reserved, |bar| bar.child(pill_slot()))
    }
}

#[cfg(test)]
mod tests;
