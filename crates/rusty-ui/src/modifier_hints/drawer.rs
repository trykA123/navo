use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt, RenderOnce, Role};

use super::layer::{Entry, Layer};
use crate::prelude::*;
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens::{self, Theme};

const HEADER_HEIGHT: f32 = 24.;
const CHIP_HEIGHT: f32 = 22.;
const CAP_HEIGHT: f32 = 20.;
const CAP_WIDTH: f32 = 22.;
const RISE: f32 = 12.;

pub type TurnOff = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct HintsDrawer {
    layer: Layer,
    settings_key: Option<SharedString>,
    on_turn_off: Option<TurnOff>,
}

#[must_use]
pub fn drawer_height(t: &Theme, layer: &Layer) -> Pixels {
    let row = t.spacing.row_height;
    let body = if layer.is_empty() {
        row
    } else {
        t.spacing.xxl + t.spacing.sm + row * layer.rows() as f32
    };
    t.spacing.lg + px(HEADER_HEIGHT) + t.spacing.lg + body + t.spacing.xl
}

impl HintsDrawer {
    #[must_use]
    pub fn new(layer: Layer) -> Self {
        Self {
            layer,
            settings_key: None,
            on_turn_off: None,
        }
    }

    #[must_use]
    pub fn settings_key(mut self, key: Option<&str>) -> Self {
        self.settings_key = key.map(|key| key.to_owned().into());
        self
    }

    #[must_use]
    pub fn on_turn_off(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_turn_off = Some(Rc::new(handler));
        self
    }
}

fn summary(layer: &Layer) -> String {
    let count = layer.count();
    let noun = if count == 1 { "shortcut" } else { "shortcuts" };
    let more = match layer.more.len() {
        0 => String::new(),
        extra => format!(" · add Shift for {extra} more"),
    };
    format!("{count} {noun} in the file list{more} · release to hide")
}

fn chip(t: &Theme, combo: SharedString) -> impl IntoElement {
    div()
        .id("modifier-hints-combo")
        .role(Role::Label)
        .aria_label(combo.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .h(px(CHIP_HEIGHT))
        .px(t.spacing.md)
        .rounded(t.radii.full)
        .bg(t.surfaces.keycap_lit)
        .text_color(t.surfaces.keycap_lit_label)
        .font_family(t.typography.mono_font.clone())
        .font_weight(FontWeight::SEMIBOLD)
        .child(combo)
}

fn turn_off(t: &Theme, key: Option<SharedString>, handler: Option<TurnOff>) -> impl IntoElement {
    let kbd = key.and_then(|key| Kbd::new("modifier-hints-off-key", &key));
    let shortcut = kbd.as_ref().map(Kbd::text);
    let hover = (t.surfaces.field, t.colors.text);
    div()
        .id("modifier-hints-off")
        .role(Role::Button)
        .aria_label("Turn off modifier hints")
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(HEADER_HEIGHT))
        .pl(t.spacing.md)
        .pr(t.spacing.xs)
        .rounded(t.radii.full)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .text_color(t.colors.muted)
        .cursor_pointer()
        .hover(move |button| button.bg(hover.0).text_color(hover.1))
        .when_some(handler, |button, handler| {
            button.on_click(move |_, window, cx| handler(window, cx))
        })
        .child(icon(
            IconName::EyeOff,
            t.spacing.icon - px(2.),
            t.colors.muted,
        ))
        .child("Turn off")
        .children(kbd)
        .when_some(shortcut, |button, keys| button.aria_keyshortcuts(keys))
}

fn header(t: &Theme, drawer: &mut HintsDrawer) -> Div {
    let combo: SharedString = drawer.layer.combo.name().into();
    let text = (!drawer.layer.is_empty()).then(|| summary(&drawer.layer));
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(HEADER_HEIGHT))
        .mb(t.spacing.lg)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(chip(t, combo))
        .children(text.map(|text| div().min_w_0().truncate().child(text)))
        .child(div().flex_1())
        .child(turn_off(
            t,
            drawer.settings_key.take(),
            drawer.on_turn_off.take(),
        ))
}

fn cap(t: &Theme, key: SharedString) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(CAP_HEIGHT))
        .min_w(px(CAP_WIDTH))
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

struct Column<'a> {
    title: SharedString,
    entries: &'a [Entry],
    is_more: bool,
}

fn row(t: &Theme) -> Div {
    div().flex().items_center().h(t.spacing.row_height)
}

fn entry_label(t: &Theme, entry: &Entry, is_more: bool) -> impl IntoElement + use<> {
    let prefix = if is_more { "hint-more" } else { "hint" };
    let spoken = format!("{} {}", entry.keys.join(" "), entry.label);
    row(t)
        .id(SharedString::from(format!("{prefix}-{}", entry.command)))
        .role(Role::Label)
        .aria_label(spoken)
        .test_support()
        .min_w_0()
        .child(div().min_w_0().truncate().child(entry.label.clone()))
}

fn column(t: &Theme, column: &Column) -> impl IntoElement + use<> {
    let (title_color, text_color) = if column.is_more {
        (t.colors.accent_text, t.colors.muted)
    } else {
        (t.colors.muted, t.colors.text)
    };
    let keys = column.entries.iter().map(|entry| {
        row(t)
            .gap(t.spacing.xxs)
            .children(entry.keys.iter().map(|key| cap(t, key.clone())))
    });
    let labels = column
        .entries
        .iter()
        .map(|entry| entry_label(t, entry, column.is_more));
    div()
        .flex()
        .flex_col()
        .flex_1()
        .flex_basis(px(0.))
        .min_w_0()
        .child(
            div()
                .h(t.spacing.xxl)
                .mb(t.spacing.sm)
                .text_size(t.typography.tiny)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(title_color)
                .child(column.title.clone()),
        )
        .child(
            div()
                .flex()
                .gap(t.spacing.md)
                .text_color(text_color)
                .child(div().flex().flex_col().flex_none().children(keys))
                .child(div().flex().flex_col().flex_1().min_w_0().children(labels)),
        )
}

fn body(t: &Theme, layer: &Layer) -> AnyElement {
    if layer.is_empty() {
        let text = format!("No shortcuts use {} in the file list.", layer.combo.name());
        return row(t)
            .text_color(t.colors.muted)
            .child(text)
            .into_any_element();
    }
    let groups = layer.groups.iter().map(|group| Column {
        title: group.name.to_uppercase().into(),
        entries: &group.entries,
        is_more: false,
    });
    let more = (!layer.more.is_empty()).then(|| Column {
        title: "+ SHIFT".into(),
        entries: &layer.more,
        is_more: true,
    });
    div()
        .flex()
        .gap(t.spacing.xxl + t.spacing.md)
        .children(groups.chain(more).map(|shown| column(t, &shown)))
        .into_any_element()
}

impl RenderOnce for HintsDrawer {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let label = format!("{} shortcuts", self.layer.combo.name());
        let card = div()
            .id("modifier-hints")
            .role(Role::Group)
            .aria_label(label)
            .test_support()
            .occlude()
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
            .child(header(&t, &mut self))
            .child(body(&t, &self.layer));
        if cx.reduce_motion() {
            return card.into_any_element();
        }
        let rise = px(RISE);
        let enter = Animation::new(t.motion.fast).with_easing(ease_out_cubic);
        card.with_animation("modifier-hints-enter", enter, move |card, delta| {
            card.top(rise * (1. - delta)).opacity(delta)
        })
        .into_any_element()
    }
}
