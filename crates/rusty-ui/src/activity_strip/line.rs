use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{RenderOnce, Role, Stateful};
use rusty_l10n::{t, tr};

use super::button::{self, Face};
use super::stops::{Stop, acts_of};
use super::{FILL_ALPHA, HoverHandler, StopHandler, fade_away, fade_in, fill_color, glyph, key_of};
use crate::activity::{ActivityItem, ActivityKind, percent};
use crate::prelude::*;
use crate::primitives::icon;
use crate::tokens::{self, Theme};

const DETAIL_SHRINK: f32 = 100.;
const CHIPS_SHRINK: f32 = 1000.;
const CHIPS_SHARE: f32 = 0.45;
const MIN_CHIP: f32 = 40.;

pub type ChipsShown = Rc<Cell<Option<usize>>>;

#[derive(IntoElement)]
pub struct ActivityStrip {
    items: Vec<ActivityItem>,
    selected: Option<Stop>,
    all_key: Option<SharedString>,
    is_leaving: bool,
    is_focused: bool,
    chips_shown: Option<ChipsShown>,
    on_stop: Option<StopHandler>,
    on_hover: Option<HoverHandler>,
}

impl ActivityStrip {
    #[must_use]
    pub fn new(items: Vec<ActivityItem>) -> Self {
        Self {
            items,
            selected: None,
            all_key: None,
            is_leaving: false,
            is_focused: false,
            chips_shown: None,
            on_stop: None,
            on_hover: None,
        }
    }

    #[must_use]
    pub fn selected(mut self, stop: Option<Stop>) -> Self {
        self.selected = stop;
        self
    }

    #[must_use]
    pub fn all_key(mut self, key: Option<&str>) -> Self {
        self.all_key = key.map(|key| key.to_owned().into());
        self
    }

    #[must_use]
    pub fn focused(mut self, is_focused: bool) -> Self {
        self.is_focused = is_focused;
        self
    }

    #[must_use]
    pub fn chips_shown(mut self, shown: ChipsShown) -> Self {
        self.chips_shown = Some(shown);
        self
    }

    #[must_use]
    pub fn leaving(mut self) -> Self {
        self.is_leaving = true;
        self
    }

    #[must_use]
    pub fn on_stop(mut self, handler: impl Fn(Stop, &mut Window, &mut App) + 'static) -> Self {
        self.on_stop = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn on_hover(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover = Some(Rc::new(handler));
        self
    }
}

fn label(id: impl Into<ElementId>, text: SharedString) -> ObservedElement<Stateful<Div>> {
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(text.clone())
        .test_support()
        .min_w_0()
        .truncate()
        .child(text)
}

pub(super) fn fill(t: &Theme, item: &ActivityItem) -> Option<impl IntoElement + use<>> {
    let base = div()
        .id("activity-fill")
        .test_support()
        .absolute()
        .left_0()
        .top_0()
        .bottom_0();
    match (item.kind, item.progress) {
        (ActivityKind::Failure, _) => Some(base.right_0().bg(t.colors.warn.opacity(FILL_ALPHA))),
        (_, Some(progress)) => Some(base.w(relative(progress)).bg(fill_color(t, item))),
        (_, None) => None,
    }
}

impl ActivityStrip {
    fn content(&self, t: &Theme, item: &ActivityItem) -> Div {
        let key = key_of(item.id);
        let (name, color) = glyph(t, item);
        let percent = item
            .progress
            .filter(|_| item.kind != ActivityKind::Failure)
            .map(|progress| {
                label(("activity-percent", key), percent(progress).into())
                    .flex_none()
                    .font_family(t.typography.numbers_font.clone())
                    .font_features(t.typography.numbers_font_features.clone())
                    .font_weight(FontWeight::SEMIBOLD)
            });
        let detail = item.detail.clone().map(|detail| {
            label(("activity-detail", key), detail)
                .flex_shrink(DETAIL_SHRINK)
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
        });
        let buttons = acts_of(item).into_iter().map(|act| {
            let face = button::act_button(item, act, self.selected, self.is_focused);
            button::render(t, face, Stop::Button(item.id, act), self.on_stop.as_ref())
        });
        div()
            .relative()
            .flex()
            .flex_grow(1.)
            .flex_shrink(1.)
            .min_w_0()
            .items_center()
            .gap(t.spacing.md)
            .child(icon(name, t.spacing.icon, color))
            .child(
                label(("activity-title", key), item.title.clone()).font_weight(FontWeight::MEDIUM),
            )
            .children(percent)
            .children(detail)
            .children(buttons)
    }

