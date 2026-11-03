use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use navo_l10n::tr;

use super::tone;
use crate::browser::{BarView, CompareFilter, FILTERS};
use crate::file_list::row::inset_ring;
use crate::primitives::{Button, ButtonVariant, IconName, Kbd};
use crate::tokens::{self, Theme};

/// What the user did on the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarEvent {
    Filter(CompareFilter),
    Leave,
    /// Go on with a walk that stopped at the network limit.
    Continue,
}

type BarHandler = Rc<dyn Fn(&BarEvent, &mut Window, &mut App)>;

/// The filter row and the totals line above the two panes.
#[derive(IntoElement)]
pub struct CompareBar {
    id: ElementId,
    view: BarView,
    on_event: Option<BarHandler>,
}

impl CompareBar {
    pub fn new(id: impl Into<ElementId>, view: BarView) -> Self {
        Self {
            id: id.into(),
            view,
            on_event: None,
        }
    }

    #[must_use]
    pub fn on_event(
        mut self,
        handler: impl Fn(&BarEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }

    fn filters(&self, t: &Theme) -> impl IntoElement + use<> {
        let chips = FILTERS
            .into_iter()
            .enumerate()
            .map(|(index, (filter, id))| {
                let on_event = self.on_event.clone();
                let is_on = self.view.filter == filter;
                chip(t, (index, tr(id)), is_on).on_click(move |_, window, cx| {
                    emit(on_event.as_ref(), BarEvent::Filter(filter), window, cx);
                })
            });
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xxs)
            .p(t.spacing.xxs)
            .rounded(t.radii.md)
            .bg(t.colors.rail)
            .shadow(inset_ring(t.colors.line))
            .children(chips)
    }

    fn totals(&self, t: &Theme) -> impl IntoElement + use<> {
        let totals = self.view.totals.iter().map(|total| {
            div()
                .flex_none()
                .text_color(tone::text(t, total.tone))
                .child(SharedString::from(total.text.clone()))
        });
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(t.spacing.lg)
            .overflow_hidden()
            .font_family(t.typography.numbers_font.clone())
            .font_features(t.typography.numbers_font_features.clone())
            .text_size(t.typography.small)
            .children(totals)
            .children(self.view.running.clone().map(|text| {
                div()
                    .flex_none()
                    .text_color(t.colors.accent_text)
                    .child(SharedString::from(text))
            }))
    }

    /// The calm notice of a walk that waits at the network limit, with its Continue button.
    fn stopped(&self, t: &Theme, text: &str) -> impl IntoElement + use<> {
        let on_event = self.on_event.clone();
        let button = Button::new("compare-continue", tr("compare-continue"))
            .variant(ButtonVariant::Ghost)
            .tooltip(tr("compare-continue-tip"))
            .on_click(move |_, window, cx| {
                emit(on_event.as_ref(), BarEvent::Continue, window, cx);
            });
        div()
            .id("compare-stopped")
            .role(Role::Status)
            .aria_label(text.to_owned())
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xl)
            .px(t.spacing.xl)
            .pb(t.spacing.xs)
            .text_size(t.typography.small)
            .text_color(t.colors.accent_text)
            .child(SharedString::from(text.to_owned()))
            .child(button)
            .children(Kbd::new("compare-continue-key", "ctrl-f7"))
    }

    fn leave(&self) -> impl IntoElement + use<> {
        let on_event = self.on_event.clone();
        let button = Button::new("compare-leave", tr("compare-leave"))
            .variant(ButtonVariant::Ghost)
            .icon(IconName::X)
            .tooltip(tr("compare-leave-tip"))
            .on_click(move |_, window, cx| {
                emit(on_event.as_ref(), BarEvent::Leave, window, cx);
            });
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .child(button)
            .children(Kbd::new("compare-leave-key", "escape"))
    }
}

fn emit(handler: Option<&BarHandler>, event: BarEvent, window: &mut Window, cx: &mut App) {
    if let Some(handler) = handler {
        handler(&event, window, cx);
    }
}

fn chip(
    t: &Theme,
    (index, label): (usize, String),
    is_on: bool,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let c = &t.colors;
    div()
        .id(("compare-filter", index))
        .role(Role::Button)
        .aria_label(label.clone())
        .flex()
        .items_center()
        .h(px(22.))
        .px(t.spacing.md)
        .rounded(t.radii.sm)
        .cursor_pointer()
        .text_size(t.typography.small)
        .text_color(if is_on { c.text } else { c.muted })
        .when(is_on, |d| {
            d.bg(t.surfaces.card)
                .font_weight(FontWeight::SEMIBOLD)
                .shadow(inset_ring(c.line))
        })
        .when(!is_on, |d| d.hover(|d| d.bg(c.hover)))
        .child(SharedString::from(label))
}

impl RenderOnce for CompareBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let row = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xl)
            .min_h(t.spacing.header_height)
            .px(t.spacing.xl)
            .child(
                div()
                    .text_size(t.typography.small)
                    .child(tr("compare-filter-label")),
            )
            .child(self.filters(&t))
            .child(self.totals(&t))
            .child(self.leave());
        div()
            .id(self.id.clone())
            .role(Role::Group)
            .aria_label(tr("compare-filter-label"))
            .flex()
            .flex_col()
            .flex_none()
            .mx(t.spacing.xs)
            .text_color(t.colors.muted)
            .child(row)
            .children(
                self.view
                    .stopped
                    .as_deref()
                    .map(|text| self.stopped(&t, text)),
            )
    }
}
