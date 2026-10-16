use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, Div, ElementId, FontWeight, InteractiveElement,
    IntoElement, Modifiers, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::FluentBuilder as _;
use crate::tokens::{self, Motion, Theme};

const CAP_HEIGHT: f32 = 22.;
const KEY_HEIGHT: f32 = 30.;
const LABEL_RISE: f32 = 4.;
const PILL_SLOT: f32 = 220.;

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
    is_sized_to_keys: bool,
    layers: Option<Layers>,
    is_pill_reserved: bool,
}

struct Layers {
    held: Option<KeyLayer>,
    sizes: Vec<KeySize>,
}

#[derive(Clone, Default)]
struct KeySize {
    base: SharedString,
    grow: f32,
}

fn key_size(labels: [SharedString; 4]) -> KeySize {
    let length = |label: &SharedString| label.chars().count() as f32;
    let widest = labels.iter().map(length).fold(0., f32::max);
    let [base, ..] = labels;
    let grow = widest - length(&base) + 1.;
    KeySize { base, grow }
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
            is_sized_to_keys: false,
            layers: None,
            is_pill_reserved: false,
        }
    }

    #[must_use]
    pub fn sized_to_keys(mut self) -> Self {
        self.is_sized_to_keys = true;
        self
    }

    #[must_use]
    pub fn layers<L: Into<SharedString>>(
        mut self,
        held: Option<KeyLayer>,
        labels: impl IntoIterator<Item = [L; 4]>,
    ) -> Self {
        let sizes = labels
            .into_iter()
            .map(|key| key_size(key.map(Into::into)))
            .collect();
        self.layers = Some(Layers { held, sizes });
        self
    }

    #[must_use]
    pub fn reserve_pill(mut self) -> Self {
        self.is_pill_reserved = true;
        self
    }
}

fn cap(t: &Theme, key: SharedString, is_lit: bool) -> Div {
    let s = &t.surfaces;
    let base = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(CAP_HEIGHT))
        .min_w(px(CAP_HEIGHT))
        .px(t.spacing.xs)
        .rounded(t.radii.sm)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.key)
        .font_weight(FontWeight::MEDIUM)
        .border_1();
    if is_lit {
        base.bg(s.keycap_lit)
            .border_color(s.keycap_lit)
            .text_color(s.keycap_lit_label)
    } else {
        base.bg(s.keycap_face)
            .text_color(s.keycap_label)
            .border_color(s.card_edge)
            .shadow(t.shadows.keycap.clone())
    }
    .child(key)
}

fn modifier_caps(t: &Theme, held: Option<KeyLayer>) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .pr(t.spacing.md)
        .border_r_1()
        .border_color(t.surfaces.card_edge)
        .children(KeyLayer::ALL.into_iter().enumerate().map(|(index, layer)| {
            let is_lit = held == Some(layer);
            div()
                .id(("fkey-mod", index))
                .role(Role::Label)
                .aria_label(layer.name())
                .aria_selected(is_lit)
                .test_support()
                .child(cap(t, layer.name().into(), is_lit))
        }))
}

struct Cell {
    index: usize,
    key: SharedString,
    label: SharedString,
    is_sized: bool,
    look: Option<LabelLook>,
}

#[derive(Clone)]
struct LabelLook {
    layer: Option<KeyLayer>,
    rise: f32,
    size: KeySize,
}

fn label_view(t: &Theme, label: SharedString, index: usize, look: Option<LabelLook>) -> AnyElement {
    let Some(LabelLook { layer, rise, size }) = look else {
        return div().min_w_0().truncate().child(label).into_any_element();
    };
    let id = SharedString::from(format!(
        "fkey-label-{}-{index}",
        layer.map_or("base", KeyLayer::name)
    ));
    let animation = Animation::new(t.motion.fast).with_easing(ease_out_cubic);
    let shown = div()
        .absolute()
        .left_0()
        .right_0()
        .truncate()
        .child(label)
        .with_animation(id, animation, move |text, delta| {
            text.top(px(rise * (1. - delta))).opacity(delta)
        });
    div()
        .relative()
        .flex_1()
        .min_w_0()
        .child(div().min_w_0().truncate().opacity(0.).child(size.base))
        .child(shown)
        .into_any_element()
}

fn cell(t: &Theme, cell: Cell) -> impl IntoElement {
    let Cell {
        index,
        key,
        label,
        is_sized,
        look,
    } = cell;
    let grow = look.as_ref().map(|look| look.size.grow);
    let spoken = if label.is_empty() {
        key.clone()
    } else {
        format!("{key} {label}").into()
    };
    div()
        .id(("fkey", index))
        .role(Role::Label)
        .aria_label(spoken)
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .map(|cell| match (grow, is_sized) {
            (Some(grow), _) => cell.flex_auto().flex_grow(grow),
            (None, true) => cell.flex_auto(),
            (None, false) => cell.flex_1(),
        })
        .min_w_0()
        .h(px(KEY_HEIGHT))
        .px(t.spacing.xs)
        .rounded(t.radii.keycap)
        .child(cap(t, key, false))
        .child(label_view(t, label, index, look))
}

fn pill_slot() -> impl IntoElement {
    div()
        .id("fkey-pill-slot")
        .test_support()
        .flex_none()
        .w(px(PILL_SLOT))
        .h(px(KEY_HEIGHT))
}

fn key_row(t: &Theme, is_tinted: bool) -> Div {
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(t.spacing.xxs)
        .px(t.spacing.xxs)
        .rounded(t.radii.md)
        .when(is_tinted, |keys| keys.bg(t.surfaces.layer_tint))
}

impl RenderOnce for FKeyBar {
    fn render(self, _: &mut Window, cx: &mut gpui_kit::App) -> impl IntoElement {
        let t = tokens::current(cx);
        let rise = Motion::travel(px(LABEL_RISE), cx.reduce_motion()) / px(1.);
        let held = self.layers.as_ref().and_then(|layers| layers.held);
        let is_layered = self.layers.is_some();
        let sizes = self.layers.map(|layers| layers.sizes).unwrap_or_default();
        let is_sized = self.is_sized_to_keys;
        let cells = self
            .keys
            .into_iter()
            .enumerate()
            .map(|(index, (key, label))| {
                let look = is_layered.then(|| LabelLook {
                    layer: held,
                    rise,
                    size: sizes.get(index).cloned().unwrap_or_default(),
                });
                let props = Cell {
                    index,
                    key,
                    label,
                    is_sized,
                    look,
                };
                cell(t, props)
            });
        let keys = key_row(t, held.is_some()).children(cells);
        div()
            .id(self.id)
            .role(Role::Group)
            .aria_label("Function keys")
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .h(t.spacing.fkey_height)
            .px(t.spacing.md)
            .text_size(t.typography.small)
            .text_color(t.colors.text)
            .when(is_layered, |bar| bar.child(modifier_caps(t, held)))
            .child(keys)
            .when(self.is_pill_reserved, |bar| bar.child(pill_slot()))
    }
}

#[cfg(test)]
mod tests;