    fn chips(&self, t: &Theme) -> Div {
        let chips = self.items.iter().skip(1).map(|item| {
            let face = button::chip(item, self.selected);
            let chip = div().relative().min_w_0().child(button::render(
                t,
                face,
                Stop::Chip(item.id),
                self.on_stop.as_ref(),
            ));
            fade_in(chip, ("activity-chip-enter", key_of(item.id)), t)
        });
        let shown = self.chips_shown.clone();
        div()
            .flex()
            .flex_wrap()
            .flex_shrink(CHIPS_SHRINK)
            .min_w_0()
            .max_w(relative(CHIPS_SHARE))
            .h(px(button::BUTTON_HEIGHT))
            .overflow_hidden()
            .items_center()
            .gap(t.spacing.xs)
            .children(chips)
            .when_some(shown, |chips, shown| {
                chips.on_children_prepainted(move |bounds, _, _| {
                    shown.set(Some(fitting(&bounds)));
                })
            })
    }

    fn all_button(&self, t: &Theme) -> Option<impl IntoElement + use<>> {
        let count = self.items.len();
        (count >= 2).then(|| {
            let face = Face {
                id: "activity-all".into(),
                label: t!("activity-all", count = count).into(),
                glyph: None,
                key: self.all_key.clone(),
                is_selected: self.selected == Some(Stop::All),
                is_chip: false,
            };
            button::render(t, face, Stop::All, self.on_stop.as_ref())
        })
    }
}

fn fitting(bounds: &[Bounds<Pixels>]) -> usize {
    let Some(first) = bounds.first() else {
        return 0;
    };
    bounds
        .iter()
        .take_while(|chip| {
            chip.top() < first.top() + chip.size.height / 2. && chip.size.width >= px(MIN_CHIP)
        })
        .count()
}

impl ActivityStrip {
    fn band(&self, t: &Theme, primary: &ActivityItem) -> impl IntoElement + use<> {
        let content = self.content(t, primary);
        let is_ringed = self.is_focused && self.selected.is_none();
        let band = div()
            .id("activity-band")
            .aria_selected(is_ringed)
            .test_support()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center()
            .gap(t.spacing.md)
            .px(t.spacing.md)
            .rounded(t.radii.lg)
            .border_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.surfaces.card)
            .overflow_hidden()
            .children(fill(t, primary))
            .child(fade_in(content, ("activity-line", key_of(primary.id)), t))
            .child(self.chips(t))
            .children(self.all_button(t));
        button::ring(t, band, is_ringed)
    }
}

impl RenderOnce for ActivityStrip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let Some(primary) = self.items.first().cloned() else {
            return div().into_any_element();
        };
        let band = self.band(&t, &primary);
        let id = if self.is_leaving {
            "activity-strip-leaving"
        } else {
            "activity-strip"
        };
        let row = div()
            .id(id)
            .role(Role::Group)
            .aria_label(tr("activity-strip"))
            .test_support()
            .relative()
            .flex()
            .flex_none()
            .h(t.spacing.fkey_height)
            .px(t.spacing.xs * 2.)
            .py(t.spacing.xs)
            .text_size(t.typography.body)
            .text_color(t.colors.text)
            .when_some(self.on_hover, |row, handler| {
                row.on_hover(move |is_hovered, window, cx| handler(*is_hovered, window, cx))
            })
            .child(band);
        if self.is_leaving {
            fade_away(row, "activity-strip-leave", &t)
        } else {
            fade_in(row, "activity-strip-enter", &t)
        }
    }
}
