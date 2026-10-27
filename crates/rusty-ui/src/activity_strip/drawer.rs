use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{RenderOnce, Role, ScrollHandle};
use rusty_l10n::{t, tr};

use super::button;
use super::stops::{Stop, acts_of};
use super::{FILL_ALPHA, StopHandler, fade_in, fill_color, glyph, key_of};
use crate::activity::{ActivityItem, ActivityKind, percent};
use crate::format;
use crate::prelude::*;
use crate::primitives::{Kbd, icon};
use crate::tokens::{self, Theme};

const BAR_HEIGHT: f32 = 4.;
const MAX_HEIGHT: f32 = 360.;
const PERCENT_WIDTH: f32 = 44.;

#[derive(IntoElement)]
pub struct ActivityDrawer {
    items: Vec<ActivityItem>,
    selected: Option<Stop>,
    is_focused: bool,
    scroll: Option<ScrollHandle>,
    on_stop: Option<StopHandler>,
}

impl ActivityDrawer {
    #[must_use]
    pub fn new(items: Vec<ActivityItem>) -> Self {
        Self {
            items,
            selected: None,
            is_focused: false,
            scroll: None,
            on_stop: None,
        }
    }

    #[must_use]
    pub fn selected(mut self, stop: Option<Stop>) -> Self {
        self.selected = stop;
        self
    }

    #[must_use]
    pub fn focused(mut self, is_focused: bool) -> Self {
        self.is_focused = is_focused;
        self
    }

    #[must_use]
    pub fn scroll(mut self, handle: &ScrollHandle) -> Self {
        self.scroll = Some(handle.clone());
        self
    }

    #[must_use]
    pub fn on_stop(mut self, handler: impl Fn(Stop, &mut Window, &mut App) + 'static) -> Self {
        self.on_stop = Some(Rc::new(handler));
        self
    }
}

fn bar(t: &Theme, item: &ActivityItem, progress: f32) -> impl IntoElement + use<> {
    div()
        .h(px(BAR_HEIGHT))
        .w_full()
        .rounded(px(BAR_HEIGHT / 2.))
        .overflow_hidden()
        .bg(t.surfaces.progress_track)
        .child(div().h_full().w(relative(progress)).bg(if item.paused {
            t.colors.faint
        } else {
            t.surfaces.progress_fill
        }))
}

fn spoken(item: &ActivityItem) -> String {
    let parts = [Some(item.title.to_string())]
        .into_iter()
        .chain([item.progress.map(percent)])
        .chain([item.detail.as_ref().map(ToString::to_string)]);
    parts.flatten().collect::<Vec<_>>().join(", ")
}

impl ActivityDrawer {
    fn texts(t: &Theme, item: &ActivityItem) -> Div {
        let detail = item.detail.clone().map(|detail| {
            div()
                .min_w_0()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(detail)
        });
        let progress = item
            .progress
            .filter(|_| item.kind != ActivityKind::Failure)
            .map(|progress| bar(t, item, progress));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(t.spacing.xxs)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(item.title.clone()),
            )
            .children(detail)
            .children(progress)
    }

    fn row(&self, t: &Theme, item: &ActivityItem) -> impl IntoElement + use<> {
        let key = key_of(item.id);
        let (name, color) = glyph(t, item);
        let is_selected = self.selected == Some(Stop::Row(item.id));
        let percent = item
            .progress
            .filter(|_| item.kind != ActivityKind::Failure)
            .map(|progress| {
                div()
                    .flex_none()
                    .w(px(PERCENT_WIDTH))
                    .font_family(t.typography.numbers_font.clone())
                    .font_features(t.typography.numbers_font_features.clone())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(percent(progress))
            });
        let buttons = acts_of(item).into_iter().map(|act| {
            let face = button::act_button(item, act, self.selected, self.is_focused);
            button::render(t, face, Stop::Button(item.id, act), self.on_stop.as_ref())
        });
        let tint = match item.kind {
            ActivityKind::Failure => t.colors.warn.opacity(FILL_ALPHA),
            _ => item
                .progress
                .map_or(t.surfaces.field, |_| fill_color(t, item)),
        };
        let row = div()
            .id(("activity-row", key))
            .role(Role::Label)
            .aria_label(spoken(item))
            .aria_selected(is_selected)
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.md)
            .px(t.spacing.md)
            .py(t.spacing.sm)
            .rounded(t.radii.lg)
            .border_1()
            .border_color(t.surfaces.card_edge)
            .bg(tint)
            .child(icon(name, t.spacing.icon, color))
            .child(Self::texts(t, item))
            .children(percent)
            .children(buttons);
        button::ring(t, row, is_selected)
    }

    fn header(t: &Theme, count: usize) -> impl IntoElement + use<> {
        let items = format::bytes(u64::try_from(count).unwrap_or(u64::MAX));
        div()
            .flex()
            .items_center()
            .gap(t.spacing.md)
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.colors.text)
                    .child(t!("activity-drawer-title", count = count, items = items)),
            )
            .child(div().flex_1())
            .children(Kbd::new("activity-drawer-esc", "escape"))
            .child(tr("activity-drawer-close"))
    }
}

impl RenderOnce for ActivityDrawer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let rows: Vec<AnyElement> = self
            .items
            .iter()
            .map(|item| self.row(&t, item).into_any_element())
            .collect();
        let drawer = div()
            .id("activity-drawer")
            .role(Role::Group)
            .aria_label(tr("activity-strip"))
            .test_support()
            .relative()
            .flex()
            .flex_col()
            .gap(t.spacing.sm)
            .p(t.spacing.lg)
            .max_h(px(MAX_HEIGHT))
            .overflow_y_scroll()
            .when_some(self.scroll.clone(), |drawer, handle| {
                drawer.track_scroll(&handle)
            })
            .rounded(t.radii.card)
            .border_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.surfaces.job_card)
            .shadow(t.shadows.menu.clone())
            .occlude()
            .child(Self::header(&t, self.items.len()))
            .children(rows);
        fade_in(drawer, "activity-drawer-enter", &t)
    }
}
